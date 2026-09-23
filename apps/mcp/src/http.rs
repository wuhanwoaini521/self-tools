//! Streamable HTTP transport（V8 §43-§51）。
//!
//! MCP 2026-era 的 stateless request/response 模型（§49）：每次 `POST /mcp`
//! 是一个完整 JSON-RPC 交换；**不**建永久 MCP session server state。
//!
//! 安全边界（§44-§46/§83-§85）：
//! - 默认只绑 loopback；LAN 绑定需要显式 `remote_enabled` **且**已配置 identity；
//! - bearer 只从 `Authorization` header 读（§32）；
//! - body / 参数 / 响应三重上限；
//! - SYSTEM 工具返回 `confirmation_required`（§55），不在本层执行。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use devtoolbox_application::mcp::{McpAuthError, McpService};
use devtoolbox_core::mcp::McpCredential;

use crate::protocol::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, METHOD_NOT_FOUND, PARSE_ERROR, UNAUTHENTICATED,
};

/// 传输配置（V8 §44-§47）。
#[derive(Clone, Debug, PartialEq)]
pub struct HttpTransportConfig {
    /// 监听地址（默认 `127.0.0.1`，§44）。
    pub bind: String,
    /// 远程（非 loopback）访问开关；默认 false（§45/§7：默认 OFF）。
    pub remote_enabled: bool,
    /// body 上限（字节，§83）。
    pub max_body_bytes: usize,
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            remote_enabled: false,
            max_body_bytes: 1024 * 1_024,
        }
    }
}

impl HttpTransportConfig {
    /// 启动门禁（§46）：请求远程绑定但没有远程开关 → 拒绝启动。
    ///
    /// `identity_configured` 由组合根提供（是否装了 RemoteIdentityProvider）。
    pub fn validate_startup(&self, identity_configured: bool) -> Result<(), String> {
        let is_loopback = self.bind.starts_with("127.0.0.1")
            || self.bind.starts_with("localhost")
            || self.bind.starts_with("[::1]");
        if is_loopback {
            return Ok(());
        }
        if !self.remote_enabled {
            return Err(format!(
                "remote bind {} requested but mcp.remote_enabled = false（§46：不能裸开）",
                self.bind
            ));
        }
        if !identity_configured {
            return Err(format!(
                "remote bind {} requested but no identity provider configured（§45）",
                self.bind
            ));
        }
        Ok(())
    }

    /// 非 loopback 绑定是否被允许（供测试与 UI 状态展示）。
    #[must_use]
    pub fn allows_non_loopback(&self, identity_configured: bool) -> bool {
        self.remote_enabled && identity_configured
    }
}

/// 共享状态（axum handler 用）。
#[derive(Clone)]
pub struct HttpState {
    service: Arc<McpService>,
    /// loopback 请求是否免认证（本地受信，§5/§22）。
    loopback_trusted: bool,
    max_body_bytes: usize,
}

impl HttpState {
    /// 构造共享状态。
    #[must_use]
    pub fn new(service: Arc<McpService>, loopback_trusted: bool, max_body_bytes: usize) -> Self {
        Self {
            service,
            loopback_trusted,
            max_body_bytes,
        }
    }
}

/// 构造 axum Router（`POST /mcp` 单端点；stateless）。
pub fn router(state: HttpState) -> Router {
    Router::new()
        .route("/mcp", post(handle_mcp))
        .with_state(state)
}

/// `POST /mcp`：一个 JSON-RPC 帧进，一个帧出。
async fn handle_mcp(
    State(state): State<HttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if body.len() > state.max_body_bytes {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
    }
    let request: JsonRpcRequest = match serde_json::from_str(&body) {
        Ok(request) => request,
        Err(_) => {
            return json_error_frame(
                StatusCode::OK,
                &JsonRpcError::new(
                    serde_json::Value::Null,
                    PARSE_ERROR,
                    "parse error".into(),
                    None,
                ),
            );
        }
    };
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);

    // 认证（§26/§32）：本地 loopback 可免 bearer（§5 Local != Remote）。
    // 本地受信判定依据**真实对端 IP**（不再靠 header 猜测）。
    // client_id 每请求唯一（§103/§67：同机不同 loopback 客户端不得共享 session）。
    let principal = if state.loopback_trusted
        && is_loopback_addr(&peer)
        && !has_proxy_headers(&headers)
    {
        // 本地 loopback = LOCAL_TRUSTED（§5/§22）：给宽 read scope，
        // 写 / SYSTEM 仍由 exposure 与 SafeAction 把关。
        crate::stdio::local_principal(&format!("http-loopback-{}", next_nonce()))
    } else {
        match credential_from_headers(&headers) {
            Some(credential) => match state.service.authenticate(&credential) {
                Ok(principal) => principal,
                Err(McpAuthError::ProviderUnavailable) => {
                    return json_error_frame(
                        StatusCode::SERVICE_UNAVAILABLE,
                        &JsonRpcError::new(
                            id,
                            crate::protocol::PROVIDER_UNAVAILABLE,
                            "identity provider unavailable".into(),
                            None,
                        ),
                    );
                }
                Err(error) => {
                    let code = match error {
                        McpAuthError::Unauthenticated => UNAUTHENTICATED,
                        McpAuthError::InvalidToken => crate::protocol::INVALID_TOKEN,
                        McpAuthError::ExpiredToken => crate::protocol::EXPIRED_TOKEN,
                        McpAuthError::InsufficientScope => crate::protocol::INSUFFICIENT_SCOPE,
                        McpAuthError::ProviderUnavailable => crate::protocol::PROVIDER_UNAVAILABLE,
                    };
                    return json_error_frame(
                        StatusCode::UNAUTHORIZED,
                        &JsonRpcError::new(id, code, error.code().into(), None),
                    );
                }
            },
            None => {
                return json_error_frame(
                    StatusCode::UNAUTHORIZED,
                    &JsonRpcError::new(
                        id,
                        UNAUTHENTICATED,
                        "missing bearer credentials".into(),
                        None,
                    ),
                );
            }
        }
    };

    let result: Result<serde_json::Value, String> = match request.method.as_str() {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": crate::protocol::MCP_PROTOCOL_VERSION,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": crate::protocol::SERVER_NAME,
                "version": crate::protocol::SERVER_VERSION,
            },
        })),
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => match state.service.list_tools(&principal) {
            Ok(definitions) => Ok(serde_json::to_value(crate::protocol::ToolsListResult {
                tools: definitions
                    .into_iter()
                    .map(|definition| serde_json::to_value(&definition).unwrap_or_default())
                    .collect(),
            })
            .unwrap_or_default()),
            Err(_) => {
                return json_error_frame(
                    StatusCode::FORBIDDEN,
                    &JsonRpcError::new(id, UNAUTHENTICATED, "discovery denied".into(), None),
                );
            }
        },
        "tools/call" => {
            let Some(params) = request.params.as_ref() else {
                return json_error_frame(
                    StatusCode::OK,
                    &JsonRpcError::new(
                        id,
                        crate::protocol::INVALID_PARAMS,
                        "params required".into(),
                        None,
                    ),
                );
            };
            let Some(name) = params.get("name").and_then(serde_json::Value::as_str) else {
                return json_error_frame(
                    StatusCode::OK,
                    &JsonRpcError::new(
                        id,
                        crate::protocol::INVALID_PARAMS,
                        "`name` is required".into(),
                        None,
                    ),
                );
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            // 与 STDIO 一致：arguments 必须是对象（审查 MCP-H）。
            if !arguments.is_object() {
                return json_error_frame(
                    StatusCode::OK,
                    &JsonRpcError::new(
                        id,
                        crate::protocol::INVALID_PARAMS,
                        "`arguments` must be an object".into(),
                        None,
                    ),
                );
            }
            match state.service.call_tool(&principal, name, arguments).await {
                Ok(invocation) => Ok(serde_json::to_value(crate::stdio::payload_to_result(
                    &invocation,
                ))
                .unwrap_or_default()),
                Err(devtoolbox_application::mcp::McpCallError::Unauthenticated) => {
                    return json_error_frame(
                        StatusCode::UNAUTHORIZED,
                        &JsonRpcError::new(id, UNAUTHENTICATED, "unauthenticated".into(), None),
                    );
                }
                Err(devtoolbox_application::mcp::McpCallError::Denied(reason)) => {
                    return json_error_frame(
                        StatusCode::FORBIDDEN,
                        &JsonRpcError::new(
                            id,
                            crate::protocol::INSUFFICIENT_SCOPE,
                            format!("denied: {reason}"),
                            None,
                        ),
                    );
                }
                Err(error) => Ok(serde_json::to_value(crate::protocol::ToolCallResult::error(
                    error.to_string(),
                ))
                .unwrap_or_default()),
            }
        }
        other => {
            return json_error_frame(
                StatusCode::OK,
                &JsonRpcError::new(
                    id,
                    METHOD_NOT_FOUND,
                    format!("method not found: {other}"),
                    None,
                ),
            );
        }
    };

    match result {
        Ok(value) => json_response(&JsonRpcResponse::new(id, value)),
        Err(message) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

/// bearer 只从 `Authorization` header 读（§32）。
fn credential_from_headers(headers: &HeaderMap) -> Option<McpCredential> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(McpCredential::from_authorization_header)
}

/// 真实对端是否 loopback（axum `ConnectInfo<SocketAddr>` 提供，§5）。
fn is_loopback_addr(peer: &std::net::SocketAddr) -> bool {
    peer.ip().is_loopback()
}

/// 是否带代理头（带则不能假定本地直连）。
fn has_proxy_headers(headers: &HeaderMap) -> bool {
    headers.get("x-forwarded-for").is_some() || headers.get("x-real-ip").is_some()
}

/// 每请求自增 nonce（本地 loopback client_id 的唯一性来源，§103）。
fn next_nonce() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn json_response(response: &JsonRpcResponse) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(response.to_line()))
        .unwrap_or_default()
}

fn json_error_frame(status: StatusCode, error: &JsonRpcError) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(error.to_line()))
        .unwrap_or_default()
}

fn json_error(status: StatusCode, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(message.to_string()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    fn build_app(state: HttpState) -> axum::Router {
        router(state)
    }

    /// `peer` 让测试显式控制「对端是否 loopback」（生产由 axum 注入真实 IP）。
    async fn post_json(
        app: axum::Router,
        path: &str,
        body: &str,
        auth: Option<&str>,
        peer: &str,
    ) -> (StatusCode, String) {
        let mut builder = Request::builder().method("POST").uri(path);
        if let Some(value) = auth {
            builder = builder.header("authorization", value);
        }
        let mut request = builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request");
        // ConnectInfo 从请求扩展读取（测试手动注入）。
        request.extensions_mut().insert(ConnectInfo(
            peer.parse::<std::net::SocketAddr>().expect("peer addr"),
        ));
        let response = app.oneshot(request).await.expect("response");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    const LOOPBACK: &str = "127.0.0.1:50000";
    const REMOTE: &str = "192.168.1.50:50000";

    fn loopback_state() -> HttpState {
        let harness = TestHarness::new();
        HttpState {
            service: harness.service(),
            loopback_trusted: true,
            max_body_bytes: 1024 * 1_024,
        }
    }

    #[tokio::test]
    async fn loopback_initialize_and_tools_list() {
        let app = build_app(loopback_state());
        let (status, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("2026-07-28"), "{body}");

        let app = build_app(loopback_state());
        let (_, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert!(body.contains("memory.search"), "{body}");
    }

    #[tokio::test]
    async fn remote_without_bearer_is_unauthorized() {
        // 非 loopback 可信 → 无凭证即 401（Demo I）。
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: false,
            max_body_bytes: 1024 * 1_024,
        });
        let (status, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("-32001"), "{body}");
    }

    #[tokio::test]
    async fn valid_bearer_lists_authorized_tools_only() {
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: false,
            max_body_bytes: 1024 * 1_024,
        });
        let (_, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            Some("Bearer tok-server"),
            REMOTE,
        )
        .await;
        // p-2 只有 server.read：memory 工具不可见（§148）。
        assert!(!body.contains("memory.search"), "{body}");
    }

    #[tokio::test]
    async fn invalid_bearer_is_rejected_without_echoing_token() {
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: false,
            max_body_bytes: 1024 * 1_024,
        });
        let (status, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            Some("Bearer super-secret-token"),
            REMOTE,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            !body.contains("super-secret-token"),
            "token 不得回显: {body}"
        );
    }

    #[tokio::test]
    async fn remote_peer_without_bearer_is_denied_even_without_proxy_headers() {
        // 回归（审查 A）：LAN 客户端不带任何转发头也不能冒充本地受信 ——
        // 判据是真实对端 IP，不是 header 缺失。
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: true,
            max_body_bytes: 1024 * 1_024,
        });
        let (status, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            None,
            REMOTE,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert!(body.contains("-32001"), "{body}");
    }

    #[tokio::test]
    async fn loopback_peer_with_proxy_header_is_not_trusted() {
        // 代理头存在 → 不当本地直连（避免伪造）。
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: true,
            max_body_bytes: 1024 * 1_024,
        });
        let mut request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("x-forwarded-for", "10.0.0.5")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#.to_string(),
            ))
            .expect("request");
        request.extensions_mut().insert(ConnectInfo(
            LOOPBACK.parse::<std::net::SocketAddr>().expect("addr"),
        ));
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn malformed_frame_is_parse_error() {
        let app = build_app(loopback_state());
        let (status, body) = post_json(app, "/mcp", "{oops", None, LOOPBACK).await;
        assert_eq!(status, StatusCode::OK, "JSON-RPC 错误仍以 200 返回");
        assert!(body.contains("-32700"), "{body}");
    }

    #[tokio::test]
    async fn unknown_method_is_method_not_found() {
        let app = build_app(loopback_state());
        let (_, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert!(body.contains("-32601"), "{body}");
    }

    #[tokio::test]
    async fn tools_call_executes_through_registry() {
        let app = build_app(loopback_state());
        let (_, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory.search","arguments":{"query":"docker"}}}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert!(body.contains("docker"), "{body}");
        assert!(!body.contains("\"isError\":true"), "{body}");
    }

    #[tokio::test]
    async fn non_object_arguments_are_rejected() {
        // 与 STDIO 一致（审查 MCP-H）：arguments 必须是对象。
        let app = build_app(loopback_state());
        let (_, body) = post_json(
            app,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory.search","arguments":"oops"}}"#,
            None,
            LOOPBACK,
        )
        .await;
        assert!(body.contains("-32602"), "{body}");
    }

    #[tokio::test]
    async fn loopback_client_ids_are_unique_per_request() {
        // §103/§67：同机不同 loopback 客户端不得共享 session（审查 MCP-C）。
        assert_ne!(next_nonce(), next_nonce());
        let first = crate::stdio::local_principal(&format!("http-loopback-{}", next_nonce()));
        let second = crate::stdio::local_principal(&format!("http-loopback-{}", next_nonce()));
        assert_ne!(first.client_id, second.client_id);
        assert!(first.authenticated);
        assert_eq!(
            first.trust,
            devtoolbox_core::mcp::McpTrustLevel::LocalTrusted
        );
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        // §83：传输层 body 上限。
        let harness = TestHarness::new();
        let app = build_app(HttpState {
            service: harness.service(),
            loopback_trusted: true,
            max_body_bytes: 64,
        });
        let (status, _) = post_json(
            app,
            "/mcp",
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"memory.search","arguments":{{"query":"{}"}}}}}}"#,
                "x".repeat(200)
            ),
            None,
            LOOPBACK,
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn startup_gate_blocks_unauthorized_remote_bind() {
        // §46：请求远程绑定但没有 remote_enabled / identity → 启动失败。
        let remote = HttpTransportConfig {
            bind: "0.0.0.0".into(),
            remote_enabled: false,
            max_body_bytes: 1024,
        };
        assert!(remote.validate_startup(true).is_err());
        assert!(remote.validate_startup(false).is_err());

        let no_identity = HttpTransportConfig {
            bind: "0.0.0.0".into(),
            remote_enabled: true,
            max_body_bytes: 1024,
        };
        assert!(no_identity.validate_startup(false).is_err());
        assert!(no_identity.validate_startup(true).is_ok());

        // loopback 始终允许（§44）。
        let loopback = HttpTransportConfig::default();
        assert!(loopback.validate_startup(false).is_ok());
        assert!(!loopback.allows_non_loopback(false));
    }

    #[test]
    fn loopback_trust_requires_a_real_loopback_peer() {
        // 对端 IP 是唯一判据（§5：LAN != trusted）。
        assert!(is_loopback_addr(&"127.0.0.1:5000".parse().expect("addr")));
        assert!(is_loopback_addr(&"[::1]:5000".parse().expect("addr")));
        assert!(!is_loopback_addr(
            &"192.168.1.50:5000".parse().expect("addr")
        ));
        // 代理头让「本地直连」假设失效。
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.5".parse().expect("header"));
        assert!(has_proxy_headers(&headers));
        assert!(!has_proxy_headers(&HeaderMap::new()));
    }
}
