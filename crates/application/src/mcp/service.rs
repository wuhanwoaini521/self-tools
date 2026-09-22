//! MCP 服务编排（V8 §51）。
//!
//! 调用链（§51）：
//!
//! ```text
//! MCP request → Authenticate → Principal → Exposure/Authorization
//!             → Schema Validation → ToolRegistry → Risk Enforcement
//!             → Tool → Structured Result → MCP response
//! ```
//!
//! SYSTEM 工具在此**不执行**（§54）：返回 `ConfirmationRequired`，
//! 执行只能由 self-tools UI 的确认命令消费票据（§56/§57）。

use std::sync::Arc;
use std::time::Instant;

use devtoolbox_core::mcp::{
    McpAuditEntry, McpCredential, McpDecision, McpPrincipal, McpResultCode,
};
use devtoolbox_core::personal_ai::ToolRisk;

use super::adapter::{McpCallOutcome, McpToolAdapter, McpToolDefinition};
use super::auth::RemoteIdentityProvider;
use super::policy::McpAuthorizationPolicy;

/// MCP 调用结果（面向 transport 层的统一形状）。
#[derive(Clone, Debug)]
pub struct McpInvocation {
    pub outcome: McpCallOutcome,
    /// 结构化结果 JSON（`tools/call` 的 content）。
    pub payload: serde_json::Value,
    /// 协议层 `isError`。
    pub is_error: bool,
}

/// 编排配置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct McpServiceConfig {
    /// 单次请求超时（毫秒；§85）。
    pub request_timeout_ms: u64,
    /// tool 参数大小上限（字节；§83）。
    pub max_argument_bytes: usize,
    /// 响应大小上限（字符；§84）。
    pub max_response_chars: usize,
}

impl Default for McpServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout_ms: 20_000,
            max_argument_bytes: 256 * 1_024,
            max_response_chars: 64 * 1_024,
        }
    }
}

/// MCP 服务（transport-neutral：STDIO 与 HTTP 复用同一实例）。
pub struct McpService {
    adapter: Arc<McpToolAdapter>,
    identity: Arc<dyn RemoteIdentityProvider>,
    audit: Arc<dyn McpAuditPort>,
    config: McpServiceConfig,
    /// SYSTEM 操作路由（§54）：复用 V7 SafeActionService。
    system_actions: Option<Arc<crate::server::action::SafeActionService>>,
    sequence: parking_lot::Mutex<u64>,
}

/// 审计端口（§78）。
pub trait McpAuditPort: Send + Sync {
    fn record(&self, entry: &McpAuditEntry);
}

impl McpService {
    #[must_use]
    pub fn new(
        adapter: Arc<McpToolAdapter>,
        identity: Arc<dyn RemoteIdentityProvider>,
        audit: Arc<dyn McpAuditPort>,
        config: McpServiceConfig,
    ) -> Self {
        Self {
            adapter,
            identity,
            audit,
            config,
            system_actions: None,
            sequence: parking_lot::Mutex::new(0),
        }
    }

    /// 接入 SafeActionService（§54：SYSTEM 走 V7 通道）。
    #[must_use]
    pub fn with_system_actions(
        mut self,
        actions: Arc<crate::server::action::SafeActionService>,
    ) -> Self {
        // §93：MCP 触发的系统操作在审计里标记来源，便于 UI 过滤。
        actions.set_audit_source(devtoolbox_core::server::AuditSource::Mcp);
        self.system_actions = Some(actions);
        self
    }

    #[must_use]
    pub fn adapter(&self) -> &Arc<McpToolAdapter> {
        &self.adapter
    }

    /// 请求 id（§80：串联 auth / tool / audit）。
    fn next_request_id(&self) -> String {
        let mut sequence = self.sequence.lock();
        *sequence += 1;
        format!("mcp-req-{:06}", *sequence)
    }

    /// 认证（§26/§42）：失败 → DENY，不进入 discovery/execution。
    pub fn authenticate(&self, credential: &McpCredential) -> Result<McpPrincipal, McpAuthError> {
        self.identity
            .authenticate(credential)
            .map_err(McpAuthError::from_failure)
    }

    /// `tools/list`（§18：授权后的 catalog）。
    pub fn list_tools(
        &self,
        principal: &McpPrincipal,
    ) -> Result<Vec<McpToolDefinition>, McpAuthError> {
        let started = Instant::now();
        let request_id = self.next_request_id();
        if !McpAuthorizationPolicy::authenticate(principal, now_unix()).is_allowed() {
            self.audit.record(&McpAuditEntry::discovery(
                &request_id,
                principal,
                now_unix(),
                McpDecision::Denied,
                started.elapsed().as_millis() as u64,
            ));
            return Err(McpAuthError::Unauthenticated);
        }
        let definitions: Vec<_> = self
            .adapter
            .all_definitions()
            .into_iter()
            .filter(|definition| {
                McpAuthorizationPolicy::authorize_tool(&self.adapter, principal, &definition.name)
                    .is_allowed()
            })
            .collect();
        self.audit.record(&McpAuditEntry::discovery(
            &request_id,
            principal,
            now_unix(),
            McpDecision::Allowed,
            started.elapsed().as_millis() as u64,
        ));
        Ok(definitions)
    }

    /// `tools/call`（§51-§55）。
    pub async fn call_tool(
        &self,
        principal: &McpPrincipal,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpInvocation, McpCallError> {
        let started = Instant::now();
        let request_id = self.next_request_id();
        let now = now_unix();

        // 1) 认证门禁。
        if !McpAuthorizationPolicy::authenticate(principal, now).is_allowed() {
            self.record(principal, &request_id, tool_name, None, McpDecision::Denied, McpResultCode::AuthFailed, started);
            return Err(McpCallError::Unauthenticated);
        }
        // 2) payload 限制（§83）。
        let argument_size = serde_json::to_vec(&arguments).map(|bytes| bytes.len()).unwrap_or(usize::MAX);
        if argument_size > self.config.max_argument_bytes {
            self.record(principal, &request_id, tool_name, None, McpDecision::Denied, McpResultCode::InvalidParams, started);
            return Err(McpCallError::PayloadTooLarge);
        }
        // 3) 暴露 + scope 授权（§18：execution 也授权）。
        let decision = McpAuthorizationPolicy::authorize_tool(&self.adapter, principal, tool_name);
        if !decision.is_allowed() {
            self.record(principal, &request_id, tool_name, None, McpDecision::Denied, McpResultCode::Denied, started);
            return Err(McpCallError::Denied(decision_reason(&decision)));
        }
        let risk = self.adapter.registry().spec(tool_name).map(|spec| spec.risk);
        // 4) SYSTEM → SafeAction 票据（§54/§55：不执行）。
        //    判定依据是**暴露分组**而不是 registry risk：V7 的 `services.restart`
        //    在 ToolRegistry 注册为 Read（registry 门禁只放行 Read+SafeWrite），
        //    SYSTEM 语义由 `ExposureGroup::SystemAction` 表达（ADR-006）。
        let is_system = devtoolbox_core::mcp::default_exposure(tool_name)
            .is_some_and(|exposure| {
                matches!(
                    exposure.group,
                    devtoolbox_core::mcp::ExposureGroup::SystemAction
                )
            })
            || matches!(risk, Some(ToolRisk::System));
        if is_system {
            return self.handle_system_action(principal, &request_id, tool_name, arguments, started);
        }
        // 5) 走 ToolRegistry（与 PersonalAgent 同一执行路径，§110）。
        //    §85：请求层超时（tool 自身超时由 tool 实现负责）。
        let timeout = std::time::Duration::from_millis(self.config.request_timeout_ms.max(1));
        let executed = match tokio::time::timeout(timeout, self.adapter.execute(tool_name, arguments)).await {
            Ok(result) => result,
            Err(_) => {
                self.record(principal, &request_id, tool_name, risk, McpDecision::Allowed, McpResultCode::ToolFailed, started);
                return Err(McpCallError::ToolFailed(format!(
                    "tool timed out after {} ms",
                    self.config.request_timeout_ms
                )));
            }
        };
        match executed {
            Ok(result) => {
                let payload = truncate_json(result.data.clone(), self.config.max_response_chars);
                self.record(principal, &request_id, tool_name, risk, McpDecision::Allowed, McpResultCode::Ok, started);
                Ok(McpInvocation {
                    outcome: McpCallOutcome::Executed(result),
                    payload,
                    is_error: false,
                })
            }
            Err(error) => {
                self.record(principal, &request_id, tool_name, risk, McpDecision::Allowed, McpResultCode::ToolFailed, started);
                Err(McpCallError::ToolFailed(error.to_string()))
            }
        }
    }

    /// SYSTEM：签发票据，不执行（§54/§55）。
    fn handle_system_action(
        &self,
        principal: &McpPrincipal,
        request_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        started: Instant,
    ) -> Result<McpInvocation, McpCallError> {
        let Some(actions) = &self.system_actions else {
            self.record(principal, request_id, tool_name, Some(ToolRisk::System), McpDecision::Denied, McpResultCode::Denied, started);
            return Err(McpCallError::Denied("system_actions_unavailable".into()));
        };
        // 只支持 services.restart（V8 与 V7 同一操作集）。
        let service_id = arguments
            .get("service_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let registry = actions.registry();
        let descriptor = match registry.resolve(service_id) {
            Ok(descriptor) => descriptor,
            Err(_) => {
                // §35/§154：未注册服务 = 授权拒绝（不是参数错误）——
                // 稳定 reason 让 client 能区分「没权限」与「调用形式错」。
                self.record(principal, request_id, tool_name, Some(ToolRisk::System), McpDecision::Denied, McpResultCode::Denied, started);
                return Err(McpCallError::Denied("unknown_service".into()));
            }
        };
        // §103：票据绑 client_id（跨 client 隔离）。
        let request = crate::server::action::restart_request(
            &descriptor.id,
            // §103：票据绑 client_id → 跨 client 不可复用。
            &principal.client_id,
            &descriptor.display_name,
        );
        match actions.plan(&request, devtoolbox_core::server::SessionTrust::RemoteAuthenticated) {
            crate::server::action::ActionPlan::ConfirmationRequired(confirmation) => {
                self.record(principal, request_id, tool_name, Some(ToolRisk::System), McpDecision::Allowed, McpResultCode::ConfirmationRequired, started);
                let payload = serde_json::json!({
                    "confirmation_required": true,
                    "confirmation_id": confirmation.id,
                    "summary": confirmation.human_readable_summary,
                    "target_id": confirmation.target_id,
                    "risk": confirmation.risk.as_str(),
                    "expires_at": confirmation.expires_at,
                    "note": "已请求用户在 self-tools 中确认；未经确认不会执行",
                });
                Ok(McpInvocation {
                    outcome: McpCallOutcome::ConfirmationRequired {
                        confirmation_id: confirmation.id,
                        summary: confirmation.human_readable_summary,
                        target_id: confirmation.target_id,
                        risk: confirmation.risk.as_str().to_string(),
                        expires_at: confirmation.expires_at,
                    },
                    payload,
                    is_error: false,
                })
            }
            crate::server::action::ActionPlan::Denied { reason, .. } => {
                self.record(principal, request_id, tool_name, Some(ToolRisk::System), McpDecision::Denied, McpResultCode::Denied, started);
                Err(McpCallError::Denied(reason))
            }
            crate::server::action::ActionPlan::Executed(_) => {
                // SYSTEM 不应有直接执行路径；出现即 fail-closed。
                self.record(principal, request_id, tool_name, Some(ToolRisk::System), McpDecision::Denied, McpResultCode::Denied, started);
                Err(McpCallError::Denied("unexpected_direct_execution".into()))
            }
        }
    }

    fn record(
        &self,
        principal: &McpPrincipal,
        request_id: &str,
        tool_name: &str,
        risk: Option<ToolRisk>,
        decision: McpDecision,
        result: McpResultCode,
        started: Instant,
    ) {
        self.audit.record(&McpAuditEntry {
            request_id: request_id.to_string(),
            timestamp: now_unix(),
            principal_id: principal.principal_id.clone(),
            client_id: principal.client_id.clone(),
            transport: principal.transport,
            tool: tool_name.to_string(),
            risk: risk.map(super::adapter::risk_str),
            decision,
            result,
            duration_ms: started.elapsed().as_millis() as u64,
        });
    }
}

/// 认证错误（传输层映射为 JSON-RPC error）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpAuthError {
    Unauthenticated,
    InvalidToken,
    ExpiredToken,
    InsufficientScope,
    ProviderUnavailable,
}

impl McpAuthError {
    fn from_failure(failure: devtoolbox_core::mcp::AuthFailure) -> Self {
        use devtoolbox_core::mcp::AuthFailure;
        match failure {
            AuthFailure::Unauthenticated => McpAuthError::Unauthenticated,
            AuthFailure::InvalidToken => McpAuthError::InvalidToken,
            AuthFailure::ExpiredToken => McpAuthError::ExpiredToken,
            AuthFailure::InsufficientScope => McpAuthError::InsufficientScope,
            AuthFailure::ProviderUnavailable => McpAuthError::ProviderUnavailable,
        }
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            McpAuthError::Unauthenticated => "unauthenticated",
            McpAuthError::InvalidToken => "invalid_token",
            McpAuthError::ExpiredToken => "expired_token",
            McpAuthError::InsufficientScope => "insufficient_scope",
            McpAuthError::ProviderUnavailable => "provider_unavailable",
        }
    }
}

/// 调用错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpCallError {
    Unauthenticated,
    Denied(String),
    InvalidParams(String),
    UnknownTool(String),
    PayloadTooLarge,
    ToolFailed(String),
}

impl std::fmt::Display for McpCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 只输出受控文案；Denied/ToolFailed 的文本由上游保证不含 secret。
        match self {
            McpCallError::Unauthenticated => formatter.write_str("unauthenticated"),
            McpCallError::Denied(reason) => write!(formatter, "denied: {reason}"),
            McpCallError::InvalidParams(detail) => write!(formatter, "invalid params: {detail}"),
            McpCallError::UnknownTool(tool) => write!(formatter, "unknown tool: {tool}"),
            McpCallError::PayloadTooLarge => formatter.write_str("payload too large"),
            McpCallError::ToolFailed(message) => formatter.write_str(message),
        }
    }
}

fn decision_reason(decision: &super::policy::AuthorizationDecision) -> String {
    match decision {
        super::policy::AuthorizationDecision::Allowed => "allowed".to_string(),
        super::policy::AuthorizationDecision::Denied { reason, .. } => reason.clone(),
    }
}

/// 截断 JSON 负载（§84：传输层再设一道闸；业务限制仍先生效）。
///
/// 先用 `serde_json::to_string` 的**紧凑**形式判定长度；超限时只保留前缀，
/// 不再二次序列化整个值（审查 MCP-E：避免峰值内存 = 完整序列化长度）。
fn truncate_json(value: serde_json::Value, max_chars: usize) -> serde_json::Value {
    let text = serde_json::to_string(&value).unwrap_or_default();
    let total = text.chars().count();
    if total <= max_chars {
        return value;
    }
    let preview: String = text.chars().take(max_chars).collect();
    // preview 与 original_chars 都来自同一次序列化结果。
    serde_json::json!({
        "truncated": true,
        "original_chars": total,
        "preview": preview,
    })
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::adapter::McpToolAdapter;
    use crate::mcp::auth::StaticTokenIdentityProvider;
    use devtoolbox_core::mcp::McpCredential;
    use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};
    use devtoolbox_core::personal_ai::ToolSpec;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryAudit {
        entries: Mutex<Vec<McpAuditEntry>>,
    }

    impl McpAuditPort for MemoryAudit {
        fn record(&self, entry: &McpAuditEntry) {
            self.entries.lock().unwrap().push(entry.clone());
        }
    }

    struct EchoTool;

    #[async_trait::async_trait]
    impl ToolExecutor for EchoTool {
        fn spec(&self) -> &ToolSpec {
            static SPEC: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(|| ToolSpec {
                name: "memory.search".into(),
                description: "检索".into(),
                input_schema: serde_json::json!({"type": "object"}),
                risk: ToolRisk::Read,
                module: "memory".into(),
            });
            &SPEC
        }
        async fn execute(&self, arguments: serde_json::Value) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
            Ok(devtoolbox_core::ToolResult::ok(arguments))
        }
    }

    fn service() -> (Arc<McpService>, Arc<MemoryAudit>, Arc<StaticTokenIdentityProvider>) {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).expect("register");
        let adapter = Arc::new(McpToolAdapter::new(Arc::new(registry)));
        let identity = Arc::new(StaticTokenIdentityProvider::new());
        identity.insert(
            "tok-read",
            "p-1",
            "pi",
            vec!["selftools.read"],
            None,
            None,
        );
        let audit = Arc::new(MemoryAudit::default());
        let service = Arc::new(McpService::new(
            adapter,
            identity.clone(),
            audit.clone(),
            McpServiceConfig::default(),
        ));
        (service, audit, identity)
    }

    fn principal_with(scopes: Vec<&'static str>) -> devtoolbox_core::mcp::McpPrincipal {
        devtoolbox_core::mcp::McpPrincipal {
            principal_id: "p".into(),
            client_id: "c".into(),
            transport: devtoolbox_core::mcp::McpTransport::Http,
            trust: devtoolbox_core::mcp::McpTrustLevel::RemoteAuthenticated,
            authenticated: true,
            scopes: scopes
                .into_iter()
                .filter_map(devtoolbox_core::mcp::McpScope::parse)
                .collect(),
            issuer: None,
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn authenticated_call_flows_through_registry() {
        let (service, audit, _) = service();
        let principal = service
            .authenticate(&McpCredential::Bearer("tok-read".into()))
            .expect("auth");
        let invocation = service
            .call_tool(&principal, "memory.search", serde_json::json!({"query": "docker"}))
            .await
            .expect("call");
        assert_eq!(invocation.payload["query"], "docker");
        assert!(!invocation.is_error);
        assert_eq!(
            audit.entries.lock().unwrap().last().expect("entry").result,
            McpResultCode::Ok
        );
    }

    #[tokio::test]
    async fn unauthenticated_call_is_denied_and_audited() {
        let (service, audit, _) = service();
        let anonymous = devtoolbox_core::mcp::McpPrincipal::anonymous_remote("curl");
        let error = service
            .call_tool(&anonymous, "memory.search", serde_json::json!({}))
            .await
            .expect_err("denied");
        assert_eq!(error, McpCallError::Unauthenticated);
        assert_eq!(
            audit.entries.lock().unwrap().last().expect("entry").result,
            McpResultCode::AuthFailed
        );
    }

    #[tokio::test]
    async fn discovery_hides_tools_without_scope() {
        let (service, audit, _) = service();
        let principal = service
            .authenticate(&McpCredential::Bearer("tok-read".into()))
            .expect("auth");
        let definitions = service.list_tools(&principal).expect("list");
        assert_eq!(definitions.len(), 1, "selftools.read 覆盖 memory.read");

        // 无 scope 的另一 principal：catalog 为空，且拒绝也被审计（§18/§19）。
        let other = principal_with(Vec::new());
        assert!(service.list_tools(&other).expect("empty").is_empty());
        assert_eq!(
            audit.entries.lock().unwrap().last().expect("entry").tool,
            "__discover__"
        );
    }

    #[tokio::test]
    async fn oversized_arguments_are_rejected() {
        let (service, _, _) = service();
        let principal = principal_with(vec!["selftools.read"]);
        let huge = "x".repeat(300 * 1_024);
        let error = service
            .call_tool(&principal, "memory.search", serde_json::json!({"query": huge}))
            .await
            .expect_err("too large");
        assert_eq!(error, McpCallError::PayloadTooLarge, "§83：参数上限");
    }

    #[tokio::test]
    async fn token_reaches_error_without_echoing_value() {
        let (service, _, _) = service();
        let error = service
            .authenticate(&McpCredential::Bearer("bogus-token".into()))
            .expect_err("invalid");
        assert_eq!(error, McpAuthError::InvalidToken);
        // 错误文本绝不含 token 值（§79）。
        assert!(!format!("{error:?}").contains("bogus-token"), "{error:?}");
    }

    #[tokio::test]
    async fn truncate_keeps_prefix_and_marks_total() {
        // §84：超限返回截断预览 + 原始长度，不返回完整 payload。
        let big = serde_json::json!({"blob": "x".repeat(500)});
        let truncated = truncate_json(big, 50);
        assert_eq!(truncated["truncated"], true);
        assert_eq!(truncated["original_chars"], 500 + 11);
        let preview = truncated["preview"].as_str().expect("preview");
        assert_eq!(preview.chars().count(), 50);

        let small = serde_json::json!({"ok": true});
        assert_eq!(truncate_json(small.clone(), 500), small, "未超限原样返回");
    }

    #[tokio::test]
    async fn tool_timeout_is_enforced() {
        // §85：慢工具在 request_timeout_ms 后以 ToolFailed 结束（不挂住）。
        struct SlowTool;

        #[async_trait::async_trait]
        impl crate::personal_ai::registry::ToolExecutor for SlowTool {
            fn spec(&self) -> &devtoolbox_core::personal_ai::ToolSpec {
                static SPEC: std::sync::LazyLock<devtoolbox_core::personal_ai::ToolSpec> =
                    std::sync::LazyLock::new(|| devtoolbox_core::personal_ai::ToolSpec {
                        name: "memory.search".into(),
                        description: "慢".into(),
                        input_schema: serde_json::json!({"type": "object"}),
                        risk: ToolRisk::Read,
                        module: "memory".into(),
                    });
                &SPEC
            }
            async fn execute(
                &self,
                _arguments: serde_json::Value,
            ) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok(devtoolbox_core::ToolResult::ok(serde_json::json!({})))
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SlowTool)).expect("register");
        let audit = Arc::new(MemoryAudit::default());
        let service = McpService::new(
            Arc::new(McpToolAdapter::new(Arc::new(registry))),
            Arc::new(crate::mcp::auth::DenyAllIdentityProvider),
            audit.clone(),
            McpServiceConfig {
                request_timeout_ms: 30,
                ..McpServiceConfig::default()
            },
        );
        let principal = principal_with(vec!["selftools.read"]);
        let error = service
            .call_tool(&principal, "memory.search", serde_json::json!({"query": "x"}))
            .await
            .expect_err("timeout");
        assert!(format!("{error:?}").contains("timed out"), "{error:?}");
    }

    #[tokio::test]
    async fn audit_never_contains_credentials() {
        let (service, audit, _) = service();
        let principal = principal_with(vec!["selftools.read"]);
        let _ = service
            .call_tool(&principal, "memory.search", serde_json::json!({"query": "x"}))
            .await;
        let json = serde_json::to_string(&*audit.entries.lock().unwrap()).expect("json");
        for forbidden in ["tok-read", "token", "secret", "password"] {
            assert!(!json.contains(forbidden), "审计不得含 {forbidden}");
        }
    }
}
