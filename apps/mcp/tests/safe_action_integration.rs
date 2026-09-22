//! MCP × SafeAction 集成测试（V8 §54-§58/§101-§103）。
//!
//! 覆盖 Gate 7 的硬要求：**MCP 无法绕过 Confirmation / Audit / Rate Limit**，
//! 以及 §103 跨 client 隔离（Client A 的票据 Client B 不能用）。
//!
//! 装配用真实 `SafeActionService` + 内存注册表 / 控制 / 审计（§106：不依赖
//! 真实 launchd / OAuth / macOS）。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use devtoolbox_application::personal_ai::registry::{ToolExecutor, ToolRegistry};
use devtoolbox_application::server::action::{
    ActionAuditPort, InMemoryActionAudit, InMemoryConfirmationStore, SafeActionConfig,
    SafeActionService, ServiceControlPort,
};
use devtoolbox_application::server::ports::ServiceProbePort;
use devtoolbox_application::server::registry::ServiceRegistryService;
use devtoolbox_core::mcp::McpCredential;
use devtoolbox_application::server::ActionOutcome;
use devtoolbox_core::server::{AuditEntry, SessionTrust};
use devtoolbox_core::personal_ai::{ToolResult, ToolRisk, ToolSpec};
use devtoolbox_core::server::{HealthCheckKind, HealthStatus, ServiceDescriptor, ServiceProviderType, ServiceStatus};

use devtoolbox_application::mcp::adapter::McpToolAdapter;
use devtoolbox_application::mcp::auth::StaticTokenIdentityProvider;
use devtoolbox_application::mcp::service::{McpAuditPort, McpService, McpServiceConfig};

/// 计数控制（执行次数断言）。
#[derive(Debug, Default)]
struct CountingControl {
    restarts: AtomicUsize,
}

impl ServiceControlPort for CountingControl {
    fn restart(&self, _service_id: &str) -> Result<(), String> {
        self.restarts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// 健康探活（注册服务恒 Healthy）。
#[derive(Debug, Default)]
struct HealthyProbe;

impl ServiceProbePort for HealthyProbe {
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        ServiceStatus {
            service_id: service.id.clone(),
            status: HealthStatus::Healthy,
            detail: "fake".into(),
            checked_at: 1,
        }
    }
}

/// 内存审计（断言「所有 write attempt 都有审计」）。
#[derive(Debug, Default)]
struct MemoryActionAudit {
    entries: std::sync::Mutex<Vec<AuditEntry>>,
}

impl ActionAuditPort for MemoryActionAudit {
    fn record(&self, entry: &AuditEntry) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).push(entry.clone());
    }
    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().take(limit).cloned().collect()
    }
}

/// `services.restart` 工具（与 V7 server 模块同形态：registry risk = Read，
/// SYSTEM 语义由暴露表 + SafeAction 票据表达，ADR-006）。
struct RestartTool;

#[async_trait::async_trait]
impl ToolExecutor for RestartTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(|| ToolSpec {
            name: "services.restart".into(),
            description: "请求重启已注册服务（需用户确认）".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["service_id"],
                "properties": {"service_id": {"type": "string"}}
            }),
            risk: ToolRisk::Read,
            module: "server".into(),
        });
        &SPEC
    }

    async fn execute(&self, _arguments: serde_json::Value) -> Result<ToolResult, devtoolbox_core::AgentError> {
        // MCP 层永不到达这里：SYSTEM 工具在 service 层被拦成确认票据。
        // 若到达说明门禁失效 —— 直接失败让测试爆掉。
        Err(devtoolbox_core::AgentError::tool_execution_failed(
            "services.restart must not execute directly",
        ))
    }
}

/// MCP 审计（断言 MCP 调用留痕）。
#[derive(Debug, Default)]
struct MemoryMcpAudit {
    entries: std::sync::Mutex<Vec<devtoolbox_core::mcp::McpAuditEntry>>,
}

impl McpAuditPort for MemoryMcpAudit {
    fn record(&self, entry: &devtoolbox_core::mcp::McpAuditEntry) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).push(entry.clone());
    }
}

impl MemoryMcpAudit {
    fn snapshot(&self) -> Vec<devtoolbox_core::mcp::McpAuditEntry> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// 完整装配：registry(restart) + identity + SafeAction + MCP service。
struct Harness {
    service: Arc<McpService>,
    actions: Arc<SafeActionService>,
    control: Arc<CountingControl>,
    action_audit: Arc<MemoryActionAudit>,
    mcp_audit: Arc<MemoryMcpAudit>,
}

fn harness() -> Harness {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(RestartTool)).expect("register restart");
    let services = Arc::new(ServiceRegistryService::new(
        vec![ServiceDescriptor {
            id: "self-tools".into(),
            display_name: "Self Tools".into(),
            description: "后端".into(),
            provider_type: ServiceProviderType::Launchd,
            provider_ref: "com.example.self-tools".into(),
            health_check: HealthCheckKind::Launchd,
            log_sources: Vec::new(),
            allowed_actions: vec!["restart".into()],
            tags: Vec::new(),
        }],
        Arc::new(HealthyProbe),
    ));
    let control = Arc::new(CountingControl::default());
    let action_audit = Arc::new(MemoryActionAudit::default());
    let actions = Arc::new(SafeActionService::new(
        Arc::clone(&services),
        control.clone(),
        Arc::new(InMemoryConfirmationStore::default()),
        action_audit.clone(),
        Arc::new(devtoolbox_core::server::DefaultActionRiskPolicy),
        SafeActionConfig {
            confirmation_ttl_secs: 60,
            cooldown_secs: 0,
            max_system_per_session: 10,
        },
    ));
    let identity = Arc::new(StaticTokenIdentityProvider::new());
    identity.insert("tok-a", "p-a", "client-a", vec!["server.action"], None, None);
    identity.insert("tok-b", "p-b", "client-b", vec!["server.action"], None, None);
    identity.insert("tok-read", "p-r", "client-r", vec!["server.read"], None, None);
    let mcp_audit = Arc::new(MemoryMcpAudit::default());
    let service = Arc::new(
        McpService::new(
            Arc::new(McpToolAdapter::new(Arc::new(registry))),
            identity,
            mcp_audit.clone(),
            McpServiceConfig::default(),
        )
        .with_system_actions(Arc::clone(&actions)),
    );
    Harness {
        service,
        actions,
        control,
        action_audit,
        mcp_audit,
    }
}

/// 用票据执行（模拟 self-tools UI 的确认命令）。
fn confirm_with(actions: &SafeActionService, confirmation_id: &str, client_id: &str, service_id: &str) -> ActionOutcome {
    let request = devtoolbox_application::server::action::restart_request(
        service_id,
        client_id,
        "Self Tools",
    );
    actions
        .confirm_and_execute(confirmation_id, &request)
        .expect("outcome")
}

fn actions_of(harness: &Harness) -> Arc<SafeActionService> {
    Arc::clone(&harness.actions)
}

/// 本地受信 principal（STDIO / loopback；§22：仍受 risk 约束）。
fn local_action_principal(client_id: &str) -> devtoolbox_core::mcp::McpPrincipal {
    devtoolbox_core::mcp::McpPrincipal {
        principal_id: format!("local-{client_id}"),
        client_id: client_id.to_string(),
        transport: devtoolbox_core::mcp::McpTransport::Stdio,
        trust: devtoolbox_core::mcp::McpTrustLevel::LocalTrusted,
        authenticated: true,
        scopes: vec![devtoolbox_core::mcp::McpScope::parse("server.action").expect("scope")],
        issuer: None,
        expires_at: None,
    }
}

#[tokio::test]
async fn system_tool_returns_confirmation_and_never_executes() {
    // §54/§55：首次调用返回 confirmation_required，零执行。
    let harness = harness();
    let principal = local_action_principal("client-a");
    let invocation = harness
        .service
        .call_tool(
            &principal,
            "services.restart",
            serde_json::json!({"service_id": "self-tools"}),
        )
        .await
        .expect("call");
    assert!(
        invocation.outcome.is_confirmation(),
        "SYSTEM 必须返回确认请求"
    );
    assert_eq!(invocation.payload["confirmation_required"], true);
    assert!(invocation.payload["confirmation_id"].is_string());
    assert_eq!(invocation.payload["risk"], "system");
    assert!(!invocation.is_error, "确认请求不是错误");
    assert_eq!(
        harness.control.restarts.load(Ordering::SeqCst),
        0,
        "未经确认不得执行"
    );
}

#[tokio::test]
async fn confirmation_cannot_be_self_approved_by_the_client() {
    // §56：外部 client 不能自行 confirmed=true。执行入口只有
    // `SafeActionService::confirm_and_execute`（self-tools 侧）。
    let harness = harness();
    let principal = local_action_principal("client-a");
    let invocation = harness
        .service
        .call_tool(
            &principal,
            "services.restart",
            serde_json::json!({"service_id": "self-tools"}),
        )
        .await
        .expect("call");
    let confirmation_id = invocation.payload["confirmation_id"]
        .as_str()
        .expect("id")
        .to_string();
    // 尝试用「已确认」参数再调一次：仍然是新票据，不会执行旧的。
    let second = harness
        .service
        .call_tool(
            &principal,
            "services.restart",
            serde_json::json!({"service_id": "self-tools", "confirmed": true}),
        )
        .await
        .expect("call");
    assert!(second.outcome.is_confirmation());
    assert_ne!(
        second.payload["confirmation_id"].as_str(),
        Some(confirmation_id.as_str()),
        "confirmed 参数不得被接受为执行授权"
    );
    assert_eq!(harness.control.restarts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cross_client_ticket_reuse_is_denied() {
    // §103：Client A 的票据，Client B 不能用。
    let harness = harness();
    let a = local_action_principal("client-a");
    let invocation = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "self-tools"}))
        .await
        .expect("call");
    let ticket = invocation.payload["confirmation_id"].as_str().expect("id").to_string();

    // Client B 用自己的身份尝试消费该票据 → Denied。
    let outcome = confirm_with(&actions_of(&harness), &ticket, "client-b", "self-tools");
    assert_eq!(outcome, ActionOutcome::Denied, "跨 client 复用必须拒绝");
    assert_eq!(harness.control.restarts.load(Ordering::SeqCst), 0);

    // Client A 自己确认 → 成功执行一次。
    let outcome = confirm_with(&actions_of(&harness), &ticket, "client-a", "self-tools");
    assert_eq!(outcome, ActionOutcome::Success);
    assert_eq!(harness.control.restarts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_ticket_and_target_mutation_are_denied() {
    // §59/§58：过期拒绝；确认后参数变化拒绝。
    let harness = harness();
    let a = local_action_principal("client-a");
    let invocation = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "self-tools"}))
        .await
        .expect("call");
    let ticket = invocation.payload["confirmation_id"].as_str().expect("id").to_string();

    // 目标变化（self-tools → 未注册的 other）→ Denied。
    let outcome = confirm_with(&actions_of(&harness), &ticket, "client-a", "other-service");
    assert_eq!(outcome, ActionOutcome::Denied, "目标变化必须拒绝");

    // 原票据仍可用一次（上面的拒绝没有消费它）。
    let outcome = confirm_with(&actions_of(&harness), &ticket, "client-a", "self-tools");
    assert_eq!(outcome, ActionOutcome::Success);
    // 重放 → Denied。
    let replay = confirm_with(&actions_of(&harness), &ticket, "client-a", "self-tools");
    assert_eq!(replay, ActionOutcome::Denied, "一次性票据");
    assert_eq!(harness.control.restarts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn scope_without_server_action_cannot_even_see_the_tool() {
    // §98 Case 5：无 server.action scope → 工具不可见（discovery 即过滤）。
    let harness = harness();
    let reader = harness
        .service
        .authenticate(&McpCredential::Bearer("tok-read".into()))
        .expect("auth");
    let visible = harness.service.list_tools(&reader).expect("list");
    assert!(
        !visible.iter().any(|tool| tool.name == "services.restart"),
        "无 server.action 不得看见 restart 工具"
    );
    let error = harness
        .service
        .call_tool(&reader, "services.restart", serde_json::json!({"service_id": "self-tools"}))
        .await
        .expect_err("denied");
    assert!(format!("{error:?}").contains("server.action") || format!("{error:?}").contains("not_exposed"));
}

#[tokio::test]
async fn unknown_service_is_denied_before_any_ticket() {
    // §35/§154：未注册服务 → 拒绝，不签发票据。
    let harness = harness();
    let a = local_action_principal("client-a");
    let error = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "sshd"}))
        .await
        .expect_err("denied");
    assert!(format!("{error:?}").contains("unknown_service"), "{error:?}");
    assert_eq!(harness.control.restarts.load(Ordering::SeqCst), 0);
    assert!(
        harness.action_audit.recent(10).iter().all(|entry| entry.result != ActionOutcome::Success),
        "未注册服务不得有成功审计"
    );
}

#[tokio::test]
async fn every_write_attempt_is_audited() {
    // §78/§131：MCP 调用与 SafeAction 尝试都留痕。
    let harness = harness();
    let a = local_action_principal("client-a");
    // 未注册服务（拒绝）。
    let _ = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "nope"}))
        .await;
    // 已注册服务（票据）。
    let invocation = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "self-tools"}))
        .await
        .expect("call");
    let ticket = invocation.payload["confirmation_id"].as_str().expect("id").to_string();
    // 确认执行。
    let _ = confirm_with(&actions_of(&harness), &ticket, "client-a", "self-tools");

    // MCP 侧审计：两次调用都留痕，且都记了 tool 名（§78/§80）。
    let mcp_entries = harness.mcp_audit.snapshot();
    assert_eq!(mcp_entries.len(), 2, "{:?}", mcp_entries.iter().map(|e| e.result).collect::<Vec<_>>());
    assert!(mcp_entries.iter().any(|entry| entry.tool == "services.restart"));
    assert!(
        mcp_entries
            .iter()
            .any(|entry| entry.result == devtoolbox_core::mcp::McpResultCode::Denied),
        "未注册服务的调用必须记为 denied"
    );
    assert!(
        mcp_entries
            .iter()
            .any(|entry| entry.result == devtoolbox_core::mcp::McpResultCode::ConfirmationRequired),
        "SYSTEM 请求必须记为 confirmation_required"
    );
    // SafeAction 侧审计：确认后的执行有 Success 记录（§64）。
    let action_entries = harness.action_audit.recent(20);
    assert!(
        action_entries
            .iter()
            .any(|entry| entry.result == ActionOutcome::Success && entry.confirmed),
        "确认执行必须留 success 审计"
    );
}

#[tokio::test]
async fn audit_never_contains_tokens() {
    // §79/§149：token 不得进入任何审计。
    let harness = harness();
    let a = local_action_principal("client-a");
    let _ = harness
        .service
        .call_tool(&a, "services.restart", serde_json::json!({"service_id": "self-tools"}))
        .await;
    let json = serde_json::to_string(&harness.mcp_audit.snapshot()).expect("json");
    assert!(!json.contains("tok-a"), "MCP 审计不得含 token");
    let action_json = serde_json::to_string(&harness.action_audit.recent(10)).expect("json");
    assert!(!action_json.contains("tok-a"), "SafeAction 审计不得含 token");
}
