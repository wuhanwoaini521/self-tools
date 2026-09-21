//! 模块工具的参数解析与错误转换助手（V6 §100：模块同构，不重复实现）。
//!
//! 四个知识模块（memory / documents / files / knowledge）共用同一套薄解析：
//! 参数形状校验是**模块无关**的机械工作，重复定义只会让「必填缺失怎么报」这类
//! 契约在各模块间漂移。

use devtoolbox_core::AgentError;

use crate::error::ApplicationError;

/// 取可选字符串参数：trim 后为空视为缺失。
#[must_use]
pub fn optional_string(arguments: &serde_json::Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 取必填字符串参数（缺失 → 参数错误，不进执行失败）。
pub fn require_string(arguments: &serde_json::Value, key: &str) -> Result<String, AgentError> {
    optional_string(arguments, key)
        .ok_or_else(|| AgentError::tool_invalid_argument(format!("`{key}` is required")))
}

/// 取可选整数参数（非数字 → 视为缺失，由调用方决定默认值）。
#[must_use]
pub fn usize_arg(arguments: &serde_json::Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
}

/// 应用层错误 → 工具执行失败（模块层唯一转换点）。
pub fn tool_error(error: ApplicationError) -> AgentError {
    AgentError::tool_execution_failed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn optional_string_trims_and_drops_empty() {
        assert_eq!(
            optional_string(&json!({"q": "  docker "}), "q").as_deref(),
            Some("docker")
        );
        assert_eq!(optional_string(&json!({"q": "   "}), "q"), None);
        assert_eq!(optional_string(&json!({"q": 7}), "q"), None);
        assert_eq!(optional_string(&json!({}), "q"), None);
    }

    #[test]
    fn require_string_reports_missing_key() {
        let error = require_string(&json!({}), "query").expect_err("missing");
        assert!(error.to_string().contains("query"), "{error}");
    }

    #[test]
    fn usize_arg_accepts_positive_integers_only() {
        assert_eq!(usize_arg(&json!({"limit": 5}), "limit"), Some(5));
        assert_eq!(usize_arg(&json!({"limit": -1}), "limit"), None);
        assert_eq!(usize_arg(&json!({"limit": "5"}), "limit"), None);
    }

    #[test]
    fn tool_error_keeps_message() {
        let error = tool_error(crate::error::ApplicationError::Knowledge {
            message: "索引不可用".to_string(),
        });
        assert!(error.to_string().contains("索引不可用"), "{error}");
    }
}
