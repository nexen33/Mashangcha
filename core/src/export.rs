use crate::model::Dataset;
use rust_xlsxwriter::{Format, Workbook};

/// UTF-8 BOM 字节序列
pub const UTF8_BOM: &[u8; 3] = &[0xEF, 0xBB, 0xBF];

/// 对普通 CSV 文本字段执行 RFC 4180 双引号转义
fn escape_csv_text(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\r') || val.contains('\n') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

/// 将当前分类数据集导出为带 UTF-8 BOM 与 `"\t{value}"` 纯文本保护前缀的 CSV 内存字节流
pub fn export_to_csv_bytes(dataset: &Dataset) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(dataset.entries.len() * 64 + 64);
    out.extend_from_slice(UTF8_BOM);
    out.extend_from_slice("企业名称,企业编号,统一社会信用代码\r\n".as_bytes());

    for entry in &dataset.entries {
        let name_col = escape_csv_text(&entry.name);
        let code_col = format!("\"\t{}\"", entry.code.replace('"', "\"\""));
        let tax_col = if entry.tax_id.is_empty() {
            String::new()
        } else {
            format!("\"\t{}\"", entry.tax_id.replace('"', "\"\""))
        };

        let line = format!("{},{},{}\r\n", name_col, code_col, tax_col);
        out.extend_from_slice(line.as_bytes());
    }

    Ok(out)
}

/// 将当前分类数据集导出为全列强制绑定 `@` 纯文本格式的 Excel (`.xlsx`) 内存字节流
pub fn export_to_xlsx_bytes(dataset: &Dataset) -> Result<Vec<u8>, String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    let header_format = Format::new().set_bold().set_num_format("@");
    let text_format = Format::new().set_num_format("@");

    worksheet
        .set_column_width(0, 36)
        .map_err(|e| e.to_string())?;
    worksheet
        .set_column_width(1, 20)
        .map_err(|e| e.to_string())?;
    worksheet
        .set_column_width(2, 24)
        .map_err(|e| e.to_string())?;

    worksheet
        .write_string_with_format(0, 0, "企业名称", &header_format)
        .map_err(|e| e.to_string())?;
    worksheet
        .write_string_with_format(0, 1, "企业编号", &header_format)
        .map_err(|e| e.to_string())?;
    worksheet
        .write_string_with_format(0, 2, "统一社会信用代码", &header_format)
        .map_err(|e| e.to_string())?;

    for (idx, entry) in dataset.entries.iter().enumerate() {
        let row = (idx + 1) as u32;
        worksheet
            .write_string_with_format(row, 0, &entry.name, &text_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(row, 1, &entry.code, &text_format)
            .map_err(|e| e.to_string())?;
        worksheet
            .write_string_with_format(row, 2, &entry.tax_id, &text_format)
            .map_err(|e| e.to_string())?;
    }

    workbook.save_to_buffer().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::build_entry;
    use crate::import::{import_csv_bytes, import_xlsx_bytes};

    #[test]
    fn test_csv_export_bom_and_lossless_reimport() {
        let dataset = Dataset {
            entries: vec![
                build_entry(
                    "中国银行股份有限公司上海分行",
                    "00100201",
                    Some("91310000100016818X"),
                ),
                build_entry("测试无税号长纯数字编号企业", "911101081234567890", None),
            ],
            updated_unix_secs: 500,
        };

        let bytes = export_to_csv_bytes(&dataset).expect("csv export should succeed");
        assert_eq!(&bytes[0..3], UTF8_BOM);

        let (reimported, _) =
            import_csv_bytes(&bytes, 500).expect("reimporting exported csv should succeed");
        assert_eq!(reimported.entries, dataset.entries);
    }

    #[test]
    fn test_xlsx_export_text_format_and_lossless_reimport() {
        let dataset = Dataset {
            entries: vec![
                build_entry(
                    "上海华为技术有限公司",
                    "0008899",
                    Some("91310115703298602B"),
                ),
                build_entry(
                    "十八位纯数字编码防科学计数法公司",
                    "123456789012345678",
                    Some("91110000802100433B"),
                ),
            ],
            updated_unix_secs: 600,
        };

        let xlsx_bytes = export_to_xlsx_bytes(&dataset).expect("xlsx export should succeed");
        let (reimported, _) =
            import_xlsx_bytes(&xlsx_bytes, 600).expect("reimporting exported xlsx should succeed");
        assert_eq!(reimported.entries, dataset.entries);
    }
}

