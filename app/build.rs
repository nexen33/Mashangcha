use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

// =========================================================================
// 开发者与 Windows 二进制属性（.exe 详细信息）配置区
// 【请在下方双引号中直接填写你的开发者与版权信息，留空则不显示】
// =========================================================================
const DEVELOPER_NAME: &str = "Tun&PaMa AG"; // 开发者 / 公司名称（Windows 属性里的“公司”）
const LEGAL_COPYRIGHT: &str = "Copyright (C) 2026 Tun&PaMa AG. All rights reserved."; // 版权署名文本（Windows 属性里的“版权”，例如："Copyright (C) 2026 YourName. All rights reserved."）
const PRODUCT_NAME: &str = "码上查"; // 产品对外名称（Windows 属性里的“产品名称”）
const FILE_DESCRIPTION: &str = "码上查 - 进销项企业编码与税号外挂检索工具"; // 文件功能说明
const FILE_VERSION: &str = "1.0.0.0"; // 文件版本号 (X.X.X.X)
const PRODUCT_VERSION: &str = "1.0.0.0"; // 产品版本号 (X.X.X.X)
// =========================================================================

fn write_res_record(
    out: &mut Vec<u8>,
    type_id: u16,
    name_id: u16,
    memory_flags: u16,
    language_id: u16,
    payload: &[u8],
) {
    let data_size = payload.len() as u32;
    let header_size: u32 = 32;
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&header_size.to_le_bytes());
    // TYPE ordinal: 0xFFFF followed by u16 type_id
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&type_id.to_le_bytes());
    // NAME ordinal: 0xFFFF followed by u16 name_id
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&name_id.to_le_bytes());
    // DataVersion (u32 = 0)
    out.extend_from_slice(&0u32.to_le_bytes());
    // MemoryFlags (u16)
    out.extend_from_slice(&memory_flags.to_le_bytes());
    // LanguageId (u16)
    out.extend_from_slice(&language_id.to_le_bytes());
    // Version (u32 = 0)
    out.extend_from_slice(&0u32.to_le_bytes());
    // Characteristics (u32 = 0)
    out.extend_from_slice(&0u32.to_le_bytes());

    out.extend_from_slice(payload);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn encode_version_node(
    key: &str,
    is_text: bool,
    val_bytes: &[u8],
    val_len_units: u16,
    children: &[u8],
) -> Vec<u8> {
    let mut node = Vec::new();
    node.extend_from_slice(&[0u8; 6]); // placeholder for wLength, wValueLength, wType

    for c in key.encode_utf16().chain(std::iter::once(0)) {
        node.extend_from_slice(&c.to_le_bytes());
    }

    while node.len() % 4 != 0 {
        node.push(0);
    }

    node.extend_from_slice(val_bytes);

    while node.len() % 4 != 0 {
        node.push(0);
    }

    node.extend_from_slice(children);

    let total_len = node.len() as u16;
    let w_type: u16 = if is_text { 1 } else { 0 };

    node[0..2].copy_from_slice(&total_len.to_le_bytes());
    node[2..4].copy_from_slice(&val_len_units.to_le_bytes());
    node[4..6].copy_from_slice(&w_type.to_le_bytes());

    node
}

fn encode_string_entry(key: &str, val: &str) -> Vec<u8> {
    let mut val_bytes = Vec::new();
    let mut wchars = 0u16;
    for c in val.encode_utf16().chain(std::iter::once(0)) {
        val_bytes.extend_from_slice(&c.to_le_bytes());
        wchars += 1;
    }
    encode_version_node(key, true, &val_bytes, wchars, &[])
}

fn build_version_info() -> Vec<u8> {
    let mut string_entries = Vec::new();

    let entries = [
        ("CompanyName", DEVELOPER_NAME),
        ("FileDescription", FILE_DESCRIPTION),
        ("FileVersion", FILE_VERSION),
        ("InternalName", "mashangcha.exe"),
        ("LegalCopyright", LEGAL_COPYRIGHT),
        ("OriginalFilename", "mashangcha.exe"),
        ("ProductName", PRODUCT_NAME),
        ("ProductVersion", PRODUCT_VERSION),
    ];

    for (k, v) in entries {
        let entry = encode_string_entry(k, v);
        string_entries.extend_from_slice(&entry);
        while string_entries.len() % 4 != 0 {
            string_entries.push(0);
        }
    }

    let string_table = encode_version_node("080404b0", true, &[], 0, &string_entries);
    let string_file_info = encode_version_node("StringFileInfo", true, &[], 0, &string_table);

    // VarFileInfo: Translation (0x0804 = zh-CN, 0x04b0 = 1200 Unicode)
    let trans_bytes = [0x04u8, 0x08u8, 0xb0u8, 0x04u8];
    let var_entry = encode_version_node("Translation", false, &trans_bytes, 4, &[]);
    let var_file_info = encode_version_node("VarFileInfo", true, &[], 0, &var_entry);

    let mut children = Vec::new();
    children.extend_from_slice(&string_file_info);
    while children.len() % 4 != 0 {
        children.push(0);
    }
    children.extend_from_slice(&var_file_info);

    // VS_FIXEDFILEINFO (52 bytes)
    let mut fixed_info = Vec::with_capacity(52);
    fixed_info.extend_from_slice(&0xFEEF04BDu32.to_le_bytes()); // dwSignature
    fixed_info.extend_from_slice(&0x00010000u32.to_le_bytes()); // dwStrucVersion
    fixed_info.extend_from_slice(&0x00010000u32.to_le_bytes()); // dwFileVersionMS (1.0)
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwFileVersionLS (0.0)
    fixed_info.extend_from_slice(&0x00010000u32.to_le_bytes()); // dwProductVersionMS (1.0)
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwProductVersionLS (0.0)
    fixed_info.extend_from_slice(&0x0000003Fu32.to_le_bytes()); // dwFileFlagsMask
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwFileFlags
    fixed_info.extend_from_slice(&0x00040004u32.to_le_bytes()); // dwFileOS (VOS_NT_WINDOWS32)
    fixed_info.extend_from_slice(&0x00000001u32.to_le_bytes()); // dwFileType (VFT_APP)
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwFileSubtype
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwFileDateMS
    fixed_info.extend_from_slice(&0x00000000u32.to_le_bytes()); // dwFileDateLS
    assert_eq!(fixed_info.len(), 52);

    encode_version_node("VS_VERSION_INFO", false, &fixed_info, 52, &children)
}

fn compile_ico_to_res(ico_path: &Path, res_path: &Path) {
    let ico = fs::read(ico_path).expect("Failed to read app.ico");
    assert!(ico.len() >= 6, "Invalid .ico header");
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert!(ico.len() >= 6 + count * 16, "Truncated .ico directory");

    let mut res = Vec::with_capacity(ico.len() + 1024);
    // 1. 32-byte Win32 .res file magic header (empty record)
    write_res_record(&mut res, 0, 0, 0, 0, &[]);

    // 2. Build GRPICONDIR payload & emit individual RT_ICON (type = 3) records
    let mut grp_dir = Vec::with_capacity(6 + count * 14);
    grp_dir.extend_from_slice(&ico[0..6]);

    for i in 0..count {
        let entry_offset = 6 + i * 16;
        let entry = &ico[entry_offset..entry_offset + 16];
        let bytes_in_res = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let img_offset =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let icon_id = (i + 1) as u16;

        let img_data = &ico[img_offset..img_offset + bytes_in_res];
        // RT_ICON = 3, MemoryFlags = 0x1010 (MOVEABLE | PURE), LanguageId = 0x0409 (en-US neutral)
        write_res_record(&mut res, 3, icon_id, 0x1010, 0x0409, img_data);

        // GRPICONDIRENTRY: first 12 bytes identical to ICONDIRENTRY, followed by u16 nID
        grp_dir.extend_from_slice(&entry[0..12]);
        grp_dir.extend_from_slice(&icon_id.to_le_bytes());
    }

    // 3. Emit RT_GROUP_ICON = 14 with Resource ID = 1 (primary executable icon)
    write_res_record(&mut res, 14, 1, 0x1030, 0x0409, &grp_dir);

    // 4. Emit RT_VERSION = 16 with Resource ID = 1 (Windows PE version & developer info)
    let version_payload = build_version_info();
    write_res_record(&mut res, 16, 1, 0x1030, 0x0804, &version_payload);

    let mut file = fs::File::create(res_path).expect("Failed to create .res file");
    file.write_all(&res).expect("Failed to write .res file");
}

fn main() {
    println!("cargo:rerun-if-changed=ui/main.slint");
    println!("cargo:rerun-if-changed=ui/icons/app.ico");
    println!("cargo:rerun-if-changed=ui/icons/app-icon.png");

    slint_build::compile("ui/main.slint").expect("Failed to compile Slint UI resources");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico_path = manifest_dir.join("ui").join("icons").join("app.ico");
    let res_path = out_dir.join("app_icon.res");

    compile_ico_to_res(&ico_path, &res_path);
    println!("cargo:rustc-link-arg-bins={}", res_path.display());
}
