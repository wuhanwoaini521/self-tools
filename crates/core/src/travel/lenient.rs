//! LLM 输出的宽松反序列化助手。
//!
//! 小模型 / 在线模型都常在「数组字段」上退化成单个字符串
//! （例如 `"best_for": "历史人文"` 而不是 `["历史人文"]`）。
//! 这里把这些字段统一收敛成 `Vec<String>`，避免整份攻略因一处类型不符被丢弃——
//! 解析失败会让整篇攻略降级成「仅来源列表」，代价远大于宽容一点的收益。

use serde::{Deserialize, Deserializer};

use crate::travel::guide::VerifiedValue;

/// 接受 `["a", "b"]`、`"a"` 两种形态，统一成 `Vec<String>`；空串视为空数组。
pub fn string_seq<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(value) => {
            if value.trim().is_empty() {
                Vec::new()
            } else {
                vec![value]
            }
        }
        OneOrMany::Many(values) => values,
    })
}

/// 接受 `{"value":"07:00-18:00",...}`、`"07:00-18:00"`、`""`、`null` 四种形态。
///
/// LLM 经常把多源验证字段写成一个裸字符串（甚至空串）；整篇攻略不应因此报废，
/// 裸字符串按「未经多源验证」处理（`verified = false`, `confidence = "low"`）。
pub fn opt_verified<'de, D>(deserializer: D) -> Result<Option<VerifiedValue>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape {
        Text(String),
        Full(VerifiedValue),
    }

    match Option::<Shape>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Shape::Text(text)) => {
            let text = text.trim();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(VerifiedValue {
                    value: text.to_string(),
                    confidence: "low".to_string(),
                    verified: false,
                    ..VerifiedValue::default()
                }))
            }
        }
        Some(Shape::Full(value)) => Ok(Some(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::string_seq;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(default, deserialize_with = "string_seq")]
        items: Vec<String>,
    }

    #[test]
    fn verified_accepts_text_object_empty_and_null() {
        #[derive(Deserialize)]
        struct Holder {
            #[serde(default, deserialize_with = "super::opt_verified")]
            hours: Option<crate::travel::guide::VerifiedValue>,
        }

        let text: Holder = serde_json::from_str(r#"{"hours":"07:00-18:00"}"#).expect("text");
        assert_eq!(
            text.hours.as_ref().map(|v| v.value.as_str()),
            Some("07:00-18:00")
        );
        assert!(!text.hours.as_ref().expect("some").verified);

        let empty: Holder = serde_json::from_str(r#"{"hours":""}"#).expect("empty");
        assert!(empty.hours.is_none());

        let null: Holder = serde_json::from_str(r#"{"hours":null}"#).expect("null");
        assert!(null.hours.is_none());

        let full: Holder = serde_json::from_str(
            r#"{"hours":{"value":"09:00-17:00","confidence":"high","verified":true}}"#,
        )
        .expect("object");
        assert!(full.hours.expect("some").verified);
    }

    #[test]
    fn accepts_array_and_single_string() {
        let many: Holder = serde_json::from_str(r#"{"items":["a","b"]}"#).expect("array");
        assert_eq!(many.items, vec!["a".to_string(), "b".to_string()]);
        let one: Holder = serde_json::from_str(r#"{"items":"历史人文"}"#).expect("string");
        assert_eq!(one.items, vec!["历史人文".to_string()]);
        let empty: Holder = serde_json::from_str(r#"{"items":""}"#).expect("empty");
        assert!(empty.items.is_empty());
        let missing: Holder = serde_json::from_str("{}").expect("missing");
        assert!(missing.items.is_empty());
    }
}
