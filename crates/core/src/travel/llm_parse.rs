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
                coordinates: None,
                poi_id: None,
                area: None,
                address: None,
            })
        })
        .collect())
}

/// 解析完整攻略 JSON（容错：缺字段走默认值）。
pub fn parse_guide_json(raw: &str) -> Result<CityGuide, TravelParseError> {
    let mut value = extract_json(raw)?;
    // `evidence` / `meta` / `sources` / `weather` 由 application 层用真实事实重算，
    // LLM 填的任何形状都会被覆盖；先剔除，避免它把整篇攻略的类型校验带崩。
    if let Some(object) = value.as_object_mut() {
        for key in ["evidence", "meta", "sources", "weather"] {
            object.remove(key);
        }
    }
    repair_parse_guide(value)
}

/// 逐字段容错解析：整篇攻略因为单个字段类型不符（`recommended_day: ""`、
/// `opening_hours: "07:00-18:00"`、`best_for: "历史人文"` …）而丢弃，
/// 代价远大于「删掉这一个字段、其余照用」。
///
/// 做法：带路径追踪地反序列化 → 失败就删掉出错的那个字段/数组元素 → 重试。
/// 循环上限 32 次后仍失败才报错。
fn repair_parse_guide(mut value: serde_json::Value) -> Result<CityGuide, TravelParseError> {
    const MAX_ATTEMPTS: usize = 32;
    let mut last_error = String::new();
    for _ in 0..MAX_ATTEMPTS {
        let text = match serde_json::to_string(&value) {
            Ok(text) => text,
            Err(error) => return Err(TravelParseError::InvalidJson(error.to_string())),
        };
        let mut deserializer = serde_json::Deserializer::from_str(&text);
        match serde_path_to_error::deserialize::<_, CityGuide>(&mut deserializer) {
            Ok(guide) => return Ok(guide),
            Err(error) => {
                let path = error.path().to_string();
                last_error = format!("{path}: {}", error.into_inner());
                if !drop_json_path(&mut value, &path) {
                    break;
                }
            }
        }
    }
    Err(TravelParseError::InvalidJson(last_error))
}

/// 按 `serde_path_to_error` 给出的路径删掉对应字段（或数组元素）。
///
/// 支持 `a.b[0].c` 形态；路径为空、越界或字段不存在时返回 `false`（调用方停止重试）。
fn drop_json_path(value: &mut serde_json::Value, path: &str) -> bool {
    enum Step<'a> {
        Key(&'a str),
        Index(usize),
    }

    let mut steps: Vec<Step> = Vec::new();
    for segment in path.split('.') {
        let (name, rest) = match segment.find('[') {
            Some(bracket) => (&segment[..bracket], &segment[bracket..]),
            None => (segment, ""),
        };
        if !name.is_empty() {
            steps.push(Step::Key(name));
        }
        let mut rest = rest;
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(close) = inner.find(']') else {
                return false;
            };
            let Ok(index) = inner[..close].parse::<usize>() else {
                return false;
            };
            steps.push(Step::Index(index));
            rest = &inner[close + 1..];
        }
    }
    if steps.is_empty() {
        return false;
    }

    let mut cursor = value;
    for step in &steps[..steps.len() - 1] {
        cursor = match step {
            Step::Key(key) => match cursor.get_mut(*key) {
                Some(next) => next,
                None => return false,
            },
            Step::Index(index) => match cursor.get_mut(*index) {
                Some(next) => next,
                None => return false,
            },
        };
    }

    match &steps[steps.len() - 1] {
        Step::Key(key) => match cursor.as_object_mut() {
            Some(object) => object.remove(*key).is_some(),
            None => false,
        },
        Step::Index(index) => match cursor.as_array_mut() {
            Some(array) if *index < array.len() => {
                array.remove(*index);
                true
            }
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{TravelParseError, extract_json, parse_facts_json, parse_guide_json};
    use crate::travel::model::FactCategory;

    #[test]
    fn guide_parse_drops_only_the_offending_field() {
        // 单字段类型漂移（recommended_day 空串 / best_for 裸字符串 / opening_hours 裸字符串）
        // 不能把整篇攻略丢掉：出问题的字段被删掉，其余照常保留。
        let raw = r#"{
            "city": {"name": "杭州"},
            "summary": "西湖为主",
            "attractions": [{
                "name": "西湖",
                "why_go": "湖山相映",
                "best_for": "历史人文",
                "recommended_day": "",
                "opening_hours": "全天",
                "tips": "秋季桂花最盛"
            }],
            "itinerary_days": [{"day": 1, "title": "第 1 天", "stops": [{"name": "西湖", "time": "09:00"}]}]
        }"#;
        let guide = parse_guide_json(raw).expect("guide survives");
        assert_eq!(guide.attractions.len(), 1);
        assert_eq!(guide.attractions[0].name, "西湖");
        assert_eq!(guide.attractions[0].best_for, vec!["历史人文".to_string()]);
        assert!(guide.attractions[0].recommended_day.is_none());
        assert_eq!(
            guide.attractions[0]
                .opening_hours
                .as_ref()
                .map(|v| v.value.as_str()),
            Some("全天")
        );
        assert_eq!(guide.itinerary_days.len(), 1);
        assert_eq!(guide.itinerary_days[0].stops.len(), 1);
    }

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
