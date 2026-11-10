use crate::model::Entry;
use pinyin::ToPinyin;
use unicode_normalization::UnicodeNormalization;

/// 判断字符是否属于 CJK 表意文字、中日韩标点符号或全角 ASCII 标点变体
#[inline]
pub fn is_cjk_or_fullwidth_punct(c: char) -> bool {
    matches!(
        c,
        '\u{2E80}'..='\u{9FFF}'
            | '\u{FF00}'..='\u{FFEF}'
            | '\u{F900}'..='\u{FAFF}'
    )
}

/// 判断字符是否属于零宽不可见格式字符
#[inline]
fn is_zero_width_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{2060}'
    )
}

/// 深度清洗企业名称：
/// 1. 先将不间断空格、全角空格、制表符、回车换行统一映射为标准半角空格 `' '`；
/// 2. 过滤所有零宽字符及其余控制字符；
/// 3. 执行 NFKC 兼容等价规范化（全角括号、字母、数字转标准半角）；
/// 4. 剔除首尾空格，对内部空格执行中英文上下文感知折叠（CJK 邻接空格直接剔除，拉丁/数字词间折叠为单空格）。
pub fn clean_company_name(raw: &str) -> String {
    let mapped = raw
        .chars()
        .map(|c| match c {
            '\u{00A0}' | '\u{3000}' | '\t' | '\r' | '\n' => ' ',
            _ => c,
        })
        .filter(|&c| !is_zero_width_char(c) && (c == ' ' || !c.is_control()));

    let nfkc_chars: Vec<char> = mapped.nfkc().collect();
    let mut result = String::with_capacity(nfkc_chars.len());
    let mut pending_space = false;
    let mut last_emitted: Option<char> = None;

    for c in nfkc_chars {
        if c == ' ' || c.is_whitespace() {
            if last_emitted.is_some() {
                pending_space = true;
            }
            continue;
        }

        if pending_space {
            if let Some(prev) = last_emitted {
                if !is_cjk_or_fullwidth_punct(prev) && !is_cjk_or_fullwidth_punct(c) {
                    result.push(' ');
                }
            }
            pending_space = false;
        }

        result.push(c);
        last_emitted = Some(c);
    }

    result
}

/// 清洗企业编号或统一社会信用代码（税号）字段：
/// 执行 NFKC 规范化后，剥离全部空白符（含 CSV 导出保护前缀 `\t`）、零宽字符与控制字符。
pub fn clean_code_field(raw: &str) -> String {
    raw.chars()
        .filter(|&c| !is_zero_width_char(c))
        .nfkc()
        .filter(|&c| !c.is_whitespace() && !c.is_control())
        .collect()
}

/// 检测清洗后的编号或税号字符串是否已发生 Excel 科学计数法截断损坏（例如 `9.110108E+17` 或 `1E+12`）
pub fn is_scientific_notation_corrupted(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut e_pos = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'E' || b == b'e' {
            if e_pos.is_some() {
                return false;
            }
            e_pos = Some(i);
        }
    }

    let Some(idx) = e_pos else {
        return false;
    };
    if idx == 0 || idx + 2 >= bytes.len() {
        return false;
    }

    let mantissa = &code[..idx];
    let exp_part = &code[idx + 1..];
    let Some(exp_digits) = exp_part.strip_prefix('+') else {
        return false;
    };
    if exp_digits.is_empty() || !exp_digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    let Ok(exp_val) = exp_digits.parse::<u32>() else {
        return false;
    };
    if exp_val < 10 {
        return false;
    }

    let mut dot_count = 0usize;
    let mut digit_count = 0usize;
    for b in mantissa.bytes() {
        if b == b'.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else if b.is_ascii_digit() {
            digit_count += 1;
        } else {
            return false;
        }
    }

    digit_count > 0
}

/// 零堆分配 ASCII 大小写不敏感子串查找：
/// 在 `haystack` 中查找全小写模式串 `needle_lower` 的首个匹配字节偏移。
#[inline]
pub fn contains_ascii_case_insensitive(haystack: &str, needle_lower: &str) -> Option<usize> {
    let h_bytes = haystack.as_bytes();
    let n_bytes = needle_lower.as_bytes();
    if n_bytes.is_empty() {
        return Some(0);
    }
    if n_bytes.len() > h_bytes.len() {
        return None;
    }
    h_bytes
        .windows(n_bytes.len())
        .position(|window| window.eq_ignore_ascii_case(n_bytes))
}

/// 根据企业名称上下文修正常见多音字拼音首字母（如“银行/分行/支行/商行/行业”中的“行”取 'h'，“重庆/重工”中的“重”取 'c'，“厦门”中的“厦”取 'x'，“长沙/长春/长江/长城”中的“长”取 'c'）
fn resolve_polyphone_initial(chars: &[char], idx: usize) -> Option<char> {
    let c = chars[idx];
    let prev = if idx > 0 { Some(chars[idx - 1]) } else { None };
    let next = chars.get(idx + 1).copied();

    match c {
        '行' => {
            if matches!(
                prev,
                Some('银' | '支' | '分' | '总' | '商' | '农' | '央' | '同' | '车' | '琴' | '酒' | '药' | '洋' | '金')
            ) || matches!(next, Some('业' | '情'))
            {
                return Some('h');
            }
        }
        '重' => {
            if matches!(next, Some('庆' | '工' | '汽' | '钢' | '百' | '啤' | '药')) {
                return Some('c');
            }
        }
        '厦' => {
            if matches!(next, Some('门')) {
                return Some('x');
            }
        }
        '长' => {
            if matches!(next, Some('沙' | '春' | '江' | '城' | '安' | '虹' | '治' | '途')) {
                return Some('c');
            }
        }
        _ => {}
    }
    None
}

/// 构造单条标准化企业记录，预计算 NFKC 小写检索键与汉字拼音首字母简码
pub fn build_entry(raw_name: &str, raw_code: &str, raw_tax_id: Option<&str>) -> Entry {
    let name = clean_company_name(raw_name);
    let code = clean_code_field(raw_code);
    let tax_id = raw_tax_id.map(clean_code_field).unwrap_or_default();

    let name_chars: Vec<char> = name.chars().collect();
    let mut search_name_nfkc = String::with_capacity(name.len());
    let mut pinyin_initials = String::with_capacity(name_chars.len());

    for (idx, &c) in name_chars.iter().enumerate() {
        if !c.is_whitespace() {
            for lower_c in c.to_lowercase() {
                search_name_nfkc.push(lower_c);
            }
        }

        if let Some(poly_initial) = resolve_polyphone_initial(&name_chars, idx) {
            pinyin_initials.push(poly_initial);
        } else if let Some(p) = c.to_pinyin() {
            if let Some(first_letter) = p.plain().chars().next() {
                pinyin_initials.push(first_letter.to_ascii_lowercase());
            }
        } else if c.is_ascii_alphanumeric() {
            pinyin_initials.push(c.to_ascii_lowercase());
        }
    }

    Entry {
        name,
        code,
        tax_id,
        search_name_nfkc,
        pinyin_initials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fullwidth_nfkc_normalization() {
        let raw = "ＡＢＣ１２３（测试）";
        assert_eq!(clean_company_name(raw), "ABC123(测试)");
    }

    #[test]
    fn test_zero_width_and_control_whitespace_mapping() {
        let raw = "\u{FEFF}  Apple\t\nInc\u{200B}\u{00A0}Ltd  ";
        assert_eq!(clean_company_name(raw), "Apple Inc Ltd");
    }

    #[test]
    fn test_context_aware_space_collapsing() {
        let raw = "  中国  银行  China   Bank  上海分行\u{200B} ";
        assert_eq!(clean_company_name(raw), "中国银行China Bank上海分行");
    }

    #[test]
    fn test_pinyin_initials_generation() {
        let entry = build_entry("中国银行股份有限公司", "100201", None);
        assert_eq!(entry.pinyin_initials, "zgyhgfyxgs");
        assert_eq!(entry.search_name_nfkc, "中国银行股份有限公司");
    }

    #[test]
    fn test_scientific_notation_detection_and_csv_tab_strip() {
        assert!(is_scientific_notation_corrupted("9.110108E+17"));
        assert!(is_scientific_notation_corrupted("1.2345e+15"));
        assert!(!is_scientific_notation_corrupted("91110108MA0012345X"));
        assert!(!is_scientific_notation_corrupted("100E+02"));

        assert_eq!(clean_code_field("\t00120390 "), "00120390");
    }
}

