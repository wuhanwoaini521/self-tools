//! LLM 输出的容错解析（需求 #十八/#二十四：LLM 返回非法 JSON / 夹带文字的兜底）。
//!
//! 解析策略：
//! - 剥掉 ```json 围栏与前后文说明文字，取第一个 `{ ... }` / `[ ... ]`；
//! - 未知类别跳过、缺失可选字段走默认值，绝不整体失败（除非完全无法解析出 JSON）。

use serde::Deserialize;
use thiserror::Error;

use crate::travel::guide::CityGuide;
use crate::travel::model::{FactCategory, TravelFact};

#[derive(Debug, Error, PartialEq)]
pub enum TravelParseError {
    #[error("no json found in llm output")]
    NoJson,
    #[error("json is not valid: {0}")]
    InvalidJson(String),
    #[error("unexpected shape: {0}")]
    Shape(String),
}

/// 从 LLM 输出中提取第一个 JSON 值（剥围栏、截取首尾括号）。
pub fn extract_json(raw: &str) -> Result<serde_json::Value, TravelParseError> {
    let trimmed = raw.trim();
    // 剥掉可能存在的 Markdown 围栏
    let mut body = trimmed;
    body = body
        .strip_prefix("```json")
        .or_else(|| body.strip_prefix("```JSON"))
        .or_else(|| body.strip_prefix("```"))
        .unwrap_or(body);
    body = body.strip_suffix("```").unwrap_or(body);
    let body = body.trim();
    if body.is_empty() {
        return Err(TravelParseError::NoJson);
    }
    let start = body
        .char_indices()
        .find_map(|(index, ch)| (ch == '{' || ch == '[').then_some(index));
    let Some(start) = start else {
        return Err(TravelParseError::NoJson);
    };
    let end = body
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (ch == '}' || ch == ']').then_some(index + ch.len_utf8()));
    let Some(end) = end else {
        return Err(TravelParseError::NoJson);
    };
    if end <= start {
        return Err(TravelParseError::NoJson);
    }
    serde_json::from_str(&body[start..end])
        .map_err(|error| TravelParseError::InvalidJson(error.to_string()))
}

/// LLM 单条事实的中间形态（容忍缺失 category / confidence）。
#[derive(Debug, Deserialize)]
struct FactItem {
    category: Option<String>,
    subject: Option<String>,
    value: Option<String>,
    confidence: Option<f32>,
}

/// 解析事实列表。`source_id` 与 `fetched_at` 由调用方（application）注入。
/// 未知类别或缺失关键字段的条目直接跳过；完全无法解析返回 Err（调用方降级处理）。
pub fn parse_facts_json(
    raw: &str,
    source_id: &str,
    fetched_at: i64,
) -> Result<Vec<TravelFact>, TravelParseError> {
    let value = extract_json(raw)?;
    let items: Vec<FactItem> = match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| TravelParseError::InvalidJson(error.to_string()))?,
        serde_json::Value::Object(mut object) => {
            // 兼容 {"facts": [...]} 包裹形式
            let facts = object.remove("facts").ok_or_else(|| {
                TravelParseError::Shape("expected array or object with \"facts\"".to_string())
            })?;
            serde_json::from_value(facts)
                .map_err(|error| TravelParseError::InvalidJson(error.to_string()))?
        }
        _ => return Err(TravelParseError::Shape("expected json array".to_string())),
    };

    Ok(items
        .into_iter()
        .filter_map(|item| {
            let category = item.category.as_deref().and_then(FactCategory::parse)?;
            let subject = item
                .subject
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())?;
            let value = item
                .value
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())?;
            let confidence = item.confidence.unwrap_or(0.6).clamp(0.0, 1.0);
            Some(TravelFact {
                category,
                subject,
                value,
                source_id: source_id.to_string(),
                confidence,
                fetched_at,
            })
        })
        .collect())
}

/// 解析完整攻略 JSON（容错：缺字段走默认值）。
pub fn parse_guide_json(raw: &str) -> Result<CityGuide, TravelParseError> {
    let value = extract_json(raw)?;
    serde_json::from_value(value).map_err(|error| TravelParseError::InvalidJson(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{TravelParseError, extract_json, parse_facts_json, parse_guide_json};
    use crate::travel::model::FactCategory;

    const FACTS_JSON: &str = r#"[
        {"category": "attraction", "subject": "西湖", "value": "免费开放", "confidence": 0.9},
        {"category": "opening_hours", "subject": "灵隐寺", "value": "07:00-18:00"},
        {"category": "food", "subject": "西湖醋鱼", "value": "杭帮菜经典"},
        {"category": "unknown_category", "subject": "x", "value": "y"}
    ]"#;

    #[test]
    fn extracts_json_from_fenced_output() {
        let raw = "好的，以下是结果：\n```json\n{\"a\": 1}\n```\n希望对你有帮助";
        let value = extract_json(raw).expect("extract");
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn extracts_json_from_bare_output() {
        let value = extract_json(r#"{"a": [1, 2]}"#).expect("extract");
        assert_eq!(value["a"][1], 2);
    }

    #[test]
    fn rejects_output_without_json() {
        assert_eq!(
            extract_json("抱歉，我无法回答"),
            Err(TravelParseError::NoJson)
        );
    }

    #[test]
    fn parses_fact_list_with_unknown_categories_skipped() {
        let facts =
            parse_facts_json(FACTS_JSON, "https://example.com", 1_700_000_000).expect("facts");
        assert_eq!(facts.len(), 3);
        assert_eq!(facts[0].category, FactCategory::Attraction);
        assert_eq!(facts[0].subject, "西湖");
        assert_eq!(facts[0].value, "免费开放");
        assert_eq!(facts[0].source_id, "https://example.com");
        assert_eq!(facts[0].confidence, 0.9);
        // 缺 confidence → 默认 0.6
        assert_eq!(facts[1].confidence, 0.6);
    }

    #[test]
    fn parses_wrapped_facts_object() {
        let raw = format!(r#"{{"facts": {FACTS_JSON}}}"#);
        let facts = parse_facts_json(&raw, "s", 1).expect("facts");
        assert_eq!(facts.len(), 3);
    }

    #[test]
    fn skips_items_with_missing_required_fields() {
        let raw = r#"[{"category": "attraction"}, {"subject": "西湖", "value": "x"}]"#;
        let facts = parse_facts_json(raw, "s", 1).expect("facts");
        assert!(facts.is_empty());
    }

    #[test]
    fn rejects_non_array_shapes() {
        assert!(matches!(
            parse_facts_json(r#"{"foo": 1}"#, "s", 1),
            Err(TravelParseError::Shape(_))
        ));
    }

    #[test]
    fn guide_json_with_missing_sections_defaults_to_empty() {
        let raw = r#"{"city": {"name": "杭州"}, "summary": "西湖美景"}"#;
        let guide = parse_guide_json(raw).expect("guide");
        assert_eq!(guide.city.name, "杭州");
        assert_eq!(guide.summary, "西湖美景");
        assert!(guide.attractions.is_empty());
        assert!(guide.sources.is_empty());
        assert!(guide.highlights.is_empty());
    }

    #[test]
    fn guide_json_with_full_sections_round_trips() {
        let raw = r#"{
            "city": {"name": "杭州", "name_en": "Hangzhou"},
            "summary": "S",
            "highlights": ["西湖"],
            "attractions": [{"name": "西湖", "intro": "著名湖泊"}],
            "itineraries": {
                "three_days": {"day": 3, "stops": [{"name": "西湖"}]}
            }
        }"#;
        let guide = parse_guide_json(raw).expect("guide");
        assert_eq!(guide.attractions[0].name, "西湖");
        let three = guide.itineraries.three_days.expect("three days");
        assert_eq!(three.day, 3);
        assert_eq!(three.stops[0].name, "西湖");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(
            parse_guide_json(r#"{"city": }"#),
            Err(TravelParseError::InvalidJson(_))
        ));
    }
}
