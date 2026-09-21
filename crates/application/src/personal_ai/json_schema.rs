//! 迷你 JSON Schema 校验（工具入参验证专用子集）。
//!
//! V4 §114：Tool 参数错误必须产生 validation error，不能将任意 JSON 直接
//! 传入业务层。本模块实现**确定性、可单测**的校验子集（不入 jsonschema
//! 依赖，保持 workspace 依赖最小；工具 schema 由本仓库自持，受限子集足够）：
//!
//! - `type`: string / integer / number / boolean / object / array
//! - `required`: 对象必填键
//! - `properties`: 对象子校验
//! - `items`: 数组元素校验（对象或 scalar）
//! - `enum`: 允许值
//! - 未知关键字忽略（`additionalProperties` 默认允许）

use std::collections::BTreeSet;

use serde_json::Value;

/// 校验器。错误信息形如 `$.query: expected type string, got number`。
pub struct Validator;

impl Validator {
    pub fn validate(schema: &Value, instance: &Value) -> Result<(), String> {
        Self::walk(schema, instance, "$")
    }

    fn walk(schema: &Value, instance: &Value, path: &str) -> Result<(), String> {
        if !schema.is_object() {
            // 无 schema（或非对象）视为放行。
            return Ok(());
        }
        if let Some(expected) = schema.get("type").and_then(Value::as_str)
            && !type_matches(expected, instance)
        {
            return Err(format!(
                "{path}: expected type `{expected}`, got {}",
                type_label(instance)
            ));
        }
        if let Some(values) = schema.get("enum").and_then(Value::as_array)
            && !values.iter().any(|value| value == instance)
        {
            return Err(format!("{path}: value not in allowed enum"));
        }
        match instance {
            Value::Object(map) => {
                if let Some(required) = schema.get("required").and_then(Value::as_array) {
                    for key in required {
                        if let Some(key) = key.as_str()
                            && !map.contains_key(key)
                        {
                            return Err(format!("{path}: missing required property `{key}`"));
                        }
                    }
                }
                if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                    for (key, sub_schema) in properties {
                        if let Some(value) = map.get(key) {
                            Self::walk(sub_schema, value, &format!("{path}.{key}"))?;
                        }
                    }
                }
            }
            Value::Array(items) => {
                if let Some(item_schema) = schema.get("items") {
                    for (index, item) in items.iter().enumerate() {
                        Self::walk(item_schema, item, &format!("{path}[{index}]"))?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn type_matches(expected: &str, instance: &Value) -> bool {
    match expected {
        "string" => instance.is_string(),
        "integer" => instance.is_i64() || instance.is_u64(),
        "number" => instance.is_number(),
        "boolean" => instance.is_boolean(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "null" => instance.is_null(),
        _ => true, // 未知 type 关键字放行
    }
}

fn type_label(instance: &Value) -> &'static str {
    match instance {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 校验对象键集合（测试/审计用：schema 是否覆盖所有声明键）。
#[must_use]
pub fn object_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_required_and_types() {
        let schema = json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"},
                "entity_type": {"type": "string", "enum": ["person", "event", "work", "story"]}
            }
        });
        assert!(Validator::validate(&schema, &json!({"query": "遵义"})).is_ok());
        assert!(Validator::validate(&schema, &json!({"query": 1})).is_err());
        assert!(Validator::validate(&schema, &json!({})).is_err());
        assert!(Validator::validate(&schema, &json!({"query": "x", "limit": 3})).is_ok());
        assert!(Validator::validate(&schema, &json!({"query": "x", "limit": "3"})).is_err());
        assert!(
            Validator::validate(&schema, &json!({"query": "x", "entity_type": "place"})).is_err()
        );
    }

    #[test]
    fn array_items_validate() {
        let schema = json!({
            "type": "array",
            "items": {"type": "object", "required": ["id"], "properties": {"id": {"type": "string"}}}
        });
        assert!(Validator::validate(&schema, &json!([{"id": "a"}, {"id": "b"}])).is_ok());
        assert!(Validator::validate(&schema, &json!([{"id": 1}])).is_err());
        assert!(Validator::validate(&schema, &json!("not array")).is_err());
    }

    #[test]
    fn unknown_keywords_ignored() {
        let schema = json!({"type": "object", "additionalProperties": false, "minLength": 2});
        assert!(Validator::validate(&schema, &json!({"a": 1})).is_ok());
    }

    #[test]
    fn top_level_scalar() {
        assert!(Validator::validate(&json!({"type": "string"}), &json!("hi")).is_ok());
        assert!(Validator::validate(&json!({"type": "string"}), &json!(1)).is_err());
    }
}
