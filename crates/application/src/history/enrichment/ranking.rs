//! 来源排序（V5 §23/§24）：去重、域名归一、来源类型分类、权威加权。
//! 全部纯函数，可离线单测。不做「top N → LLM」的裸直通。

use crate::history::enrichment::ports::{SourceEvidence, SourceType};

/// 归一化域名（去 scheme/www/端口/大小写）。`https://www.Museum.cn:443/x` → `museum.cn`。
#[must_use]
pub fn normalize_domain(url: &str) -> String {
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = without_scheme.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    host.trim_start_matches("www.").to_lowercase()
}

/// 归一化 URL 用于去重：去 fragment 与常见会变化参数（utm_*）。
#[must_use]
pub fn dedupe_key(url: &str) -> String {
    let base = url.split('#').next().unwrap_or_default();
    let mut parts = base.split('?');
    let path = parts.next().unwrap_or_default();
    let query = parts.next();
    let mut kept = Vec::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if !pair.starts_with("utm_") && !pair.is_empty() {
                kept.push(pair);
            }
        }
    }
    let mut key = path.trim_end_matches('/').to_string();
    if !kept.is_empty() {
        key.push('?');
        key.push_str(&kept.join("&"));
    }
    key
}

/// 来源类型分类（基于域名 + 标题线索；先域名规则后标题）。default General。
#[must_use]
pub fn classify_source(domain: &str, title: &str) -> SourceType {
    let domain = domain.to_lowercase();
    let title = title.to_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|needle| domain.contains(needle) || title.contains(needle));

    if any(&[
        ".gov.", ".gov.cn", "gov.", "state.", "mod.", "cctv", "people.cn", "xinhuanet",
        "政府", "官网", "official", "ministry",
    ]) {
        return SourceType::Official;
    }
    if any(&["museum", "gallery", "heritage", "博物馆", "美术馆", "texas"]) && domain.contains("museum") || title.contains("博物馆") || domain.contains("gallery") {
        return SourceType::Museum;
    }
    if any(&[".archive.org", "archive", "digital archive", "档案", "馆藏"]) {
        return SourceType::Archive;
    }
    if any(&[".edu", ".edu.cn", "university", "ac.cn", "大学", "学院", "研究所"]) {
        return SourceType::University;
    }
    if any(&[".ac.", "academic", "期刊", "学报", "china-scholar", "cass", "社科"]) {
        return SourceType::Academic;
    }
    if any(&["baike", "wikipedia", "百科", "encyclopedia", "dict.", "cidian"]) {
        return SourceType::Reference;
    }
    SourceType::General
}

/// 权威权重（越大越好）。
#[must_use]
pub const fn authority_weight(kind: SourceType) -> u8 {
    match kind {
        SourceType::Official => 100,
        SourceType::Museum => 90,
        SourceType::Archive => 85,
        SourceType::University => 80,
        SourceType::Academic => 75,
        SourceType::Reference => 60,
        SourceType::General => 20,
    }
}

/// 对原始搜索结果做 去重 + 分类 + 排序，返回排序后的证据列表（≤ limit）。
#[must_use]
pub fn rank_sources(sources: Vec<SourceEvidence>, limit: usize) -> Vec<SourceEvidence> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut ranked: Vec<(SourceEvidence, u8)> = Vec::new();
    for source in sources {
        let key = dedupe_key(&source.url);
        if key.is_empty() || !seen.insert(key) {
            continue; // 去重
        }
        let weight = authority_weight(source.source_type);
        ranked.push((source, weight));
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.title.cmp(&b.0.title)));
    ranked.into_iter().take(limit).map(|(source, _)| source).collect()
}

/// 把证据包转成给模型的来源列表文本（id = url；模型只引用提供的 url）。
#[must_use]
pub fn sources_to_prompt_block(sources: &[SourceEvidence]) -> String {
    if sources.is_empty() {
        return "[sources]（无外部来源。仅可基于以下 canonical/evidence 回答，并如实标注信息局限。）".to_string();
    }
    let mut lines = vec!["[sources] 以下为检索到的来源，按其 url（作为 source_id）引用：".to_string()];
    for (index, source) in sources.iter().enumerate() {
        lines.push(format!(
            "{}. {} | {} | {} | {}",
            index + 1,
            source.url,
            source.domain,
            source.title,
            source.published_at.map(|ts| format!("published:{ts}")).unwrap_or_default()
        ));
        let snippet: String = source.snippet.chars().take(500).collect();
        lines.push(format!("   excerpt: {snippet}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(url: &str, source_type: SourceType) -> SourceEvidence {
        SourceEvidence {
            title: "t".into(),
            url: url.into(),
            domain: normalize_domain(url),
            snippet: "s".into(),
            published_at: None,
            source_type,
        }
    }

    #[test]
    fn domain_normalization() {
        assert_eq!(normalize_domain("https://www.Museum.cn:443/a"), "museum.cn");
        assert_eq!(normalize_domain("http://gov.example.com/x"), "gov.example.com");
    }

    #[test]
    fn dedupe_strips_fragments_and_utm() {
        let a = dedupe_key("https://x.com/a?utm_source=1&id=5#top");
        let b = dedupe_key("https://x.com/a?id=5&utm_medium=mail");
        assert_eq!(a, b);
    }

    #[test]
    fn source_classification_priorities() {
        assert_eq!(classify_source("www.gov.cn", "通知"), SourceType::Official);
        assert_eq!(classify_source("museum.example.com", "馆藏"), SourceType::Museum);
        assert_eq!(classify_source("archive.example.org", "档案"), SourceType::Archive);
        assert_eq!(classify_source("peking.edu.cn", "论文"), SourceType::University);
        assert_eq!(classify_source("baike.example.com", "词条"), SourceType::Reference);
        assert_eq!(classify_source("blog.example.com", "随想"), SourceType::General);
    }

    #[test]
    fn ranking_dedupes_and_orders_by_authority() {
        let sources = vec![
            evidence("https://blog.com/a", SourceType::General),
            evidence("https://www.gov.cn/a", SourceType::Official),
            evidence("https://blog.com/a?utm_source=1", SourceType::General), // 重复 → 去重
            evidence("https://museum.cn/b", SourceType::Museum),
        ];
        let ranked = rank_sources(sources, 10);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].domain, "gov.cn");
        assert_eq!(ranked[1].source_type, SourceType::Museum);
    }

    #[test]
    fn ranking_respects_limit() {
        let sources = vec![
            evidence("https://a.cn/1", SourceType::General),
            evidence("https://a.cn/2", SourceType::General),
            evidence("https://a.cn/3", SourceType::General),
        ];
        assert_eq!(rank_sources(sources, 2).len(), 2);
    }

    #[test]
    fn sources_prompt_block_lists_urls() {
        let sources = vec![
            evidence("https://www.gov.cn/report", SourceType::Official),
            evidence("https://baike.cn/item", SourceType::Reference),
        ];
        let block = sources_to_prompt_block(&sources);
        assert!(block.contains("https://www.gov.cn/report"));
        assert!(block.contains("baike.cn"));
    }
}