use crate::clean::contains_ascii_case_insensitive;
use crate::model::{Dataset, Entry};
use unicode_normalization::UnicodeNormalization;

/// 搜索结果列表最大返回条数（Top-20）
pub const MAX_SEARCH_RESULTS: usize = 20;

/// 内存检索输出结构
#[derive(Debug, Clone)]
pub struct SearchResult<'a> {
    /// 按相关度升序排列的前 Top-20 匹配项引用
    pub items: Vec<&'a Entry>,
    /// 满足多关键词 AND 条件的全部记录总数
    pub total_matched: usize,
}

/// 在数据集中执行亚毫秒级多关键词 AND 过滤与四档确定性相关度排序，截取前 20 条返回
pub fn search_dataset<'a>(dataset: &'a Dataset, raw_query: &str) -> SearchResult<'a> {
    let normalized_query: String = raw_query
        .chars()
        .nfkc()
        .flat_map(|c| c.to_lowercase())
        .collect();
    let tokens: Vec<&str> = normalized_query
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .collect();

    if tokens.is_empty() {
        let items: Vec<&'a Entry> = dataset.entries.iter().take(MAX_SEARCH_RESULTS).collect();
        return SearchResult {
            items,
            total_matched: dataset.entries.len(),
        };
    }

    let mut matched: Vec<(u32, usize, &'a Entry)> = Vec::new();

    'entry_loop: for (orig_idx, entry) in dataset.entries.iter().enumerate() {
        let mut first_primary_pos: Option<usize> = None;
        let mut first_tax_pos: Option<usize> = None;
        let mut first_is_exact = false;
        let mut first_is_prefix = false;
        let mut extra_pos_sum: usize = 0;

        for (tok_idx, &token) in tokens.iter().enumerate() {
            let name_pos = entry.search_name_nfkc.find(token);
            let py_pos = entry.pinyin_initials.find(token);
            let code_pos = contains_ascii_case_insensitive(&entry.code, token);
            let tax_pos = if entry.tax_id.is_empty() {
                None
            } else {
                contains_ascii_case_insensitive(&entry.tax_id, token)
            };

            let primary_pos = [name_pos, py_pos, code_pos].into_iter().flatten().min();

            if primary_pos.is_none() && tax_pos.is_none() {
                continue 'entry_loop;
            }

            if tok_idx == 0 {
                first_primary_pos = primary_pos;
                first_tax_pos = tax_pos;

                if entry.code.eq_ignore_ascii_case(token)
                    || entry.search_name_nfkc == token
                    || entry.pinyin_initials == token
                {
                    first_is_exact = true;
                } else if primary_pos == Some(0) {
                    first_is_prefix = true;
                }
            } else {
                let pos = primary_pos.or(tax_pos).unwrap_or(0);
                extra_pos_sum = extra_pos_sum.saturating_add(pos);
            }
        }

        let name_len = (entry.name.chars().count().min(99)) as u32;
        let score: u32 = if first_is_exact {
            // Tier 0 (0..100): 精确匹配
            name_len
        } else if first_is_prefix {
            // Tier 1 (100..500): 前缀匹配
            100 + (extra_pos_sum.min(299) as u32) + name_len
        } else if let Some(first_pos) = first_primary_pos {
            // Tier 2 (500..2000): 名称 / 拼音简码 / 编号子串匹配
            500 + (first_pos.saturating_add(extra_pos_sum).min(1399) as u32) + name_len
        } else {
            // Tier 3 (2000+): 仅统一社会信用代码/税号子串匹配
            let t_pos = first_tax_pos.unwrap_or(0);
            2000 + (t_pos.saturating_add(extra_pos_sum).min(1000) as u32) + name_len
        };

        matched.push((score, orig_idx, entry));
    }

    let total_matched = matched.len();
    if matched.len() > MAX_SEARCH_RESULTS {
        matched.select_nth_unstable_by_key(MAX_SEARCH_RESULTS - 1, |&(score, idx, _)| (score, idx));
        matched.truncate(MAX_SEARCH_RESULTS);
    }
    matched.sort_unstable_by_key(|&(score, idx, _)| (score, idx));

    let items = matched.into_iter().map(|(_, _, entry)| entry).collect();
    SearchResult {
        items,
        total_matched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::build_entry;

    #[test]
    fn test_multi_keyword_and_filtering() {
        let dataset = Dataset {
            entries: vec![
                build_entry(
                    "中国银行股份有限公司上海陆家嘴支行",
                    "100201",
                    Some("91310000100016818X"),
                ),
                build_entry(
                    "中国银行股份有限公司北京分行",
                    "100202",
                    Some("91110000100016818Y"),
                ),
                build_entry("上海华为技术有限公司", "HW-001", None),
            ],
            updated_unix_secs: 0,
        };

        let res = search_dataset(&dataset, "zgyh 上海");
        assert_eq!(res.total_matched, 1);
        assert_eq!(res.items[0].code, "100201");
    }

    #[test]
    fn test_relevance_scoring_tiers() {
        let dataset = Dataset {
            entries: vec![
                build_entry(
                    "仅税号包含测试关键字企业",
                    "999001",
                    Some("91310000HW0016818X"),
                ),
                build_entry("华东某含hw子串科技公司", "888002", None),
                build_entry("华为技术有限公司", "777003", None),
                build_entry("测试精确代码公司", "HW", None),
            ],
            updated_unix_secs: 0,
        };

        let res = search_dataset(&dataset, "hw");
        assert_eq!(res.total_matched, 4);
        assert_eq!(res.items[0].code, "HW"); // Tier 0: Exact code match
        assert_eq!(res.items[1].code, "777003"); // Tier 1: Prefix pinyin "hwjsyxgs"
        assert_eq!(res.items[2].code, "888002"); // Tier 2: Substring match
        assert_eq!(res.items[3].code, "999001"); // Tier 3: Tax ID match only
    }

    #[test]
    fn test_top_20_truncation_with_total_count() {
        let mut entries = Vec::with_capacity(100);
        for i in 0..100 {
            entries.push(build_entry(
                &format!("上海测试企业第{:03}号有限公司", i),
                &format!("CODE-{:03}", i),
                None,
            ));
        }
        let dataset = Dataset {
            entries,
            updated_unix_secs: 0,
        };

        let res = search_dataset(&dataset, "上海");
        assert_eq!(res.total_matched, 100);
        assert_eq!(res.items.len(), MAX_SEARCH_RESULTS);
    }
}

