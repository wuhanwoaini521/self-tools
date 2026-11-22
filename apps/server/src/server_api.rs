//! Home Server 的 HTTP 面（Phase 1.4）。
//!
//! 只读能力（status / services / apps / recent actions / logs / mcp status）对
//! 登录后的 pad 开放；写操作（restart / confirm / cancel）必须携带网关验签注入的
//! [`AuthenticatedRemote`] 且角色为 `admin`。直接访问 loopback 的写请求没有这个
//! Extension，因此仍然不可用。
//!
//! 边界与桌面命令保持同形：`request_restart` 只签发票据，`confirm` 才执行；
//! 票据一次性、绑 principal 派生的 session、有过期时间。审计只记录动作/目标/
//! 结果/时间，**不记录请求体**。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path, Query};
use serde::Deserialize;
use serde_json::{Value, json};

use devtoolbox_runtime::server::ServerRuntime;

use crate::ai_api::ApiError;
use crate::gateway_auth::AuthenticatedRemote;

type ServerRuntimeExtension = Extension<Arc<ServerRuntime>>;

/// `POST /api/v1/server/actions/restart` 的请求体（与桌面命令同形）。
#[derive(Debug, Deserialize)]
pub struct RestartRequest {
    #[serde(rename = "serviceId", alias = "service_id")]
    pub service_id: String,
}

/// `POST .../confirm` 的请求体。
#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    #[serde(rename = "serviceId", alias = "service_id")]
    pub service_id: String,
}

/// `GET /api/v1/server/logs` 的查询参数（camelCase，兼容 snake_case）。
#[derive(Debug, Default, Deserialize)]
pub struct LogsQuery {
    #[serde(rename = "serviceId", alias = "service_id")]
    pub service_id: Option<String>,
    #[serde(rename = "logSourceId", alias = "log_source_id")]
    pub log_source_id: Option<String>,
    #[serde(rename = "maxLines", alias = "max_lines")]
    pub max_lines: Option<usize>,
    #[serde(rename = "maxBytes", alias = "max_bytes")]
    pub max_bytes: Option<usize>,
    #[serde(rename = "maxAgeSecs", alias = "max_age_secs")]
    pub max_age_secs: Option<u64>,
}

pub async fn status(Extension(server): ServerRuntimeExtension) -> Result<Json<Value>, ApiError> {
    let status = server.status().map_err(server_error)?;
    serde_json::to_value(&status)
        .map(Json)
        .map_err(|error| internal(error.to_string()))
}

pub async fn services(Extension(server): ServerRuntimeExtension) -> Json<Value> {
    let statuses = server.services.status_all();
    Json(Value::Array(
        statuses
            .iter()
            .map(|(service, status)| {
                json!({
                    "service_id": service.id,
                    "display_name": service.display_name,
                    "description": service.description,
                    "status": status.status.as_str(),
                    "detail": status.detail,
                    "checked_at": status.checked_at,
                    "allowed_actions": service.allowed_actions,
                })
            })
            .collect(),
    ))
}

pub async fn apps(Extension(server): ServerRuntimeExtension) -> Json<Value> {
    let statuses = server.apps.status_all();
    Json(Value::Array(
        statuses
            .iter()
            .map(|(app, status)| {
                json!({
                    "app_id": app.id,
                    "name": app.name,
                    "description": app.description,
                    "url": app.url,
                    "category": app.category,
                    "status": status.status.as_str(),
                    "detail": status.detail,
                })
            })
            .collect(),
    ))
}

pub async fn recent_actions(
    Extension(server): ServerRuntimeExtension,
    Query(query): Query<RecentQuery>,
) -> Json<Value> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let entries = server.recent_actions(limit);
    Json(Value::Array(
        entries
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "timestamp": entry.timestamp,
                    "source": entry.source.as_str(),
                    "action_type": entry.action_type,
                    "target_id": entry.target_id,
                    "risk": entry.risk.as_str(),
                    "confirmed": entry.confirmed,
                    "result": entry.result.as_str(),
                    "duration_ms": entry.duration_ms,
                    "error_code": entry.error_code,
                })
            })
            .collect(),
    ))
}

#[derive(Debug, Default, Deserialize)]
pub struct RecentQuery {
    pub limit: Option<usize>,
}

/// 有界日志（先 clamp 再脱敏；`untrusted: true` 与桌面命令一致）。
pub async fn logs(
    Extension(server): ServerRuntimeExtension,
    Query(query): Query<LogsQuery>,
) -> Result<Json<Value>, ApiError> {
    let service_id = query
        .service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_target("缺少 serviceId".to_string()))?;
    let service = server
        .services
        .resolve(service_id)
        .map_err(|error| invalid_target(error.to_string()))?;
    let fallback_source = query.log_source_id.clone();
    let spec = devtoolbox_core::server::LogReadRequest {
        service_id: service_id.to_string(),
        log_source_id: query.log_source_id,
        max_lines: query.max_lines.unwrap_or(0),
        max_bytes: query.max_bytes.unwrap_or(0),
        max_age_secs: query.max_age_secs.unwrap_or(0),
    };
    let (lines, bytes, age) = devtoolbox_core::server::logs::clamp_limits(&spec);
    let raw = server
        .logs
        .tail(
            &service,
            fallback_source.as_deref().unwrap_or_default(),
            lines,
            bytes,
            age,
        )
        .map_err(server_error)?;
    let redacted = devtoolbox_application::server::logs::LogRedactor::redact(raw);
    Ok(Json(json!({
        "service_id": redacted.service_id,
        "log_source_id": redacted.log_source_id,
        "text": redacted.text,
        "lines": redacted.lines,
        "redactions": redacted.redactions,
        "truncated": redacted.truncated,
        "untrusted": true,
    })))
}

pub async fn mcp_status(Extension(server): ServerRuntimeExtension) -> Json<Value> {
    let mcp = server.mcp_settings();
    Json(json!({
        "mcp": mcp,
        "identity_configured": false,
        "auth_status": "未配置身份提供者（远程 MCP 保持关闭）",
    }))
}

/// 请求重启：只返回确认票据，绝不执行。
pub async fn request_restart(
    remote: Option<Extension<AuthenticatedRemote>>,
    Extension(server): ServerRuntimeExtension,
    Json(body): Json<RestartRequest>,
) -> Result<Json<Value>, ApiError> {
    let remote = require_admin(remote)?;
    let confirmation = server
        .request_restart_for(&body.service_id, &remote.session_id, remote.trust())
        .map_err(action_error)?;
    Ok(Json(json!({
        "confirmation_id": confirmation.id,
        "action_type": confirmation.action_type,
        "target_id": confirmation.target_id,
        "summary": confirmation.human_readable_summary,
        "risk": confirmation.risk.as_str(),
        "created_at": confirmation.created_at,
        "expires_at": confirmation.expires_at,
    })))
}

/// 用户确认后执行；票据必须由同一 principal 签发。
pub async fn confirm_action(
    remote: Option<Extension<AuthenticatedRemote>>,
    Extension(server): ServerRuntimeExtension,
    Path(confirmation_id): Path<String>,
    Json(body): Json<ConfirmRequest>,
) -> Result<Json<Value>, ApiError> {
    let remote = require_admin(remote)?;
    let outcome = server
        .confirm_restart_for(&confirmation_id, &body.service_id, &remote.session_id)
        .map_err(action_error)?;
    Ok(Json(json!({
        "outcome": outcome.as_str(),
        "confirmed": true,
    })))
}

/// 取消待确认票据；跨 principal 取消同样 Denied。
pub async fn cancel_action(
    remote: Option<Extension<AuthenticatedRemote>>,
    Extension(server): ServerRuntimeExtension,
    Path(confirmation_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let remote = require_admin(remote)?;
    let outcome = server
        .cancel_for(&confirmation_id, &remote.session_id)
        .map_err(action_error)?;
    Ok(Json(json!({ "outcome": outcome.as_str() })))
}

fn require_admin(
    remote: Option<Extension<AuthenticatedRemote>>,
) -> Result<AuthenticatedRemote, ApiError> {
    let Some(Extension(remote)) = remote else {
        return Err(ApiError {
            code: "gateway_auth_required",
            message: "该操作需要经局域网密码网关登录后执行".to_string(),
        });
    };
    if !remote.is_admin() {
        return Err(ApiError {
            code: "gateway_admin_required",
            message: "当前网关身份没有系统操作权限".to_string(),
        });
    }
    Ok(remote)
}

/// 用例错误 → HTTP 契约（稳定 code，不回显请求体）。
fn action_error(message: String) -> ApiError {
    if message.contains("unknown_service") || message.contains("invalid_service_id") {
        return ApiError {
            code: "server_invalid_target",
            message,
        };
    }
    if message.contains("untrusted_session") {
        return ApiError {
            code: "server_action_denied",
            message,
        };
    }
    if message.starts_with("cooldown") || message.starts_with("session_limit") {
        return ApiError {
            code: "server_action_rate_limited",
            message,
        };
    }
    ApiError {
        code: "server_error",
        message,
    }
}

fn server_error(message: String) -> ApiError {
    ApiError {
        code: "server_error",
        message,
    }
}

fn invalid_target(message: String) -> ApiError {
    ApiError {
        code: "server_invalid_target",
        message,
    }
}

fn internal(message: String) -> ApiError {
    ApiError {
        code: "server_error",
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use devtoolbox_application::server::action::ServiceControlPort;
    use devtoolbox_core::server::ActionOutcome;
    use devtoolbox_core::settings::AppSettings;
    use devtoolbox_runtime::history_enrichment::SettingsLoader;
    use tower::ServiceExt;

    #[derive(Debug, Default)]
    struct CountingControl {
        restarts: std::sync::atomic::AtomicUsize,
    }

    impl ServiceControlPort for CountingControl {
        fn restart(&self, _service_id: &str) -> Result<(), String> {
            self.restarts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    fn loader() -> SettingsLoader {
        Arc::new(|| Ok(AppSettings::default()))
    }

    fn test_server() -> (tempfile::TempDir, Arc<ServerRuntime>) {
        let directory = tempfile::tempdir().expect("tempdir");
        let services = vec![devtoolbox_infrastructure::server::test_support::service(
            "self-tools",
        )];
        let runtime = ServerRuntime::assemble(
            directory.path(),
            loader(),
            devtoolbox_runtime::server::web_trust(),
            services,
            Vec::new(),
            Arc::new(CountingControl::default()),
        )
        .expect("assemble");
        (directory, Arc::new(runtime))
    }

    fn restart_router(server: Arc<ServerRuntime>) -> axum::Router {
        axum::Router::new()
            .route(
                "/api/v1/server/actions/restart",
                axum::routing::post(request_restart),
            )
            .layer(Extension(server))
    }

    fn remote(principal: &str) -> AuthenticatedRemote {
        AuthenticatedRemote::new(principal, crate::gateway_auth::GatewayRole::Admin)
    }

    #[tokio::test]
    async fn direct_loopback_write_without_gateway_marker_is_rejected() {
        let (_directory, server) = test_server();
        let app = restart_router(server);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/server/actions/restart")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"serviceId":"self-tools"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn gateway_admin_gets_a_ticket_and_only_the_same_principal_can_confirm() {
        let (_directory, server) = test_server();

        let ticket = server
            .request_restart_for(
                "self-tools",
                &remote("lan-owner").session_id,
                remote("lan-owner").trust(),
            )
            .expect("ticket");

        // 另一个 principal 的 confirm 必须 Denied（票据绑 session）。
        let other = server
            .confirm_restart_for(&ticket.id, "self-tools", &remote("intruder").session_id)
            .expect("outcome");
        assert_eq!(other, ActionOutcome::Denied);

        let outcome = server
            .confirm_restart_for(&ticket.id, "self-tools", &remote("lan-owner").session_id)
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Success);

        // 同一张票据不能再次确认（一次性）。
        let replay = server
            .confirm_restart_for(&ticket.id, "self-tools", &remote("lan-owner").session_id)
            .expect("outcome");
        assert_eq!(replay, ActionOutcome::Denied);
    }

    #[tokio::test]
    async fn handlers_require_admin_and_issue_only_tickets() {
        let (_directory, server) = test_server();

        let missing = request_restart(
            None,
            Extension(Arc::clone(&server)),
            Json(RestartRequest {
                service_id: "self-tools".into(),
            }),
        )
        .await
        .expect_err("marker required");
        assert_eq!(missing.code, "gateway_auth_required");

        let viewer = request_restart(
            Some(Extension(AuthenticatedRemote::new(
                "pad-reader",
                crate::gateway_auth::GatewayRole::Viewer,
            ))),
            Extension(Arc::clone(&server)),
            Json(RestartRequest {
                service_id: "self-tools".into(),
            }),
        )
        .await
        .expect_err("admin required");
        assert_eq!(viewer.code, "gateway_admin_required");

        let Json(ticket) = request_restart(
            Some(Extension(remote("lan-owner"))),
            Extension(Arc::clone(&server)),
            Json(RestartRequest {
                service_id: "self-tools".into(),
            }),
        )
        .await
        .expect("ticket");
        assert_eq!(ticket["target_id"], "self-tools");
        assert!(ticket["confirmation_id"].is_string());
        assert!(ticket["expires_at"].is_number());
    }

    #[test]
    fn action_errors_map_to_stable_http_codes() {
        assert_eq!(
            action_error("untrusted_session: 当前会话无法验证身份".into()).code,
            "server_action_denied"
        );
        assert_eq!(
            action_error("cooldown: 稍后再试".into()).code,
            "server_action_rate_limited"
        );
        assert_eq!(
            action_error("server error (unknown_service): 服务未注册".into()).code,
            "server_invalid_target"
        );
        assert_eq!(
            action_error("service_control_disabled".into()).code,
            "server_error"
        );
    }
}
