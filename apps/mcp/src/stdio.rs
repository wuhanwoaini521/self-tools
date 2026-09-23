//! STDIO transport（V8 §24-§25）。
//!
//! 铁律：
//! - **stdout 只走协议帧**；任何日志走 stderr（§25）。本模块除
//!   `write_response` 外不直接写 stdout；
//! - 每行一个 JSON-RPC 帧（MCP STDIO 用 newline-delimited JSON）；
//! - 未知方法 → `-32601`；未知工具 → `-32602`；
//! - identity：本地 STDIO = `LOCAL_TRUSTED`（§22），仍过 exposure/scope 门禁。

use std::io::{BufRead, Write};
use std::sync::Arc;

use devtoolbox_application::mcp::{McpAuthError, McpCallError, McpService};
use devtoolbox_core::mcp::{McpCredential, McpPrincipal};

use crate::protocol::{
    ContentBlock, INTERNAL_ERROR, INVALID_PARAMS, JsonRpcError, JsonRpcRequest, JsonRpcResponse,
    MCP_PROTOCOL_VERSION, METHOD_NOT_FOUND, PARSE_ERROR, SERVER_NAME, SERVER_VERSION,
    ToolCallResult, ToolsListResult,
};

/// STDIO 服务器（transport-neutral：`McpService` 由组合根注入）。
pub struct StdioServer {
    service: Arc<McpService>,
}

impl StdioServer {
    #[must_use]
    pub fn new(service: Arc<McpService>) -> Self {
        Self { service }
    }

    /// 主循环：读一行 → 处理 → 写一行。返回退出码（0 = 正常 EOF）。
    ///
    /// `out` 参数让测试可以注入内存 writer 并断言 stdout 纯洁性（§104）。
    pub fn serve<R: BufRead, W: Write>(
        &self,
        reader: R,
        out: &mut W,
        principal: &McpPrincipal,
    ) -> Result<i32, std::io::Error> {
        // 调用方提供 runtime（bin 入口用多线程；测试复用外层 runtime）。
        // 不在此构造：嵌套 runtime 会 panic，且每次调用重建浪费。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(std::io::Error::other)?;
        self.serve_with(&runtime, reader, out, principal)
    }

    /// 用既有 runtime 服务（测试与嵌入场景）。
    pub fn serve_with<R: BufRead, W: Write>(
        &self,
        runtime: &tokio::runtime::Runtime,
        reader: R,
        out: &mut W,
        principal: &McpPrincipal,
    ) -> Result<i32, std::io::Error> {
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Some(response) = self.handle_line(trimmed, principal, runtime) else {
                continue; // 通知：不响应
            };
            writeln!(out, "{}", response.to_line())?;
            out.flush()?;
        }
        Ok(0)
    }

    /// 处理一行输入 → 可选响应帧（`None` = 通知）。
    fn handle_line(
        &self,
        line: &str,
        principal: &McpPrincipal,
        runtime: &tokio::runtime::Runtime,
    ) -> Option<JsonRpcResponse> {
        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(_) => {
                return Some(JsonRpcResponse::new(
                    serde_json::Value::Null,
                    serde_json::to_value(JsonRpcError::new(
                        serde_json::Value::Null,
                        PARSE_ERROR,
                        "parse error".into(),
                        None,
                    ))
                    .unwrap_or_default(),
                ));
            }
        };
        if request.is_notification() {
            // 通知不需要响应（MCP：`notifications/initialized` 等）。
            return None;
        }
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);
        let result = match request.method.as_str() {
            "initialize" => Ok(serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            })),
            "ping" => Ok(serde_json::json!({})),
            "tools/list" => self.list_tools(principal),
            "tools/call" => {
                runtime.block_on(async { self.call_tool(principal, request.params.as_ref()).await })
            }
            other => Err(JsonRpcError::new(
                id.clone(),
                METHOD_NOT_FOUND,
                format!("method not found: {other}"),
                None,
            )),
        };
        Some(match result {
            Ok(value) => JsonRpcResponse::new(id, value),
            Err(error) => JsonRpcResponse::new(
                id,
                serde_json::to_value(error).unwrap_or_else(|_| {
                    serde_json::to_value(JsonRpcError::new(
                        serde_json::Value::Null,
                        INTERNAL_ERROR,
                        "error encode failed".into(),
                        None,
                    ))
                    .unwrap_or_default()
                }),
            ),
        })
    }

    fn list_tools(&self, principal: &McpPrincipal) -> Result<serde_json::Value, JsonRpcError> {
        match self.service.list_tools(principal) {
            Ok(definitions) => {
                let tools: Vec<serde_json::Value> = definitions
                    .into_iter()
                    .map(|definition| serde_json::to_value(&definition).unwrap_or_default())
                    .collect();
                Ok(serde_json::to_value(ToolsListResult { tools }).unwrap_or_default())
            }
            Err(error) => Err(auth_error(serde_json::Value::Null, error)),
        }
    }

    async fn call_tool(
        &self,
        principal: &McpPrincipal,
        params: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let Some(params) = params else {
            return Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                "params required".into(),
                None,
            ));
        };
        let Some(name) = params.get("name").and_then(serde_json::Value::as_str) else {
            return Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                "`name` is required".into(),
                None,
            ));
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !arguments.is_object() {
            return Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                "`arguments` must be an object".into(),
                None,
            ));
        }
        match self.service.call_tool(principal, name, arguments).await {
            Ok(invocation) => {
                Ok(serde_json::to_value(payload_to_result(&invocation)).unwrap_or_default())
            }
            Err(McpCallError::Unauthenticated) => Err(JsonRpcError::new(
                serde_json::Value::Null,
                crate::protocol::UNAUTHENTICATED,
                "unauthenticated".into(),
                None,
            )),
            Err(McpCallError::Denied(reason)) => Err(JsonRpcError::new(
                serde_json::Value::Null,
                crate::protocol::INSUFFICIENT_SCOPE,
                format!("denied: {reason}"),
                None,
            )),
            Err(McpCallError::UnknownTool(tool)) => Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                format!("unknown tool: {tool}"),
                None,
            )),
            Err(McpCallError::InvalidParams(detail)) => Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                format!("invalid params: {detail}"),
                None,
            )),
            Err(McpCallError::PayloadTooLarge) => Err(JsonRpcError::new(
                serde_json::Value::Null,
                INVALID_PARAMS,
                "payload too large".into(),
                None,
            )),
            Err(McpCallError::ToolFailed(message)) => {
                Ok(serde_json::to_value(ToolCallResult::error(message)).unwrap_or_default())
            }
        }
    }
}

/// 认证错误 → JSON-RPC 错误（§41：具体失败态）。
fn auth_error(id: serde_json::Value, error: McpAuthError) -> JsonRpcError {
    let code = match error {
        McpAuthError::Unauthenticated => crate::protocol::UNAUTHENTICATED,
        McpAuthError::InvalidToken => crate::protocol::INVALID_TOKEN,
        McpAuthError::ExpiredToken => crate::protocol::EXPIRED_TOKEN,
        McpAuthError::InsufficientScope => crate::protocol::INSUFFICIENT_SCOPE,
        McpAuthError::ProviderUnavailable => crate::protocol::PROVIDER_UNAVAILABLE,
    };
    JsonRpcError::new(id, code, error.code().to_string(), None)
}

/// 把 invocation 的 payload 包成 MCP `tools/call` content 形状。
pub fn payload_to_result(
    invocation: &devtoolbox_application::mcp::McpInvocation,
) -> ToolCallResult {
    if invocation.is_error {
        let message = invocation
            .payload
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("tool failed")
            .to_string();
        return ToolCallResult::error(message);
    }
    ToolCallResult {
        content: vec![ContentBlock {
            kind: "text",
            text: serde_json::to_string_pretty(&invocation.payload).unwrap_or_default(),
        }],
        is_error: false,
        structured_content: Some(invocation.payload.clone()),
    }
}

/// `initialize` 的本地受信 principal（§22：local trusted ≠ root）。
#[must_use]
pub fn local_principal(client_id: &str) -> McpPrincipal {
    let mut principal = McpPrincipal::local(client_id);
    // 本地 STDIO 默认给宽 read scope（§69：READ 全允许）；
    // 写 / SYSTEM scope 由 exposure 与 SafeAction 继续把关。
    principal.scopes =
        vec![devtoolbox_core::mcp::McpScope::parse("selftools.read").expect("builtin scope")];
    principal
}

/// 无凭证（本地 STDIO 不经 bearer；身份由进程边界决定，§5）。
#[must_use]
pub fn local_credential() -> McpCredential {
    McpCredential::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use std::io::Cursor;

    /// 同步测试：自建 runtime 并复用（避免 `#[tokio::test]` 嵌套 runtime）。
    fn serve_input(input: &str) -> String {
        let harness = TestHarness::new();
        let server = StdioServer::new(harness.service());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let mut out: Vec<u8> = Vec::new();
        server
            .serve_with(
                &runtime,
                Cursor::new(input),
                &mut out,
                &local_principal("test"),
            )
            .expect("serve");
        String::from_utf8(out).expect("utf8")
    }

    #[test]
    fn initialize_and_ping_are_protocol_level() {
        let text = serve_input(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n",
        );
        assert!(text.contains("2026-07-28"), "{text}");
        assert!(text.contains("self-tools"));
    }

    #[test]
    fn tools_list_and_call_work_over_stdio() {
        let text = serve_input(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"memory.search\",\"arguments\":{\"query\":\"docker\"}}}\n",
        );
        assert!(text.contains("memory.search"), "{text}");
        assert!(text.contains("docker"), "{text}");
    }

    #[test]
    fn unknown_method_and_tool_return_protocol_errors() {
        let text = serve_input(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"nope/nope\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"shell.exec\"}}\n",
        );
        assert!(text.contains("-32601"), "未知方法: {text}");
        // 未注册 / 未暴露工具由授权层拒绝（不泄露工具是否存在）。
        assert!(text.contains("unknown_tool"), "{text}");
    }

    #[test]
    fn notifications_produce_no_response() {
        assert!(
            serve_input("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                .is_empty()
        );
    }

    #[test]
    fn malformed_frame_yields_parse_error() {
        assert!(serve_input("{not json\n").contains("-32700"));
    }

    #[test]
    fn stdout_stays_protocol_only() {
        // §104：stdout 每一行都必须是可解析的 JSON 帧。
        let text = serve_input("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n");
        for line in text.lines() {
            assert!(
                serde_json::from_str::<serde_json::Value>(line).is_ok(),
                "stdout 行必须是 JSON 帧: {line}"
            );
        }
    }

    #[test]
    fn invalid_arguments_shape_is_rejected() {
        let text = serve_input(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"memory.search\",\"arguments\":\"oops\"}}\n",
        );
        assert!(text.contains("-32602"), "{text}");
    }
}
