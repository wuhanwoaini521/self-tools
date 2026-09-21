//! 校验门（V5 §26）：持久化 READY 前逐项验证。失败 → FAILED，绝不存 READY。

use std::collections::HashSet;

use devtoolbox_core::history_enrichment::{EnrichmentKey, EnrichmentPayload};

/// 校验结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// 校验（一体化入口）。
pub fn validate(
    key: &EnrichmentKey,
    payload: &EnrichmentPayload,
    allowed_source_ids: &[String],
) -> ValidationReport {
    let mut errors = Vec::new();

    // 1) 形状 / 必需字段
    if payload.section != key.section.as_str() {
        errors.push(format!(
            "section mismatch: payload `{}` vs key `{}`",
            payload.section,
            key.section.as_str()
        ));
    }
    if payload.content.trim().is_empty() {
        errors.push("content is empty".to_string());
    }
    if payload.content.chars().count() > 20_000 {
        errors.push("content exceeds 20k chars".to_string());
    }

    // 2) 来源引用解析：claims 的 source_ids 必须 ⊆ 本次提供的 source_ids
    let allowed: HashSet<&str> = allowed_source_ids.iter().map(String::as_str).collect();
    let mut seen_citations = 0usize;
    for claim in &payload.claims {
        if claim.text.trim().is_empty() {
            errors.push("claim with empty text".to_string());
        }
        for source_id in &claim.source_ids {
            if !allowed.contains(source_id.as_str()) {
                errors.push(format!("claim cites unknown source: {source_id}"));
            } else {
                seen_citations += 1;
            }
        }
    }
    if !payload.claims.is_empty() && seen_citations == 0 {
        errors.push("claims exist but none carries a resolvable source".to_string());
    }

    ValidationReport { valid: errors.is_empty(), errors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::history_enrichment::{EnrichmentClaim, EnrichmentSection};

    fn key() -> EnrichmentKey {
        EnrichmentKey::new("event", "zunyi", EnrichmentSection::Overview, "zh-CN")
    }

    #[test]
    fn valid_payload_passes() {
        let payload = EnrichmentPayload {
            section: "overview".into(),
            content: "内容".into(),
            claims: vec![EnrichmentClaim {
                text: "主张".into(),
                source_ids: vec!["https://a.cn/x".into()],
            }],
            ..EnrichmentPayload::default()
        };
        let report = validate(&key(), &payload, &["https://a.cn/x".into()]);
        assert!(report.valid, "{:?}", report.errors);
    }

    #[test]
    fn empty_content_fails() {
        let payload = EnrichmentPayload {
            section: "overview".into(),
            content: "   ".into(),
            claims: vec![],
            uncertainties: vec![],
            controversies: vec![],
        };
        let report = validate(&key(), &payload, &[]);
        assert!(!report.valid);
        assert!(report.errors.iter().any(|e| e.contains("empty")));
    }

    #[test]
    fn unknown_citation_fails() {
        let payload = EnrichmentPayload {
            section: "overview".into(),
            content: "内容".into(),
            claims: vec![EnrichmentClaim {
                text: "x".into(),
                source_ids: vec!["https://ghost.cn/y".into()],
            }],
            ..EnrichmentPayload::default()
        };
        let report = validate(&key(), &payload, &["https://a.cn/x".into()]);
        assert!(!report.valid);
        assert!(report.errors.iter().any(|e| e.contains("unknown source")));
    }

    #[test]
    fn section_mismatch_fails() {
        let payload = EnrichmentPayload {
            section: "impact".into(),
            ..EnrichmentPayload::default()
        };
        let report = validate(&key(), &payload, &[]);
        assert!(!report.valid);
    }
}