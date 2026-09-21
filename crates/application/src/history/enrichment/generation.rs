//! 结构化生成（V5 §25）：LLM 输出必须为约束 JSON envelope，而不是大段 Markdown。
//!
//! 输出 shape（模型必须遵守）：
//! ```json
//! {
//!   "section": "overview",
//!   "content": "…正文，仅使用给定来源…",
//!   "claims": [{"text": "主张", "source_ids": ["https://…"]}],
//!   "uncertainties": [],
//!   "controversies": []
//! }
//! ```

use devtoolbox_core::history_enrichment::{EnrichmentClaim, EnrichmentPayload};

/// 生成系统提示词（拆开：core rules + 结构化 schema，V4 §57 同风格）。
pub fn system_prompt() -> &'static str {
    "你是 self-tools 的历史百科编辑，负责为给定历史实体撰写审慎的补充解读。\
\n\n规则：\
\n1. 只依据提供给你的 sources 与 canonical/evidence 内容写作。\
\n2. 每条实质性主张必须用 claims[].source_ids 标注出处（值为提供来源的 url 原文）。\
\n3. 证据不足的主张写入 uncertainties；存在争论的写入 controversies。\
\n4. 禁止编造来源；禁止引用 sources 里不存在的 url。\
\n5. 若 sources 为空：如实说明资料局限，并明确标注「未能核实」。\
\n6. 只输出 JSON，不要 Markdown、不要前后缀说明。"
}

/// 用户提示词：实体信息 + sources。
#[must_use]
pub fn user_prompt(
    entity_type: &str,
    entity_id: &str,
    entity_label: &str,
    section: &str,
    canonical_block: &str,
    sources_block: &str,
) -> String {
    format!(
        "实体: {entity_type} `{entity_id}`（{entity_label}）\n\
         要撰写的 section: {section}\n\
         输出 JSON envelope: {{\"section\": \"{section}\", \"content\": \"...\", \
         \"claims\": [{{\"text\": \"...\", \"source_ids\": [\"https://...\"]}}], \
         \"uncertainties\": [\"...\"], \"controversies\": [\"...\"]}}\n\n\
         ==== canonical / evidence（可信底稿，可引用但也要注明来源）====\n{canonical_block}\n\n\
         {sources_block}"
    )
}

/// 解析模型输出的 JSON envelope；任何形状错误 → Err（校验门前置，不入 READY）。
pub fn parse_generation(raw: &str, expected_section: &str) -> Result<EnrichmentPayload, String> {
    let trimmed = raw.trim();
    let candidate = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```")
            .trim_start_matches("json")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    let value: serde_json::Value = serde_json::from_str(candidate)
        .map_err(|error| format!("invalid generation json: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "generation must be a JSON object".to_string())?;

    let section = object
        .get("section")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    if section != expected_section {
        return Err(format!(
            "generation section mismatch: expected `{expected_section}`, got `{section}`"
        ));
    }
    let content = object
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let claims = parse_claims(object.get("claims"));
    let uncertainties = parse_string_list(object.get("uncertainties"));
    let controversies = parse_string_list(object.get("controversies"));
    Ok(EnrichmentPayload {
        section,
        content,
        claims,
        uncertainties,
        controversies,
    })
}

fn parse_claims(value: Option<&serde_json::Value>) -> Vec<EnrichmentClaim> {
    let Some(array) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            let object = item.as_object()?;
            Some(EnrichmentClaim {
                text: object.get("text")?.as_str()?.to_string(),
                source_ids: object
                    .get("source_ids")
                    .and_then(serde_json::Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_envelope() {
        let raw = r#"{"section":"overview","content":"1935 年 1 月…","claims":[{"text":"确立领导地位","source_ids":["https://gov.cn/x"]}],"uncertainties":["具体日期存疑"],"controversies":[]}"#;
        let payload = parse_generation(raw, "overview").unwrap();
        assert_eq!(payload.section, "overview");
        assert_eq!(payload.claims.len(), 1);
        assert_eq!(payload.claims[0].source_ids[0], "https://gov.cn/x");
        assert_eq!(payload.uncertainties, vec!["具体日期存疑"]);
    }

    #[test]
    fn rejects_section_mismatch() {
        let raw = r#"{"section":"background","content":"x"}"#;
        assert!(parse_generation(raw, "overview").is_err());
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse_generation("一堆 Markdown 文字", "overview").is_err());
    }

    #[test]
    fn accepts_fenced_json() {
        let raw = "```json\n{\"section\":\"overview\",\"content\":\"好\",\"claims\":[]}\n```";
        let payload = parse_generation(raw, "overview").unwrap();
        assert_eq!(payload.content, "好");
    }

    #[test]
    fn strips_invalid_claims_items() {
        let raw = r#"{"section":"overview","content":"x","claims":[{"text":"ok","source_ids":["a"]},{"source_ids":["b"]}]}"#;
        let payload = parse_generation(raw, "overview").unwrap();
        // 第二个 claim 缺 text → 被跳过
        assert_eq!(payload.claims.len(), 1);
        assert_eq!(payload.claims[0].source_ids, vec!["a"]);
    }
}
