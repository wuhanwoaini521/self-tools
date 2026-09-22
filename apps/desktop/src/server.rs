//! Home Server 组合根（V7 Gates 3-7）。
//!
//! 装配：平台指标 adapter + 注册表（来源 = settings）+ 安全动作层
//! （确认存储 + 审计 + launchd 控制）。**不含任何业务规则**。
//!
//! 依赖方向不变：`application → infrastructure` = 0；desktop 是唯一组合根。

use std::path::Path;
use std::sync::Arc;

use devtoolbox_application::server::action::{
    ActionAuditPort, InMemoryConfirmationStore, SafeActionConfig, SafeActionService,
    ServiceControlPort,
};
use devtoolbox_application::server::{
    ActionOutcome, ActionRequest, ActionRisk, AuditEntry, Confirmation,
};
use devtoolbox_application::server::ports::LogTailPort;
use devtoolbox_application::server::registry::{
    ApplicationRegistryService, ServiceRegistryService,
};
use devtoolbox_application::server::service::{ServerConfig, ServerService};
use devtoolbox_core::server::{
    ApplicationDescriptor, ApplicationStatus, HealthStatus, RegisteredAction, ServiceDescriptor,
    ServiceStatus, SessionTrust,
};
use devtoolbox_core::settings::AppSettings;

use crate::knowledge::SettingsLoader;
use crate::server_adapters::{
    HttpAppProbeAdapter, LaunchdControlAdapter, LaunchdProbeAdapter, LogTailAdapter,
    SystemMetricsAdapter,
};

/// SQLite 审计存储（`config/server_actions.db`；复用 infra SQLite 模式）。
pub struct SqliteAuditStore {
    store: devtoolbox_infrastructure::ServerActionAuditSqlite,
    /// 保留策略（§112：条数 / 天数）。
    max_entries: usize,
    retention_days: i64,
}

impl SqliteAuditStore {
    #[must_use]
    pub fn new(
        store: devtoolbox_infrastructure::ServerActionAuditSqlite,
        max_entries: usize,
        retention_days: i64,
    ) -> Self {
        Self {
            store,
            max_entries,
            retention_days,
        }
    }
}

impl ActionAuditPort for SqliteAuditStore {
    fn record(&self, entry: &AuditEntry) {
        // 审计写入失败不得影响操作结果（只记错误，不中断执行链）。
        if let Err(error) = self.store.record(entry) {
            eprintln!("[server] audit write failed: {error}");
            return;
        }
        // §112：惰性裁剪（每 64 条一次，避免每次写入都全表 DELETE）。
        if entry.timestamp % 64 == 0
            && let Err(error) = self
                .store
                .prune(self.max_entries, self.retention_days, entry.timestamp)
        {
            eprintln!("[server] audit prune failed: {error}");
        }
    }

    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        self.store.recent(limit).unwrap_or_default()
    }
}

/// 内存审计（未配置 SQLite 时的降级；重启即丢，但不影响写路径可用性）。
#[derive(Debug, Default)]
pub struct MemoryAuditStore {
    entries: parking_lot::Mutex<Vec<AuditEntry>>,
}

impl ActionAuditPort for MemoryAuditStore {
    fn record(&self, entry: &AuditEntry) {
        self.entries.lock().push(entry.clone());
    }

    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        self.entries
            .lock()
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

/// launchd 控制（未配置平台时的 fail-closed 占位）。
#[derive(Debug, Default)]
pub struct DisabledServiceControl;

impl ServiceControlPort for DisabledServiceControl {
    fn restart(&self, _service_id: &str) -> Result<(), String> {
        Err("service_control_disabled".to_string())
    }
}

/// Home Server 运行时。
pub struct ServerRuntime {
    settings_loader: SettingsLoader,
    pub server: Arc<ServerService>,
    pub services: Arc<ServiceRegistryService>,
    pub apps: Arc<ApplicationRegistryService>,
    pub actions: Arc<SafeActionService>,
    pub logs: Arc<dyn LogTailPort>,
    pub trust: SessionTrust,
}

impl ServerRuntime {
    /// 装配（注册表来自 settings；无注册条目 = 无能力，fail-closed）。
    pub fn build(
        config_directory: &Path,
        settings: SettingsLoader,
        trust: SessionTrust,
    ) -> Result<Self, String> {
        let (services, apps) = registered_from(&settings);
        let control: Arc<dyn ServiceControlPort> = if services.is_empty() {
            Arc::new(DisabledServiceControl)
        } else {
            Arc::new(LaunchdControlAdapter::new(&services))
        };
        Self::assemble(config_directory, settings, trust, services, apps, control)
    }

    /// 用显式注册表装配（测试与桌面设置页使用）。
    pub fn assemble(
        config_directory: &Path,
        settings: SettingsLoader,
        trust: SessionTrust,
        services: Vec<ServiceDescriptor>,
        apps: Vec<ApplicationDescriptor>,
        control: Arc<dyn ServiceControlPort>,
    ) -> Result<Self, String> {
        // §22/§59/§67/§112：阈值 / TTL / 冷却 / 会话上限 / 审计保留全部来自设置
        // （旧 settings.json 无 server 段 → serde default）。
        let server_settings = (settings)()
            .map(|loaded| loaded.server)
            .unwrap_or_default();
        let server_config = ServerConfig {
            thresholds: server_settings.thresholds,
            health_cache_secs: 5,
        };
        let action_config = SafeActionConfig {
            confirmation_ttl_secs: server_settings.confirmation_ttl_secs,
            cooldown_secs: server_settings.cooldown_secs,
            max_system_per_session: server_settings.max_system_per_session,
        };
        let audit: Arc<dyn ActionAuditPort> =
            match devtoolbox_infrastructure::ServerActionAuditSqlite::open_store(
                config_directory.join("server_actions.db"),
            ) {
                Ok(store) => Arc::new(SqliteAuditStore::new(
                    store,
                    server_settings.audit_max_entries,
                    server_settings.audit_retention_days,
                )),
                Err(error) => {
                    eprintln!("[server] audit store unavailable ({error}); using memory audit");
                    Arc::new(MemoryAuditStore::default())
                }
            };
        let service_registry = Arc::new(ServiceRegistryService::new(
            services.clone(),
            Arc::new(LaunchdProbeAdapter::new(&services)),
        ));
        let app_registry = Arc::new(ApplicationRegistryService::new(
            apps,
            Arc::new(HttpAppProbeAdapter::new()),
        ));
        let server = Arc::new(ServerService::new(
            Arc::new(SystemMetricsAdapter::default()),
            Arc::clone(&service_registry),
            Arc::clone(&app_registry),
            server_config,
        ));
        let actions = Arc::new(SafeActionService::new(
            Arc::clone(&service_registry),
            control,
            Arc::new(InMemoryConfirmationStore::default()),
            audit,
            Arc::new(devtoolbox_core::server::DefaultActionRiskPolicy),
            action_config,
        ));
        Ok(Self {
            settings_loader: settings,
            server,
            services: service_registry,
            apps: app_registry,
            actions,
            logs: Arc::new(LogTailAdapter::default()),
            trust,
        })
    }

    /// 从设置读取注册表（过滤非法条目；`serde(default)` 保证旧 settings 可用）。
    #[must_use]
    pub fn settings_snapshot(loader: &SettingsLoader) -> (Vec<ServiceDescriptor>, Vec<ApplicationDescriptor>) {
        registered_from(loader)
    }

    // --- 命令层薄封装（UI 调用；不含业务） ---

    pub fn status(&self) -> Result<devtoolbox_application::server::ServerStatus, String> {
        self.server.status().map_err(|error| error.to_string())
    }

    pub fn service_status(
        &self,
        service_id: &str,
    ) -> Result<ServiceStatus, String> {
        self.services
            .status(service_id)
            .map_err(|error| error.to_string())
    }

    pub fn app_status(&self, app_id: &str) -> Result<ApplicationStatus, String> {
        self.apps.status(app_id).map_err(|error| error.to_string())
    }

    /// 请求重启（模型路径）：只签发票据。
    pub fn request_restart(
        &self,
        service_id: &str,
    ) -> Result<Confirmation, String> {
        let service = self
            .services
            .resolve(service_id)
            .map_err(|error| error.to_string())?;
        let request = ActionRequest {
            action: RegisteredAction::RestartService {
                service_id: service.id.clone(),
                expect: HealthStatus::Healthy,
            },
            session_id: "desktop".to_string(),
            summary: format!("重启服务「{}」", service.display_name),
            risk: ActionRisk::System,
            created_at: unix_now(),
        };
        match self.actions.plan(&request, self.trust) {
            devtoolbox_application::server::action::ActionPlan::ConfirmationRequired(
                confirmation,
            ) => Ok(confirmation),
            devtoolbox_application::server::action::ActionPlan::Denied { reason, detail } => {
                Err(format!("{reason}: {detail}"))
            }
            devtoolbox_application::server::action::ActionPlan::Executed(_) => {
                Err("unexpected_direct_execution".to_string())
            }
        }
    }

    /// 用户确认后执行（UI 路径）：重验证 → 执行 → 审计。
    pub fn confirm_restart(
        &self,
        confirmation_id: &str,
        service_id: &str,
    ) -> Result<ActionOutcome, String> {
        let service = self
            .services
            .resolve(service_id)
            .map_err(|error| error.to_string())?;
        let request = ActionRequest {
            action: RegisteredAction::RestartService {
                service_id: service.id.clone(),
                expect: HealthStatus::Healthy,
            },
            session_id: "desktop".to_string(),
            summary: format!("重启服务「{}」", service.display_name),
            risk: ActionRisk::System,
            created_at: unix_now(),
        };
        self.actions
            .confirm_and_execute(confirmation_id, &request)
            .map_err(|error| error.to_string())
    }

    pub fn cancel(&self, confirmation_id: &str) -> Result<ActionOutcome, String> {
        self.actions
            .cancel(confirmation_id)
            .map_err(|error| error.to_string())
    }

    /// MCP 传输设置（设置页展示；命令层用）。
    #[must_use]
    pub fn mcp_settings(&self) -> devtoolbox_core::settings::McpSettings {
        (self.settings_loader)()
            .map(|settings| settings.server.mcp)
            .unwrap_or_default()
    }

    pub fn recent_actions(&self, limit: usize) -> Vec<AuditEntry> {
        self.actions.recent_audit(limit)
    }
}

/// 从设置读注册表并过滤非法条目。
fn registered_from(
    loader: &SettingsLoader,
) -> (Vec<ServiceDescriptor>, Vec<ApplicationDescriptor>) {
    let settings = loader().unwrap_or_else(|_| AppSettings::default());
    let services: Vec<ServiceDescriptor> = settings
        .server
        .services
        .into_iter()
        .filter(|service| service.is_valid())
        .collect();
    let apps: Vec<ApplicationDescriptor> = settings
        .server
        .applications
        .into_iter()
        .filter(|app| app.is_valid())
        .collect();
    (services, apps)
}

/// 默认信任级别：桌面 = LocalDesktop（§76）。
#[must_use]
pub fn desktop_trust() -> SessionTrust {
    SessionTrust::LocalDesktop
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_application::server::action::ActionPlan;

    /// 桌面测试用计数控制（真实 launchd 由 platform 提供；测试不触系统）。
    #[derive(Debug, Default)]
    struct DesktopCountingControl {
        restarts: std::sync::atomic::AtomicUsize,
    }

    impl ServiceControlPort for DesktopCountingControl {
        fn restart(&self, _service_id: &str) -> Result<(), String> {
            self.restarts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    fn loader() -> SettingsLoader {
        Arc::new(|| Ok(AppSettings::default()))
    }

    #[test]
    fn empty_registry_builds_fail_closed_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = ServerRuntime::build(directory.path(), loader(), desktop_trust())
            .expect("build with empty registry");
        assert!(runtime.services.list().is_empty());
        assert!(runtime.apps.list().is_empty());
        // 未配置注册表 → 重启请求必须失败（无服务可操作）。
        assert!(runtime.request_restart("self-tools").is_err());
        assert!(runtime.recent_actions(10).is_empty());
    }

    #[test]
    fn registered_service_flows_through_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        let services = vec![
            devtoolbox_infrastructure::server::test_support::service("self-tools"),
        ];
        let runtime = ServerRuntime::assemble(
            directory.path(),
            loader(),
            desktop_trust(),
            services,
            Vec::new(),
            Arc::new(DesktopCountingControl::default()),
        )
        .expect("assemble");

        // 1) 请求 → 票据（不执行）。
        let confirmation = runtime.request_restart("self-tools").expect("confirmation");
        assert_eq!(confirmation.state, devtoolbox_core::server::ConfirmationState::Pending);

        // 2) 未确认 → 拒绝。
        let denied = runtime
            .confirm_restart("cfm-unknown", "self-tools")
            .expect("outcome");
        assert_eq!(denied, ActionOutcome::Denied);

        // 3) 确认 → 执行 + 审计。
        let outcome = runtime
            .confirm_restart(&confirmation.id, "self-tools")
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Success);
        let audit = runtime.recent_actions(10);
        assert!(audit.iter().any(|entry| entry.result == ActionOutcome::Success));
        assert!(audit.iter().any(|entry| entry.confirmed));

        // 4) 冷却期内再次请求 → 拒绝。
        assert!(runtime.request_restart("self-tools").is_err());
    }

    #[test]
    fn untrusted_session_denies_write_even_with_registry() {
        let directory = tempfile::tempdir().unwrap();
        let services = vec![
            devtoolbox_infrastructure::server::test_support::service("self-tools"),
        ];
        let runtime = ServerRuntime::assemble(
            directory.path(),
            loader(),
            SessionTrust::RemoteUntrusted,
            services,
            Vec::new(),
            Arc::new(DesktopCountingControl::default()),
        )
        .expect("assemble");
        let error = runtime.request_restart("self-tools").expect_err("denied");
        assert!(error.contains("untrusted_session"), "{error}");
    }

    #[test]
    fn plan_reports_denied_reason_for_unknown_service() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = ServerRuntime::build(directory.path(), loader(), desktop_trust())
            .expect("build");
        let request = ActionRequest {
            action: RegisteredAction::restart_service("sshd").expect("valid id"),
            session_id: "s".into(),
            summary: "x".into(),
            risk: ActionRisk::System,
            created_at: 0,
        };
        match runtime.actions.plan(&request, desktop_trust()) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "unknown_service"),
            other => panic!("expected denied, got {other:?}"),
        }
    }
}
