use crate::model::KeyMode;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

/// 加密容器文件头魔数（4 字节）
pub const FILE_MAGIC: &[u8; 4] = b"MSC1";

/// 加密容器结构版本号（2 字节小端）
pub const SCHEMA_VERSION: u16 = 1;

/// 内置 32 字节固定盐值
pub const BUILTIN_OBFUSCATED_SALT: [u8; 32] = *b"MSC1_2026_WIN7_STATIC_SALT_KEY!!";

/// 7 字节 AAD 头部长度 + 12 字节 Nonce + 16 字节 Poly1305 Tag = 最小密文长度 35 字节
const HEADER_LEN: usize = 7;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// 加解密与容器校验错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// 文件读写错误
    IoError(String),
    /// 二进制序列化或反序列化失败
    SerializationError(String),
    /// 系统随机数生成器调用失败
    RngError(String),
    /// 文件头魔数、版本号或长度非法
    InvalidHeader,
    /// 文件绑定的运行模式（默认 vs 便携）与当前运行模式不匹配
    ModeMismatch { expected: KeyMode, found: KeyMode },
    /// AEAD 认证标签校验失败（硬件环境变更或密文被篡改）
    AuthenticationFailed,
}

/// 基于双轨硬件指纹派生 256-bit 对称密钥
/// - 默认模式：`Salt || [0u8] || MachineGuid || SystemDriveVolumeSerial`
/// - 便携模式：`Salt || [1u8] || "" || UsbDriveVolumeSerial`
pub fn derive_key(mode: KeyMode, machine_guid: Option<&str>, volume_serial: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(BUILTIN_OBFUSCATED_SALT);
    hasher.update([mode as u8]);
    if mode == KeyMode::Default {
        hasher.update(machine_guid.unwrap_or("").trim().as_bytes());
    }
    hasher.update(volume_serial.to_le_bytes());

    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// 构造 7 字节关联认证数据（AAD）文件头
#[inline]
fn build_aad_header(mode: KeyMode) -> [u8; HEADER_LEN] {
    let ver = SCHEMA_VERSION.to_le_bytes();
    [
        FILE_MAGIC[0],
        FILE_MAGIC[1],
        FILE_MAGIC[2],
        FILE_MAGIC[3],
        ver[0],
        ver[1],
        mode as u8,
    ]
}

/// 将明文字节切片加密为带 7 字节 AAD 头部与 12 字节 Nonce 的容器字节流
pub fn encrypt_bytes(
    plaintext: &[u8],
    key: &[u8; 32],
    mode: KeyMode,
    nonce_bytes: [u8; NONCE_LEN],
) -> Result<Vec<u8>, CryptoError> {
    let header = build_aad_header(mode);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)?;

    let mut out = Vec::with_capacity(HEADER_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// 校验 7 字节 AAD 头部并解密容器字节流
pub fn decrypt_bytes(
    blob: &[u8],
    key: &[u8; 32],
    expected_mode: KeyMode,
) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < HEADER_LEN + NONCE_LEN + TAG_LEN {
        return Err(CryptoError::InvalidHeader);
    }
    if &blob[0..4] != FILE_MAGIC {
        return Err(CryptoError::InvalidHeader);
    }
    let version = u16::from_le_bytes([blob[4], blob[5]]);
    if version != SCHEMA_VERSION {
        return Err(CryptoError::InvalidHeader);
    }
    let Some(found_mode) = KeyMode::from_byte(blob[6]) else {
        return Err(CryptoError::InvalidHeader);
    };
    if found_mode != expected_mode {
        return Err(CryptoError::ModeMismatch {
            expected: expected_mode,
            found: found_mode,
        });
    }

    let header = &blob[0..HEADER_LEN];
    let nonce = Nonce::from_slice(&blob[HEADER_LEN..HEADER_LEN + NONCE_LEN]);
    let ciphertext = &blob[HEADER_LEN + NONCE_LEN..];

    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: header,
            },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)
}

/// 将任意可序列化结构通过 bincode 编码并生成随机 96-bit Nonce 封装为加密字节流
pub fn seal_value<T: Serialize>(
    val: &T,
    key: &[u8; 32],
    mode: KeyMode,
) -> Result<Vec<u8>, CryptoError> {
    let plaintext =
        bincode::serialize(val).map_err(|e| CryptoError::SerializationError(e.to_string()))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| CryptoError::RngError(e.to_string()))?;
    encrypt_bytes(&plaintext, key, mode, nonce_bytes)
}

/// 验证并解密字节流后反序列化还原为目标结构体
pub fn open_value<T: DeserializeOwned>(
    blob: &[u8],
    key: &[u8; 32],
    expected_mode: KeyMode,
) -> Result<T, CryptoError> {
    let plaintext = decrypt_bytes(blob, key, expected_mode)?;
    bincode::deserialize(&plaintext).map_err(|e| CryptoError::SerializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::build_entry;
    use crate::model::Dataset;

    #[test]
    fn test_dual_track_key_derivation_isolation() {
        let k_default_1 = derive_key(KeyMode::Default, Some("GUID-AAA"), 0x1234_5678);
        let k_default_2 = derive_key(KeyMode::Default, Some("GUID-BBB"), 0x1234_5678);
        let k_default_3 = derive_key(KeyMode::Default, Some("GUID-AAA"), 0x8765_4321);
        let k_portable = derive_key(KeyMode::Portable, Some("GUID-AAA"), 0x1234_5678);

        assert_ne!(k_default_1, k_default_2);
        assert_ne!(k_default_1, k_default_3);
        assert_ne!(k_default_1, k_portable);
    }

    #[test]
    fn test_seal_and_open_roundtrip() {
        let key = derive_key(KeyMode::Default, Some("MACHINE-01"), 0xAABB_CCDD);
        let ds = Dataset {
            entries: vec![build_entry(
                "中国银行上海分行",
                "100201",
                Some("91310000100016818X"),
            )],
            updated_unix_secs: 1700000000,
        };

        let sealed = seal_value(&ds, &key, KeyMode::Default).expect("seal should succeed");
        let opened: Dataset =
            open_value(&sealed, &key, KeyMode::Default).expect("open should succeed");
        assert_eq!(ds, opened);
    }

    #[test]
    fn test_aad_and_ciphertext_tamper_rejection() {
        let key = derive_key(KeyMode::Portable, None, 0xCAFE_BABE);
        let ds = Dataset::default();
        let mut sealed = seal_value(&ds, &key, KeyMode::Portable).expect("seal should succeed");

        // 模式不匹配检测
        let err_mode = open_value::<Dataset>(&sealed, &key, KeyMode::Default).unwrap_err();
        assert_eq!(
            err_mode,
            CryptoError::ModeMismatch {
                expected: KeyMode::Default,
                found: KeyMode::Portable
            }
        );

        // 篡改密文最后 1 字节
        let last_idx = sealed.len() - 1;
        sealed[last_idx] ^= 0x01;
        let err_auth = open_value::<Dataset>(&sealed, &key, KeyMode::Portable).unwrap_err();
        assert_eq!(err_auth, CryptoError::AuthenticationFailed);
    }
}

