use crate::clean::{
    build_entry, clean_code_field, clean_company_name, is_cjk_or_fullwidth_punct,
    is_scientific_notation_corrupted,
};
use crate::model::{ConflictField, Dataset, DuplicateConflict, Entry};
use calamine::{open_workbook_auto, Data, Reader, Xlsx};
use encoding_rs::GB18030;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;

/// 单次导入允许的最大非空行数上限
pub const MAX_IMPORT_ROWS: usize = 100_000;

/// 表头扫描最大前序非空行数
const HEADER_SCAN_LIMIT: usize = 10;

/// 税号列特征词库（最高优先级匹配）
const TAX_HEADER_KEYWORDS: &[&str] = &[
    "统一社会信用代码",
    "纳税人识别号",
    "社会信用代码",
    "信用代码",
    "税号",
    "税友编码",
];

/// 企业编号列特征词库（次高优先级匹配）
const CODE_HEADER_KEYWORDS: &[&str] = &[
    "企业编号",
    "客户编码",
    "供应商编码",
    "客户代码",
    "供应商代码",
    "企业代码",
    "往来编码",
    "编号",
    "编码",
    "代码",
    "自编码",
];

/// 企业名称列特征词库（最后匹配，防止“客户编码”被“客户”抢先命中）
const NAME_HEADER_KEYWORDS: &[&str] = &[
    "企业名称",
    "客户名称",
    "供应商名称",
    "购方名称",
    "销方名称",
    "往来单位",
    "单位名称",
    "公司名称",
    "客户",
    "供应商",
    "名称",
    "抬头",
];

/// 导入与前置校验错误枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// 文件读取失败
    IoError(String),
    /// 表格格式解析失败
    ParseError(String),
    /// 表格无任何有效数据行
    EmptyDataset,
    /// 非空有效行数超出安全上限
    TooManyRows { max_allowed: usize },
    /// 检测到编号或税号发生 Excel 科学计数法精度丢失截断
    ScientificNotationDetected { row: usize, raw_value: String },
    /// 必填字段（企业名称或企业编号）缺失为空
    MissingRequiredField {
        row: usize,
        field_name: &'static str,
    },
    /// 存在重复的企业名称或重复的企业编号（拒绝导入并返回全量冲突明细）
    DuplicateRecords {
        total_conflicts: usize,
        conflicts: Vec<DuplicateConflict>,
    },
}

/// 原始非空数据行及其在源表格中的 1-based 物理行号
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRow {
    /// 源文件中的 1-based 物理行号
    pub physical_row: usize,
    /// 该行各列原始文本
    pub cells: Vec<String>,
}

/// 动态识别或回退后的列索引映射
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnMapping {
    /// 识别到的表头在 `rows` 切片中的索引（`None` 表示无表头，从 `rows[0]` 直接导入）
    pub header_row_index: Option<usize>,
    /// 企业名称列索引（0-based）
    pub name_col: usize,
    /// 企业编号列索引（0-based）
    pub code_col: usize,
    /// 可选的统一社会信用代码/税号列索引（0-based）
    pub tax_col: Option<usize>,
}

/// 剥离表头单元格中的常见括号注释与必填星号修饰（如 `"企业编号(必填)*"` -> `"企业编号"`）
fn normalize_header_cell(raw: &str) -> String {
    let cleaned = clean_company_name(raw).to_lowercase();
    let mut base = cleaned.as_str();
    if let Some(idx) = base.find('(') {
        base = &base[..idx];
    }
    if let Some(idx) = base.find(':') {
        base = &base[..idx];
    }
    if let Some(idx) = base.find('：') {
        base = &base[..idx];
    }
    base.trim_matches(|c: char| c == '*' || c.is_whitespace())
        .to_string()
}

/// 在单行标准化表头单元格中按给定词库查找最佳匹配列（优先精确相等，仅当单元格字符数 <= 8 时允许子串包含）
fn find_matching_column(
    norm_cells: &[String],
    keywords: &[&str],
    used_cols: &[usize],
) -> Option<usize> {
    // 第一轮：精确相等匹配
    for &kw in keywords {
        for (col_idx, cell) in norm_cells.iter().enumerate() {
            if used_cols.contains(&col_idx) || cell.is_empty() {
                continue;
            }
            if cell == kw {
                return Some(col_idx);
            }
        }
    }

    // 第二轮：短标题（<= 8 个字符）包含匹配
    for &kw in keywords {
        for (col_idx, cell) in norm_cells.iter().enumerate() {
            if used_cols.contains(&col_idx) || cell.is_empty() {
                continue;
            }
            let char_count = cell.chars().count();
            if char_count <= 8 && cell.contains(kw) {
                return Some(col_idx);
            }
        }
    }

    None
}

/// 扫描前 10 个非空行动态识别表头行与列索引；若未命中则回退固定前 3 列并智能判定首行是否为非标表头
pub fn detect_columns(rows: &[RawRow]) -> ColumnMapping {
    let scan_count = rows.len().min(HEADER_SCAN_LIMIT);

    for (row_idx, row) in rows.iter().take(scan_count).enumerate() {
        let norm_cells: Vec<String> = row
            .cells
            .iter()
            .map(|c| {
                let n = normalize_header_cell(c);
                if n.chars().count() <= 20 {
                    n
                } else {
                    String::new()
                }
            })
            .collect();

        let mut used = Vec::with_capacity(3);

        // 1. 最高优先级匹配税号列
        let tax_col = find_matching_column(&norm_cells, TAX_HEADER_KEYWORDS, &used);
        if let Some(tc) = tax_col {
            used.push(tc);
        }

        // 2. 次高优先级匹配编号列
        let code_col = find_matching_column(&norm_cells, CODE_HEADER_KEYWORDS, &used);
        if let Some(cc) = code_col {
            used.push(cc);
        }

        // 3. 最后匹配名称列
        let name_col = find_matching_column(&norm_cells, NAME_HEADER_KEYWORDS, &used);

        if let (Some(nc), Some(cc)) = (name_col, code_col) {
            if nc != cc {
                return ColumnMapping {
                    header_row_index: Some(row_idx),
                    name_col: nc,
                    code_col: cc,
                    tax_col,
                };
            }
        }
    }

    // 回退至固定前 3 列，并检查首行第 2 列（索引 1）是否包含汉字以判定是否跳过首行
    let header_row_index = if let Some(first_row) = rows.first() {
        let col1_text = first_row.cells.get(1).map(|s| s.as_str()).unwrap_or("");
        if col1_text.chars().any(is_cjk_or_fullwidth_punct) {
            Some(0)
        } else {
            None
        }
    } else {
        None
    };

    ColumnMapping {
        header_row_index,
        name_col: 0,
        code_col: 1,
        tax_col: Some(2),
    }
}

/// 对非空原始行执行表头定位、科学计数法拦截、深度字符清洗与严格前置查重校验
pub fn parse_and_validate_rows(
    rows: Vec<RawRow>,
    now_unix_secs: u64,
) -> Result<(Dataset, ColumnMapping), ImportError> {
    if rows.is_empty() {
        return Err(ImportError::EmptyDataset);
    }
    if rows.len() > MAX_IMPORT_ROWS {
        return Err(ImportError::TooManyRows {
            max_allowed: MAX_IMPORT_ROWS,
        });
    }

    let mapping = detect_columns(&rows);
    let data_start_idx = mapping.header_row_index.map(|i| i + 1).unwrap_or(0);
    if data_start_idx >= rows.len() {
        return Err(ImportError::EmptyDataset);
    }

    let data_slice = &rows[data_start_idx..];
    let mut entries: Vec<Entry> = Vec::with_capacity(data_slice.len());
    let mut seen_names: HashMap<String, usize> = HashMap::with_capacity(data_slice.len());
    let mut seen_codes: HashMap<String, usize> = HashMap::with_capacity(data_slice.len());
    let mut conflicts: Vec<DuplicateConflict> = Vec::new();

    for row in data_slice {
        let raw_name = row
            .cells
            .get(mapping.name_col)
            .map(|s| s.as_str())
            .unwrap_or("");
        let raw_code = row
            .cells
            .get(mapping.code_col)
            .map(|s| s.as_str())
            .unwrap_or("");
        let raw_tax = mapping
            .tax_col
            .and_then(|col| row.cells.get(col))
            .map(|s| s.as_str());

        let cleaned_code = clean_code_field(raw_code);
        if is_scientific_notation_corrupted(&cleaned_code) {
            return Err(ImportError::ScientificNotationDetected {
                row: row.physical_row,
                raw_value: cleaned_code,
            });
        }

        if let Some(rt) = raw_tax {
            let cleaned_tax = clean_code_field(rt);
            if !cleaned_tax.is_empty() && is_scientific_notation_corrupted(&cleaned_tax) {
                return Err(ImportError::ScientificNotationDetected {
                    row: row.physical_row,
                    raw_value: cleaned_tax,
                });
            }
        }

        let entry = build_entry(raw_name, raw_code, raw_tax);
        if entry.name.is_empty() && entry.code.is_empty() {
            continue;
        }
        if entry.name.is_empty() {
            return Err(ImportError::MissingRequiredField {
                row: row.physical_row,
                field_name: "企业名称",
            });
        }
        if entry.code.is_empty() {
            return Err(ImportError::MissingRequiredField {
                row: row.physical_row,
                field_name: "企业编号",
            });
        }

        if let Some(&first_row) = seen_names.get(&entry.search_name_nfkc) {
            conflicts.push(DuplicateConflict {
                field: ConflictField::Name,
                first_row,
                duplicate_row: row.physical_row,
                conflicting_value: entry.name.clone(),
            });
        } else {
            seen_names.insert(entry.search_name_nfkc.clone(), row.physical_row);
        }

        let code_key = entry.code.to_ascii_lowercase();
        if let Some(&first_row) = seen_codes.get(&code_key) {
            conflicts.push(DuplicateConflict {
                field: ConflictField::Code,
                first_row,
                duplicate_row: row.physical_row,
                conflicting_value: entry.code.clone(),
            });
        } else {
            seen_codes.insert(code_key, row.physical_row);
        }

        entries.push(entry);
    }

    if !conflicts.is_empty() {
        return Err(ImportError::DuplicateRecords {
            total_conflicts: conflicts.len(),
            conflicts,
        });
    }

    if entries.is_empty() {
        return Err(ImportError::EmptyDataset);
    }

    Ok((
        Dataset {
            entries,
            updated_unix_secs: now_unix_secs,
        },
        mapping,
    ))
}

/// 将 `calamine::Data` 转换为字符串，对 `!f.is_finite() || f.fract() != 0.0 || f.abs() >= 1e15` 输出 `E+` 标记以便在编号/税号列精准拦截
fn format_calamine_cell(dt: &Data) -> String {
    match dt {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
                (*f as i64).to_string()
            } else {
                let s = format!("{:e}", f);
                if let Some((mantissa, exp)) = s.split_once('e') {
                    let exp_num: i32 = exp.parse().unwrap_or(99);
                    if exp_num >= 10 {
                        return format!("{}E+{}", mantissa, exp_num);
                    }
                }
                format!("{}E+99", f)
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(_) => String::new(),
    }
}

/// 判断一行单元格是否全部为空白
fn is_row_completely_blank(cells: &[String]) -> bool {
    cells.iter().all(|c| clean_code_field(c).is_empty())
}

/// 从 `calamine::Range<Data>` 流式提取非空行并执行行数上限保护
fn extract_rows_from_range(range: &calamine::Range<Data>) -> Result<Vec<RawRow>, ImportError> {
    let start_row_offset = range.start().map(|(r, _)| r as usize).unwrap_or(0);
    let mut rows = Vec::new();

    for (idx, row_slice) in range.rows().enumerate() {
        let cells: Vec<String> = row_slice.iter().map(format_calamine_cell).collect();
        if is_row_completely_blank(&cells) {
            continue;
        }
        rows.push(RawRow {
            physical_row: start_row_offset + idx + 1,
            cells,
        });
        if rows.len() > MAX_IMPORT_ROWS {
            return Err(ImportError::TooManyRows {
                max_allowed: MAX_IMPORT_ROWS,
            });
        }
    }

    Ok(rows)
}

/// 从内存 CSV 字节流（自动探测 UTF-8 / UTF-8 BOM 与 GB18030）导入并校验数据集
pub fn import_csv_bytes(
    bytes: &[u8],
    now_unix_secs: u64,
) -> Result<(Dataset, ColumnMapping), ImportError> {
    let raw_bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let decoded_cow = match std::str::from_utf8(raw_bytes) {
        Ok(utf8_str) => std::borrow::Cow::Borrowed(utf8_str),
        Err(_) => {
            let (cow, _, _) = GB18030.decode(raw_bytes);
            cow
        }
    };

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(decoded_cow.as_bytes());

    let mut rows = Vec::new();
    for (idx, record_res) in rdr.records().enumerate() {
        let record = record_res.map_err(|e| ImportError::ParseError(e.to_string()))?;
        let cells: Vec<String> = record.iter().map(|s| s.to_string()).collect();
        if is_row_completely_blank(&cells) {
            continue;
        }
        rows.push(RawRow {
            physical_row: idx + 1,
            cells,
        });
        if rows.len() > MAX_IMPORT_ROWS {
            return Err(ImportError::TooManyRows {
                max_allowed: MAX_IMPORT_ROWS,
            });
        }
    }

    parse_and_validate_rows(rows, now_unix_secs)
}

/// 从内存 `.xlsx` 字节流导入并校验数据集
pub fn import_xlsx_bytes(
    bytes: &[u8],
    now_unix_secs: u64,
) -> Result<(Dataset, ColumnMapping), ImportError> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        Xlsx::new(cursor).map_err(|e| ImportError::ParseError(e.to_string()))?;
    let sheet_names = workbook.sheet_names().to_owned();
    let Some(first_sheet) = sheet_names.first() else {
        return Err(ImportError::EmptyDataset);
    };
    let range = workbook
        .worksheet_range(first_sheet)
        .map_err(|e| ImportError::ParseError(e.to_string()))?;

    let rows = extract_rows_from_range(&range)?;
    parse_and_validate_rows(rows, now_unix_secs)
}

/// 从磁盘文件路径（支持 `.xlsx`、`.xls`、`.csv`）流式读取、识别表头并执行前置严格查重导入
pub fn import_from_path(
    path: &Path,
    now_unix_secs: u64,
) -> Result<(Dataset, ColumnMapping), ImportError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "csv" {
        let bytes = fs::read(path).map_err(|e| ImportError::IoError(e.to_string()))?;
        return import_csv_bytes(&bytes, now_unix_secs);
    }

    let mut workbook =
        open_workbook_auto(path).map_err(|e| ImportError::ParseError(e.to_string()))?;
    let sheet_names = workbook.sheet_names().to_owned();
    let Some(first_sheet) = sheet_names.first() else {
        return Err(ImportError::EmptyDataset);
    };
    let range = workbook
        .worksheet_range(first_sheet)
        .map_err(|e| ImportError::ParseError(e.to_string()))?;

    let rows = extract_rows_from_range(&range)?;
    parse_and_validate_rows(rows, now_unix_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_header_detection_priority_within_10_rows() {
        let rows = vec![
            RawRow {
                physical_row: 1,
                cells: vec!["2026年度往来客户台账汇总表".to_string()],
            },
            RawRow {
                physical_row: 2,
                cells: vec!["制表部门：财务部".to_string()],
            },
            RawRow {
                physical_row: 3,
                cells: vec![
                    "序号".to_string(),
                    "客户".to_string(),
                    "客户编码".to_string(),
                    "税友编码".to_string(),
                ],
            },
            RawRow {
                physical_row: 4,
                cells: vec![
                    "1".to_string(),
                    "中国银行上海分行".to_string(),
                    "100201".to_string(),
                    "91310000100016818X".to_string(),
                ],
            },
        ];

        let (ds, mapping) = parse_and_validate_rows(rows, 100).expect("should parse successfully");
        assert_eq!(mapping.header_row_index, Some(2));
        assert_eq!(mapping.name_col, 1);
        assert_eq!(mapping.code_col, 2);
        assert_eq!(mapping.tax_col, Some(3));
        assert_eq!(ds.entries.len(), 1);
        assert_eq!(ds.entries[0].name, "中国银行上海分行");
        assert_eq!(ds.entries[0].code, "100201");
        assert_eq!(ds.entries[0].tax_id, "91310000100016818X");
    }

    #[test]
    fn test_headerless_fallback_preserves_first_data_row() {
        let rows = vec![
            RawRow {
                physical_row: 1,
                cells: vec![
                    "上海华为技术有限公司".to_string(),
                    "HW-001".to_string(),
                    "91310115703298602B".to_string(),
                ],
            },
            RawRow {
                physical_row: 2,
                cells: vec![
                    "北京百度网讯科技有限公司".to_string(),
                    "BD-002".to_string(),
                    "".to_string(),
                ],
            },
        ];

        let (ds, mapping) = parse_and_validate_rows(rows, 100).expect("should parse headerless");
        assert_eq!(mapping.header_row_index, None);
        assert_eq!(ds.entries.len(), 2);
        assert_eq!(ds.entries[0].code, "HW-001");
        assert_eq!(ds.entries[1].tax_id, "");
    }

    #[test]
    fn test_duplicate_name_and_code_rejection() {
        let rows = vec![
            RawRow {
                physical_row: 1,
                cells: vec!["企业名称".to_string(), "企业编号".to_string()],
            },
            RawRow {
                physical_row: 2,
                cells: vec!["上海华为".to_string(), "1001".to_string()],
            },
            RawRow {
                physical_row: 5,
                cells: vec!["上海 华为\u{200B}".to_string(), "1002".to_string()],
            },
            RawRow {
                physical_row: 8,
                cells: vec!["腾讯科技".to_string(), "1001".to_string()],
            },
        ];

        let err = parse_and_validate_rows(rows, 100).unwrap_err();
        match err {
            ImportError::DuplicateRecords {
                total_conflicts,
                conflicts,
            } => {
                assert_eq!(total_conflicts, 2);
                assert_eq!(
                    conflicts[0],
                    DuplicateConflict {
                        field: ConflictField::Name,
                        first_row: 2,
                        duplicate_row: 5,
                        conflicting_value: "上海华为".to_string(),
                    }
                );
                assert_eq!(
                    conflicts[1],
                    DuplicateConflict {
                        field: ConflictField::Code,
                        first_row: 2,
                        duplicate_row: 8,
                        conflicting_value: "1001".to_string(),
                    }
                );
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_float_scientific_notation_interception_and_gb18030_csv() {
        let cell_str = format_calamine_cell(&Data::Float(9.110108e17));
        let rows = vec![RawRow {
            physical_row: 3,
            cells: vec!["测试企业".to_string(), cell_str],
        }];
        let err = parse_and_validate_rows(rows, 100).unwrap_err();
        assert!(matches!(
            err,
            ImportError::ScientificNotationDetected { row: 3, .. }
        ));

        // 验证 GB18030 编码 CSV 解析与空行跳过
        let csv_utf8 = "企业名称,企业编号,税号\r\n\r\n中国银行,100201,91310000100016818X\r\n";
        let (gbk_bytes, _, _) = GB18030.encode(csv_utf8);
        let (ds, _) = import_csv_bytes(&gbk_bytes, 200).expect("GB18030 CSV should parse");
        assert_eq!(ds.entries.len(), 1);
        assert_eq!(ds.entries[0].name, "中国银行");
        assert_eq!(ds.entries[0].code, "100201");
    }
}
