//! 检索用文本工具（application 层，纯函数）。
//!
//! 中文没有空格分词，因此关键词提取对 CJK 连续片段额外产出 bigram，
//! 让 SQL `LIKE` 粗筛也能命中「Docker 数据目录」这类混合查询。

/// 关键词上限（防止长查询变成大量 LIKE）。
const MAX_KEYWORDS: usize = 12;

/// 从查询中提取关键词（小写、去重、保序）。
///
/// - ASCII/数字/下划线/连字符/- 点 连成 token；
/// - CJK 连续片段整体作为一个 token，并在长度 ≥ 2 时追加 bigram。
#[must_use]
pub fn keywords(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in split_tokens(query) {
        push_unique(&mut out, token.clone());
        if token.chars().count() >= 2 && token.chars().any(is_cjk) {
            for bigram in bigrams(&token) {
                push_unique(&mut out, bigram);
            }
        }
        if out.len() >= MAX_KEYWORDS {
            break;
        }
    }
    out.truncate(MAX_KEYWORDS);
    out
}

/// 归一化文本（小写 + 折平换行），用于词法打分。
#[must_use]
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
}

/// 命中 token 数（用于打分：命中越多越相关）。
#[must_use]
pub fn hit_count(haystack_normalized: &str, tokens: &[String]) -> usize {
    tokens
        .iter()
        .filter(|token| haystack_normalized.contains(token.as_str()))
        .count()
}

/// 关键词覆盖率（0.0–1.0）。
#[must_use]
pub fn coverage(haystack_normalized: &str, tokens: &[String]) -> f32 {
    if tokens.is_empty() {
        return 0.0;
    }
    hit_count(haystack_normalized, tokens) as f32 / tokens.len() as f32
}

/// 在 `haystack` 中定位关键词首次出现处，返回一个居中片段（供 snippet 使用）。
#[must_use]
pub fn snippet_around(text: &str, tokens: &[String], max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let lowered = normalize(trimmed);
    let byte_index = tokens
        .iter()
        .filter_map(|token| lowered.find(token.as_str()))
        .min();
    let start_char = match byte_index {
        Some(index) => {
            let char_index = lowered[..index].chars().count();
            char_index.saturating_sub(max_chars / 3)
        }
        None => 0,
    };
    let mut out: String = trimmed.chars().skip(start_char).take(max_chars).collect();
    if start_char > 0 {
        out.insert(0, '…');
    }
    if start_char + max_chars < trimmed.chars().count() {
        out.push('…');
    }
    out
}

fn split_tokens(query: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_is_cjk = false;
    for ch in query.chars() {
        if is_word_char(ch) {
            let is_cjk = is_cjk(ch);
            if !current.is_empty() && is_cjk != current_is_cjk {
                tokens.push(current.clone());
                current.clear();
            }
            current_is_cjk = is_cjk;
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
        .into_iter()
        .filter(|token| !token.is_empty())
        .collect()
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.') || is_cjk(ch)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn bigrams(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::with_capacity(chars.len() - 1);
    for window in chars.windows(2) {
        out.push(window.iter().collect());
    }
    out.truncate(6);
    out
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if value.is_empty() || out.contains(&value) {
        return;
    }
    out.push(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_queries_split_on_punctuation() {
        let tokens = keywords("Jenkins durabletask, AccessDeniedException?");
        assert!(tokens.contains(&"jenkins".to_string()));
        assert!(tokens.contains(&"durabletask".to_string()));
        assert!(tokens.contains(&"accessdeniedexception".to_string()));
        // 大小写归一 + 去重
        assert_eq!(tokens.iter().filter(|token| *token == "jenkins").count(), 1);
    }

    #[test]
    fn cjk_queries_emit_bigrams() {
        let tokens = keywords("韩国旅行");
        assert!(tokens.contains(&"韩国旅行".to_string()));
        assert!(tokens.contains(&"韩国".to_string()));
        assert!(tokens.contains(&"国旅".to_string()));
        assert!(tokens.contains(&"旅行".to_string()));
    }

    #[test]
    fn mixed_query_keeps_both_scripts() {
        let tokens = keywords("Docker 数据目录");
        assert!(tokens.contains(&"docker".to_string()));
        assert!(tokens.contains(&"数据目录".to_string()));
        assert!(tokens.iter().all(|token| !token.is_empty()));
    }

    #[test]
    fn empty_and_symbol_queries_yield_nothing() {
        assert!(keywords("").is_empty());
        assert!(keywords("   ").is_empty());
        assert!(keywords("!!! ???").is_empty());
    }

    #[test]
    fn keyword_count_is_capped() {
        let query = (0..40)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(keywords(&query).len() <= MAX_KEYWORDS);
    }

    #[test]
    fn coverage_and_hits_measure_relevance() {
        let tokens = keywords("docker volume");
        let text = normalize("Docker volume 配置与数据目录");
        assert_eq!(hit_count(&text, &tokens), 2);
        assert!((coverage(&text, &tokens) - 1.0).abs() < f32::EPSILON);
        let partial = normalize("Docker 安装");
        assert_eq!(hit_count(&partial, &tokens), 1);
        assert!((coverage(&partial, &tokens) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn snippet_around_centers_on_match() {
        let text = format!(
            "{}{}{}",
            "前言".repeat(50),
            "Docker volume 关键内容",
            "后记".repeat(50)
        );
        let tokens = keywords("docker volume");
        let snippet = snippet_around(&text, &tokens, 40);
        assert!(snippet.contains("Docker volume"));
        assert!(snippet.starts_with('…'));
        assert!(snippet.ends_with('…'));

        let short = snippet_around("短文本", &tokens, 40);
        assert_eq!(short, "短文本");
    }
}
