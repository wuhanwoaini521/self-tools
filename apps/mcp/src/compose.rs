//! MCP 组合根（V8：把 ToolRegistry + identity + audit + SafeAction 装配成
//! 可运行的 `McpService`）。
//!
//! 当前装配范围（Phase 1，V8 §152 的「Local MCP only」最小集）：
//! - ToolRegistry 从环境变量声明的域装配（默认空 = fail-closed）；
//! - identity = `DenyAllIdentityProvider`（远程一律拒绝），loopback 由
//!   transport 层给 LOCAL_TRUSTED；
//! - SafeAction = 空注册表（无已注册服务 → 任何 restart 请求被拒）。
//!
//! 完整装配（真实 memory/files/server stores）属于后续 Gate：在能证明
//! 安全之前，宁可是「能力少」而不是「能力错」（§152）。

use std::sync::Arc;

use devtoolbox_application::mcp::adapter::McpToolAdapter;
use devtoolbox_application::mcp::auth::{DenyAllIdentityProvider, RemoteIdentityProvider};
use devtoolbox_application::mcp::service::{McpAuditPort, McpService, McpServiceConfig};
use devtoolbox_application::personal_ai::registry::ToolRegistry;
use devtoolbox_application::server::action::{
    ActionAuditPort, InMemoryActionAudit, InMemoryConfirmationStore, SafeActionConfig,
    SafeActionService, ServiceControlPort,
};
use devtoolbox_application::server::registry::ServiceRegistryService;
use devtoolbox_application::server::ports::ServiceProbePort;
use devtoolbox_core::mcp::McpAuditEntry;
use devtoolbox_core::server::{HealthStatus, ServiceDescriptor, ServiceStatus, SessionTrust};
use std::sync::Mutex;

/// 内存审计（进程内；重启即丢——不引外部日志栈，V8 §111）。
#[derive(Default)]
pub struct MemoryAuditStore {
    entries: Mutex<Vec<McpAuditEntry>>,
}

impl McpAuditPort for MemoryAuditStore {
    fn record(&self, entry: &McpAuditEntry) {
        let mut entries = self.entries.lock().unwrap_or_else(|error| error.into_inner());
        entries.push(entry.clone());
    }
}

/// 空探活（未装配真实平台时的 fail-closed 状态）。
#[derive(Debug, Default)]
pub struct UnknownProbe;

impl ServiceProbePort for UnknownProbe {
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        ServiceStatus {
            service_id: service.id.clone(),
            status: HealthStatus::Unknown,
            detail: "平台探活未装配".into(),
            checked_at: 0,
        }
    }
}

/// 拒绝所有重启（未装配真实平台控制时的 fail-closed）。
#[derive(Debug, Default)]
pub struct DenyAllControl;

impl ServiceControlPort for DenyAllControl {
    fn restart(&self, _service_id: &str) -> Result<(), String> {
        Err("service_control_not_configured".into())
    }
}

/// 装配结果。
pub struct Composition {
    registry: Arc<ToolRegistry>,
    identity: Arc<dyn RemoteIdentityProvider>,
    audit: Arc<MemoryAuditStore>,
    actions: Arc<SafeActionService>,
}

impl Composition {
    pub fn service(&self) -> Arc<McpService> {
        Arc::new(
            McpService::new(
                Arc::new(McpToolAdapter::new(Arc::clone(&self.registry))),
                Arc::clone(&self.identity),
                Arc::clone(&self.audit) as Arc<dyn McpAuditPort>,
                McpServiceConfig::default(),
            )
            .with_system_actions(Arc::clone(&self.actions)),
        )
    }

    /// 是否配置了远程身份提供者（启动门禁用，§46）。
    pub fn identity_configured(&self) -> bool {
        // DenyAll 恒 false：远程绑定会被 startup gate 拒绝。
        // 接入真实 OAuth/OIDC provider 后改为读取其配置状态。
        false
    }
}

/// 装配（ Phase 1：空能力集）。
pub fn build() -> Result<Composition, String> {
    let registry = Arc::new(ToolRegistry::new());
    let services = Arc::new(ServiceRegistryService::new(
        Vec::new(),
        Arc::new(UnknownProbe),
    ));
    let audit_bridge = Arc::new(InMemoryActionAudit::default());
    let actions = Arc::new(SafeActionService::new(
        Arc::clone(&services),
        Arc::new(DenyAllControl),
        Arc::new(InMemoryConfirmationStore::default()),
        audit_bridge as Arc<dyn ActionAuditPort>,
        Arc::new(devtoolbox_core::server::DefaultActionRiskPolicy),
        SafeActionConfig::default(),
    ));
    Ok(Composition {
        registry,
        identity: Arc::new(DenyAllIdentityProvider),
        audit: Arc::new(MemoryAuditStore::default()),
        actions,
    })
}

/// 从注册表构造的 tool 数量（诊断输出）。
#[must_use]
pub fn tool_count(registry: &ToolRegistry) -> usize {
    registry.specs().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_application::server::action::ActionPlan;

    #[test]
    fn phase1_composition_fails_closed() {
        let composition = build().expect("compose");
        // 空注册表 → 无工具可发现。
        assert_eq!(tool_count(&composition.registry), 0);
        // 未配置身份 → 远程绑定被 startup gate 拒绝（§46）。
        assert!(!composition.identity_configured());
        // 空服务注册表 → restart 请求被拒。
        let request = devtoolbox_application::server::action::restart_request(
            "self-tools",
            "cli",
            "Self Tools",
        );
        match composition
            .actions
            .plan(&request, SessionTrust::LocalDesktop)
        {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "unknown_service"),
            other => panic!("expected denied, got {other:?}"),
        }
    }
}
