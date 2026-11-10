use serde::{Deserialize, Serialize};

/// 进销项业务数据分类枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabCategory {
    /// 进项分类（对应持久化文件 jinxiang.dat）
    Jinxiang = 0,
    /// 销项分类（对应持久化文件 xiaoxiang.dat）
    Xiaoxiang = 1,
}

impl TabCategory {
    /// 从整型索引转换为分类枚举
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => TabCategory::Xiaoxiang,
            _ => TabCategory::Jinxiang,
        }
    }

    /// 转换为 UI 绑定的整型索引
    pub fn as_index(self) -> i32 {
        match self {
            TabCategory::Jinxiang => 0,
            TabCategory::Xiaoxiang => 1,
        }
    }

    /// 获取当前分类对应的加密存储文件名
    pub fn storage_filename(self) -> &'static str {
        match self {
            TabCategory::Jinxiang => "jinxiang.dat",
            TabCategory::Xiaoxiang => "xiaoxiang.dat",
        }
    }

    /// 获取分类中文显示名称
    pub fn display_name(self) -> &'static str {
        match self {
            TabCategory::Jinxiang => "进项",
            TabCategory::Xiaoxiang => "销项",
        }
    }
}

/// 界面三档字号配置枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontSizeTier {
    /// 小字号（主名称 16px，编号/税号 14px，单行高 60px）
    Small = 0,
    /// 中字号（主名称 18px，编号/税号 15px，单行高 66px，默认）
    Medium = 1,
    /// 大字号（主名称 20px，编号/税号 16px，单行高 72px）
    Large = 2,
}

impl FontSizeTier {
    /// 从整型索引转换为字号档位
    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => FontSizeTier::Small,
            2 => FontSizeTier::Large,
            _ => FontSizeTier::Medium,
        }
    }

    /// 转换为 UI 绑定的整型索引
    pub fn as_index(self) -> i32 {
        match self {
            FontSizeTier::Small => 0,
            FontSizeTier::Medium => 1,
            FontSizeTier::Large => 2,
        }
    }
}

/// 双轨硬件指纹绑定模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    /// 默认主机模式（绑定 MachineGuid + 系统盘卷序列号）
    Default = 0,
    /// 便携 U 盘模式（绑定可执行文件所在盘符硬件卷序列号）
    Portable = 1,
}

impl KeyMode {
    /// 从字节标识解析模式
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(KeyMode::Default),
            1 => Some(KeyMode::Portable),
            _ => None,
        }
    }
}

/// 应用全局偏好设置（加密持久化于 config.dat）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// 启动时默认激活的分类
    pub default_tab: TabCategory,
    /// 是否开启窗口置顶（HWND_TOPMOST）
    pub is_topmost: bool,
    /// 当前字号档位
    pub font_tier: FontSizeTier,
    /// 是否允许点击企业名称直接复制企业名称（默认 false 为禁用，点击企业名称复制编号）
    #[serde(default)]
    pub allow_copy_name: bool,
    /// 用户自定义窗口宽度（逻辑像素，None 时取当前字号档位的默认值）
    #[serde(default)]
    pub custom_width: Option<u32>,
    /// 用户自定义窗口高度（逻辑像素，None 时取当前字号档位 3 行的默认值）
    #[serde(default)]
    pub custom_height: Option<u32>,
    /// 是否在未搜索时展示所有企业（默认 false 为禁用，未搜索时显示检索占位提示）
    #[serde(default)]
    pub show_all_enterprises: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_tab: TabCategory::Jinxiang,
            is_topmost: true,
            font_tier: FontSizeTier::Medium,
            allow_copy_name: false,
            custom_width: None,
            custom_height: None,
            show_all_enterprises: false,
        }
    }
}

/// 单条企业检索记录（5 个紧凑字段，复用原 ASCII 编码执行忽略大小写匹配）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// 清洗后的标准展示企业名称
    pub name: String,
    /// 清洗后的纯文本企业编号（第二列）
    pub code: String,
    /// 清洗后的可选统一社会信用代码/税号（第三列，无则为空字符串）
    pub tax_id: String,
    /// 预计算的 NFKC 小写无空格名称检索键
    pub search_name_nfkc: String,
    /// 预计算的名称汉字拼音首字母小写简码
    pub pinyin_initials: String,
}

/// 单个分类下的全量数据集
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dataset {
    /// 企业记录列表
    pub entries: Vec<Entry>,
    /// 最近一次更新的 Unix 时间戳（秒）
    pub updated_unix_secs: u64,
}

/// 导入查重冲突字段类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictField {
    /// 企业名称重复
    Name,
    /// 企业编号重复
    Code,
}

impl ConflictField {
    /// 获取冲突字段中文标签
    pub fn label(self) -> &'static str {
        match self {
            ConflictField::Name => "企业名称",
            ConflictField::Code => "企业编号",
        }
    }
}

/// 导入前置查重冲突明细项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateConflict {
    /// 发生重复的字段类型
    pub field: ConflictField,
    /// 首次出现的 1-based 物理行号
    pub first_row: usize,
    /// 重复出现的 1-based 物理行号
    pub duplicate_row: usize,
    /// 发生冲突的清洗后字段值
    pub conflicting_value: String,
}

