//! 测试共享装配（V8 §106：CI 不依赖真实 OAuth / model key / macOS）。
//!
//! 用内存 Fake 工具 + 静态 token provider 构造完整 `McpService`。

use std::sync::Arc;

use devtoolbox_application::mcp::adapter::McpToolAdapter;
use devtoolbox_application::mcp::auth::StaticTokenIdentityProvider;
use devtoolbox_application::mcp::service::{McpAuditPort, McpService, McpServiceConfig};
use devtoolbox_application::personal_ai::registry::{ToolExecutor, ToolRegistry};
use devtoolbox_core::mcp::McpAuditEntry;
use devtoolbox_core::personal_ai::{ToolResult, ToolRisk, ToolSpec};
use std::sync::Mutex;

/// 内存审计（断言用；同时用于「审计不含 token」测试）。
#[derive(Default)]
pub struct MemoryAudit {
    pub entries: Mutex<Vec<McpAuditEntry>>,
}

impl McpAuditPort for MemoryAudit {
    fn record(&self, entry: &McpAuditEntry) {
        self.entries.lock().unwrap().push(entry.clone());
    }
}

/// echo 工具（把参数回显，便于断言调用链）。
pub struct EchoTool;

#[async_trait::async_trait]
impl ToolExecutor for EchoTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(|| ToolSpec {
            name: "memory.search".into(),
            description: "检索个人记忆（测试替身）".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {"query": {"type": "string"}}
            }),
            risk: ToolRisk::Read,
            module: "memory".into(),
        });
        &SPEC
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, devtoolbox_core::AgentError> {
        Ok(ToolResult::ok(serde_json::json!({"echo": arguments})))
    }
}

/// 完整 harness：adapter + identity（静态 token）+ audit + service。
pub struct TestHarness {
    adapter: Arc<McpToolAdapter>,
    identity: Arc<StaticTokenIdentityProvider>,
    audit: Arc<MemoryAudit>,
}

impl TestHarness {
    #[must_use]
    pub fn new() -> Self {
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(EchoTool))
            .expect("register echo tool");
        Self::with_registry(Arc::new(registry))
    }

    #[must_use]
    pub fn with_registry(registry: Arc<ToolRegistry>) -> Self {
        let identity = Arc::new(StaticTokenIdentityProvider::new());
        identity.insert("tok-read", "p-1", "pi", vec!["selftools.read"], None, None);
        identity.insert(
            "tok-server",
            "p-2",
            "phone",
            vec!["server.read"],
            None,
            None,
        );
        identity.insert(
            "tok-action",
            "p-3",
            "cli",
            vec!["server.action"],
            None,
            None,
        );
        Self {
            adapter: Arc::new(McpToolAdapter::new(registry)),
            identity,
            audit: Arc::new(MemoryAudit::default()),
        }
    }

    #[must_use]
    pub fn service(&self) -> Arc<McpService> {
        let audit: Arc<dyn McpAuditPort> = Arc::clone(&self.audit) as Arc<dyn McpAuditPort>;
        Arc::new(self.service_with_audit(audit))
    }

    /// 用显式 audit（trait object）构造 service。
    fn service_with_audit(&self, audit: Arc<dyn McpAuditPort>) -> McpService {
        McpService::new(
            Arc::clone(&self.adapter),
            self.identity.clone(),
            audit,
            McpServiceConfig::default(),
        )
    }

    /// 带 SYSTEM 路由的 service（用于确认流测试）。
    #[must_use]
    pub fn service_with_actions(
        &self,
        actions: Arc<devtoolbox_application::server::action::SafeActionService>,
    ) -> Arc<McpService> {
        Arc::new(
            self.service_with_audit(Arc::clone(&self.audit) as Arc<dyn McpAuditPort>)
                .with_system_actions(actions),
        )
    }

    #[must_use]
    pub fn audit(&self) -> Arc<MemoryAudit> {
        Arc::clone(&self.audit)
    }

    #[must_use]
    pub fn identity(&self) -> Arc<StaticTokenIdentityProvider> {
        Arc::clone(&self.identity)
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}
