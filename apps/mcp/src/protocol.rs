//! JSON-RPC 2.0 协议帧（MCP 2026-07-28 的传输基础）。
//!
//! **薄协议 adapter**（V8 §1）：只编码/解码帧与方法名，不含任何工具语义。
//! 工具语义全部在 `devtoolbox_application::mcp`（ToolRegistry 派生）。
//!
//! 支持的 MCP 方法（V8 P0 范围）：
//! - `initialize`
//! - `ping`
//! - `tools/list`
//! - `tools/call`
//!
//! 明确**不**实现（P1/P2，V8 §61-§65）：`resources/*`、`prompts/*`、
//! `sampling/*`、`elicitation/*`、Tasks extension。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP 协议版本（目标规范 2026-07-28；`initialize` 协商用）。
pub const MCP_PROTOCOL_VERSION: &str = "2026-07-28";

/// 服务端标识（`initialize` 的 serverInfo）。
pub const SERVER_NAME: &str = "self-tools";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 请求帧。
#[derive(Clone, Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// 是否是通知（无 id → 不需要响应）。
    #[must_use]
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// 成功响应帧。
#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub result: Value,
}

/// 错误响应帧。
#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcError {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub error: JsonRpcErrorBody,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC 标准错误码。
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// self-tools 自定义错误码（应用层语义，与 §41 四态对齐）。
pub const UNAUTHENTICATED: i64 = -32001;
pub const INVALID_TOKEN: i64 = -32002;
pub const EXPIRED_TOKEN: i64 = -32003;
pub const INSUFFICIENT_SCOPE: i64 = -32004;
pub const PROVIDER_UNAVAILABLE: i64 = -32005;

impl JsonRpcResponse {
    #[must_use]
    pub fn new(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result,
        }
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            serde_json::to_string(&JsonRpcError::new(
                Value::Null,
                INTERNAL_ERROR,
                "response encode failed".into(),
                None,
            ))
            .unwrap_or_default()
        })
    }
}

impl JsonRpcError {
    #[must_use]
    pub fn new(id: Value, code: i64, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            error: JsonRpcErrorBody {
                code,
                message,
                data,
            },
        }
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// `initialize` 结果（MCP 协议形状）。
#[derive(Clone, Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: &'static str,
    pub capabilities: Capabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Capabilities {
    /// V8 P0 只声明 tools（Resources/Prompts 为 P1/P2）。
    pub tools: ToolsCapability,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "std::ops::Not::not")]
    pub list_changed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: &'static str,
}

/// `tools/list` 结果。
#[derive(Clone, Debug, Serialize)]
pub struct ToolsListResult {
    pub tools: Vec<Value>,
}

/// `tools/call` 结果（MCP content 形状）。
#[derive(Clone, Debug, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError", skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

impl ToolCallResult {
    /// 结构化结果（保留完整 JSON 供 client 使用）。
    #[must_use]
    pub fn structured(payload: Value) -> Self {
        let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
        Self {
            content: vec![ContentBlock {
                kind: "text",
                text,
            }],
            is_error: false,
            structured_content: Some(payload),
        }
    }

    /// 业务结果但带确认语义（§55：正常返回，不是错误）。
    #[must_use]
    pub fn confirmation(payload: Value) -> Self {
        Self::structured(payload)
    }

    /// 错误结果（`isError = true`；文本为受控消息，不含 secret）。
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock {
                kind: "text",
                text: message.into(),
            }],
            is_error: true,
            structured_content: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_frame_decodes() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        let request: JsonRpcRequest = serde_json::from_str(raw).expect("decode");
        assert_eq!(request.method, "tools/list");
        assert_eq!(request.id, Some(json!(1)));
        assert!(!request.is_notification());
    }

    #[test]
    fn notification_has_no_id() {
        let raw = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let request: JsonRpcRequest = serde_json::from_str(raw).expect("decode");
        assert!(request.is_notification());
        assert_eq!(request.id, None);
    }

    #[test]
    fn malformed_frame_is_parse_error() {
        let broken = serde_json::from_str::<JsonRpcRequest>("{not json");
        assert!(broken.is_err());
    }

    #[test]
    fn response_and_error_frames_encode() {
        let response = JsonRpcResponse::new(json!(1), json!({"ok": true}));
        let line = response.to_line();
        assert!(line.contains("\"jsonrpc\":\"2.0\""));
        assert!(line.contains("\"id\":1"));

        let error = JsonRpcError::new(json!(2), METHOD_NOT_FOUND, "no method".into(), None);
        let line = error.to_line();
        assert!(line.contains("-32601"));
        assert!(line.contains("no method"));
    }

    #[test]
    fn initialize_advertises_tools_only() {
        let result = InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION,
            capabilities: Capabilities::default(),
            server_info: ServerInfo {
                name: SERVER_NAME,
                version: SERVER_VERSION,
            },
        };
        let json = serde_json::to_value(&result).expect("json");
        assert_eq!(json["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(json["capabilities"]["tools"].is_object());
        // V8 P0 不声明 resources / prompts。
        assert!(json["capabilities"].get("resources").is_none());
        assert!(json["capabilities"].get("prompts").is_none());
    }

    #[test]
    fn tool_call_result_shapes() {
        let ok = ToolCallResult::structured(json!({"echo": 1}));
        assert!(!ok.is_error);
        assert_eq!(ok.structured_content, Some(json!({"echo": 1})));
        assert_eq!(ok.content[0].kind, "text");

        let failed = ToolCallResult::error("boom");
        assert!(failed.is_error);
        assert!(failed.structured_content.is_none());
    }

    #[test]
    fn custom_auth_error_codes_are_stable() {
        // §41：四态 + provider 不可用；不与 JSON-RPC 标准码冲突。
        for code in [UNAUTHENTICATED, INVALID_TOKEN, EXPIRED_TOKEN, INSUFFICIENT_SCOPE, PROVIDER_UNAVAILABLE] {
            assert!((-32099..=-32000).contains(&code), "{code}");
        }
        assert_eq!(UNAUTHENTICATED, -32001);
        assert_eq!(INSUFFICIENT_SCOPE, -32004);
    }
}
