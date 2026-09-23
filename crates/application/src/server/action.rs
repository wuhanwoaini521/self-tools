//! Safe Action 服务（V7 §51-§71）——**V7 安全核心**。
//!
//! 调用链（与 §54 一一对应）：
//!
//! ```text
//! plan(request)                     → Authorize（策略 + 注册表 + cooldown）
//!                                      │
//!                    Read ────────────┴──────── System
//!                     │                           │
//!               直接执行                    签发 Confirmation
//!                                             │ 用户确认（confirm_action）
//!                                             ▼
//!                              confirm_and_execute(confirmation_id)
//!                                1. 取票据（不存在 → Denied）
//!                                2. 状态 Pending？否则 Denied（重放）
//!                                3. 未过期？否则 Expired
//!                                4. fingerprint 重算比对（TOCTOU）
//!                                5. 标记 Consumed（一次性）
//!                                6. 执行（ServiceControlPort）
//!                                7. 审计（含 DENIED/FAILED/SUCCESS）
//! ```
//!
//! 不变量：
//! - **无确认 = 不执行**（§71 硬 Gate）：`confirm_and_execute` 是唯一写入口；
//! - 确认后参数不可变（§70）：票据只存 fingerprint，执行时重算比对；
//! - 一次性（§60）：`Consumed` 后再用 → Denied。

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::server::{
    ActionAuthorizationDecision, ActionOutcome, ActionRequest, ActionRisk, ActionRiskPolicy,
    AuditEntry, AuditSource, Confirmation, ConfirmationState, DefaultActionRiskPolicy,
    RegisteredAction, SessionTrust,
};

use super::registry::ServiceRegistryService;

/// 执行端口（infrastructure 实现；只接受 typed action，§61）。
pub trait ServiceControlPort: Send + Sync {
    /// 重启已注册服务。返回 `Ok(())` = 成功；`Err(稳定错误码)` = 失败。
    fn restart(&self, service_id: &str) -> Result<(), String>;
}

/// 确认票据存储（内存 + 可选持久化由组合根决定；V7 用内存 + SQLite 审计）。
pub trait ConfirmationStorePort: Send + Sync {
    fn insert(&self, confirmation: Confirmation);
    fn get(&self, id: &str) -> Option<Confirmation>;
    fn update(&self, confirmation: Confirmation);
    /// 惰性清理过期 / 已消费票据。
    fn prune(&self, now: i64);
}

/// 审计存储端口（§63：所有 write attempt 都记账）。
pub trait ActionAuditPort: Send + Sync {
    fn record(&self, entry: &AuditEntry);
    fn recent(&self, limit: usize) -> Vec<AuditEntry>;
}

/// 内存审计存储（进程内；重启即丢。不引外部日志栈，V8 §111）。
#[derive(Debug, Default)]
pub struct InMemoryActionAudit {
    entries: Mutex<Vec<AuditEntry>>,
}

impl ActionAuditPort for InMemoryActionAudit {
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

/// 内存确认存储（默认实现；单进程桌面场景）。
#[derive(Debug, Default)]
pub struct InMemoryConfirmationStore {
    confirmations: Mutex<BTreeMap<String, Confirmation>>,
}

impl ConfirmationStorePort for InMemoryConfirmationStore {
    fn insert(&self, confirmation: Confirmation) {
        self.confirmations
            .lock()
            .insert(confirmation.id.clone(), confirmation);
    }

    fn get(&self, id: &str) -> Option<Confirmation> {
        self.confirmations.lock().get(id).cloned()
    }

    fn update(&self, confirmation: Confirmation) {
        self.confirmations
            .lock()
            .insert(confirmation.id.clone(), confirmation);
    }

    fn prune(&self, _now: i64) {
        let now = now_unix();
        self.confirmations
            .lock()
            .retain(|_, entry| entry.is_usable(now));
    }
}

/// 计划结果（§53：Read 直接执行，System 需确认）。
#[derive(Clone, Debug, PartialEq)]
pub enum ActionPlan {
    /// 已直接执行（Read 类；V7 暂无此类写操作）。
    Executed(ActionOutcome),
    /// 需要用户确认（携带票据；模型不得自行继续）。
    ConfirmationRequired(Confirmation),
    /// 被拒绝（稳定 reason + 人类可读说明）。
    Denied { reason: String, detail: String },
}

/// Safe Action 配置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafeActionConfig {
    /// 确认票据有效期秒数（§59：30–120）。
    pub confirmation_ttl_secs: i64,
    /// 同一目标冷却秒数（§67）。
    pub cooldown_secs: i64,
    /// 每会话 SYSTEM 操作上限（§67，按窗口）。
    pub max_system_per_session: usize,
}

impl Default for SafeActionConfig {
    fn default() -> Self {
        Self {
            confirmation_ttl_secs: 60,
            cooldown_secs: 60,
            max_system_per_session: 5,
        }
    }
}

/// Safe Action 服务。
pub struct SafeActionService {
    services: Arc<ServiceRegistryService>,
    control: Arc<dyn ServiceControlPort>,
    confirmations: Arc<dyn ConfirmationStorePort>,
    audit: Arc<dyn ActionAuditPort>,
    policy: Arc<dyn ActionRiskPolicy>,
    config: SafeActionConfig,
    /// cooldown / 计数（内存；进程级足够，重启即重置）。
    cooldowns: Mutex<BTreeMap<String, i64>>,
    session_counts: Mutex<BTreeMap<String, usize>>,
    sequence: Mutex<u64>,
    /// 审计来源标签（MCP 装配时设为 `Mcp`，§93；Arc 共享故内部可变）。
    audit_source: Mutex<AuditSource>,
}

impl SafeActionService {
    #[must_use]
    pub fn new(
        services: Arc<ServiceRegistryService>,
        control: Arc<dyn ServiceControlPort>,
        confirmations: Arc<dyn ConfirmationStorePort>,
        audit: Arc<dyn ActionAuditPort>,
        policy: Arc<dyn ActionRiskPolicy>,
        config: SafeActionConfig,
    ) -> Self {
        Self {
            services,
            control,
            confirmations,
            audit,
            policy,
            config,
            cooldowns: Mutex::new(BTreeMap::new()),
            session_counts: Mutex::new(BTreeMap::new()),
            sequence: Mutex::new(0),
            audit_source: Mutex::new(AuditSource::Desktop),
        }
    }

    /// 标记审计来源（V8 §93：MCP 装配时调用；`Arc` 共享，内部可变）。
    pub fn set_audit_source(&self, source: AuditSource) {
        *self.audit_source.lock() = source;
    }

    /// 默认策略 + 内存存储的便捷构造（组合根常用）。
    #[must_use]
    pub fn with_defaults(
        services: Arc<ServiceRegistryService>,
        control: Arc<dyn ServiceControlPort>,
        audit: Arc<dyn ActionAuditPort>,
    ) -> Self {
        Self::new(
            services,
            control,
            Arc::new(InMemoryConfirmationStore::default()),
            audit,
            Arc::new(DefaultActionRiskPolicy),
            SafeActionConfig::default(),
        )
    }

    /// §54 第一步：授权 + 风险分派。
    pub fn plan(&self, request: &ActionRequest, trust: SessionTrust) -> ActionPlan {
        let started = std::time::Instant::now();
        // 1) 目标必须在注册表（§35）。新增 `RegisteredAction` 变体时这个 match
        //    会给出编译错误 —— 强制显式决定新操作的授权路径。
        match &request.action {
            RegisteredAction::RestartService { service_id, .. } => {
                match self.services.resolve(service_id) {
                    Ok(descriptor) => {
                        if !descriptor.allows("restart") {
                            return self.deny(
                                request,
                                "action_not_allowed",
                                "该服务未声明允许 restart",
                                started,
                            );
                        }
                    }
                    Err(error) => {
                        let reason = match &error {
                            crate::error::ApplicationError::Server { reason, .. } => reason.clone(),
                            _ => "unknown_service".to_string(),
                        };
                        return self.deny(request, &reason, "服务未注册或 id 不合法", started);
                    }
                }
            }
        }
        // 2) 会话授权（§73 fail-closed）。
        match self.policy.authorize(&request.action, trust) {
            ActionAuthorizationDecision::Denied { reason, detail } => {
                return self.deny(request, &reason, &detail, started);
            }
            ActionAuthorizationDecision::Allowed => {}
        }
        // 3) 频率限制（§67）。
        if let Some(reason) = self.cooldown_violation(&request.action) {
            return self.deny(request, "cooldown", &reason, started);
        }
        if let Some(reason) = self.session_limit(&request.session_id) {
            return self.deny(request, "session_limit", &reason, started);
        }
        // 4) 风险分派（§53）。
        if request.risk.requires_confirmation() {
            ActionPlan::ConfirmationRequired(self.issue_confirmation(request))
        } else {
            ActionPlan::Executed(self.execute_now(request))
        }
    }

    /// §54 最后一步：用户确认后执行（重验证 → 执行 → 审计）。
    pub fn confirm_and_execute(
        &self,
        confirmation_id: &str,
        request: &ActionRequest,
    ) -> Result<ActionOutcome, crate::error::ApplicationError> {
        let started = std::time::Instant::now();
        let Some(mut confirmation) = self.confirmations.get(confirmation_id) else {
            self.audit_attempt(
                request,
                false,
                ActionOutcome::Denied,
                Some("unknown_confirmation"),
                started.elapsed(),
            );
            return Ok(ActionOutcome::Denied);
        };
        let now = now_unix();
        // §60 一次性。
        if confirmation.state != ConfirmationState::Pending {
            self.audit_attempt(
                request,
                false,
                ActionOutcome::Denied,
                Some("confirmation_replay"),
                started.elapsed(),
            );
            return Ok(ActionOutcome::Denied);
        }
        // §59 过期。
        if now >= confirmation.expires_at {
            confirmation.state = ConfirmationState::Expired;
            self.confirmations.update(confirmation);
            self.audit_attempt(
                request,
                false,
                ActionOutcome::Expired,
                Some("confirmation_expired"),
                started.elapsed(),
            );
            return Ok(ActionOutcome::Expired);
        }
        // §58/§70 TOCTOU：指纹必须逐字节一致。
        if confirmation.request_fingerprint != request.fingerprint() {
            self.audit_attempt(
                request,
                false,
                ActionOutcome::Denied,
                Some("fingerprint_mismatch"),
                started.elapsed(),
            );
            return Ok(ActionOutcome::Denied);
        }
        // §103 跨 client 隔离：票据绑签发时的 session（MCP principal 的 client_id）；
        // 其它 client 拿同一张票据也必须被拒（重放的一种形态）。
        if confirmation.session_id != request.session_id {
            self.audit_attempt(
                request,
                false,
                ActionOutcome::Denied,
                Some("session_mismatch"),
                started.elapsed(),
            );
            return Ok(ActionOutcome::Denied);
        }
        // 先标记消费（即使执行失败也不可重放）。
        confirmation.state = ConfirmationState::Consumed;
        self.confirmations.update(confirmation);

        // 执行（typed；无命令字符串）。
        let outcome = match &request.action {
            RegisteredAction::RestartService { service_id, .. } => {
                match self.control.restart(service_id) {
                    Ok(()) => ActionOutcome::Success,
                    Err(code) => {
                        self.audit_attempt(
                            request,
                            true,
                            ActionOutcome::Failed,
                            Some(&code),
                            started.elapsed(),
                        );
                        return Ok(ActionOutcome::Failed);
                    }
                }
            }
        };
        self.record_cooldown(&request.action);
        self.audit_attempt(request, true, outcome, None, started.elapsed());
        Ok(outcome)
    }

    /// 取消一个待确认票据（§62）。
    pub fn cancel(
        &self,
        confirmation_id: &str,
    ) -> Result<ActionOutcome, crate::error::ApplicationError> {
        let Some(mut confirmation) = self.confirmations.get(confirmation_id) else {
            return Ok(ActionOutcome::Denied);
        };
        if confirmation.state != ConfirmationState::Pending {
            return Ok(ActionOutcome::Denied);
        }
        confirmation.state = ConfirmationState::Cancelled;
        self.confirmations.update(confirmation);
        Ok(ActionOutcome::Cancelled)
    }

    /// 最近审计（§66 UI）。
    #[must_use]
    pub fn recent_audit(&self, limit: usize) -> Vec<AuditEntry> {
        self.audit.recent(limit)
    }

    /// 服务注册表（SafeAction 的授权来源；MCP 层解析 service_id 用）。
    #[must_use]
    pub fn registry(&self) -> &Arc<ServiceRegistryService> {
        &self.services
    }

    /// 确认票据存储（测试辅助：模拟「已过期 / 已消费」等时间相关状态）。
    #[doc(hidden)]
    #[must_use]
    pub fn confirmations_for_test(&self) -> Arc<dyn ConfirmationStorePort> {
        Arc::clone(&self.confirmations)
    }

    // --- 内部 ---

    fn issue_confirmation(&self, request: &ActionRequest) -> Confirmation {
        let now = now_unix();
        let confirmation = Confirmation {
            id: self.next_id("cfm"),
            request_fingerprint: request.fingerprint(),
            action_type: request.action.action_type().to_string(),
            target_id: request.action.target_id().to_string(),
            human_readable_summary: request.summary.clone(),
            risk: request.risk,
            created_at: now,
            expires_at: now + self.config.confirmation_ttl_secs.max(1),
            state: ConfirmationState::Pending,
            session_id: request.session_id.clone(),
        };
        self.confirmations.insert(confirmation.clone());
        confirmation
    }

    fn deny(
        &self,
        request: &ActionRequest,
        reason: &str,
        detail: &str,
        started: std::time::Instant,
    ) -> ActionPlan {
        self.audit_attempt(
            request,
            false,
            ActionOutcome::Denied,
            Some(reason),
            started.elapsed(),
        );
        ActionPlan::Denied {
            reason: reason.to_string(),
            detail: detail.to_string(),
        }
    }

    fn execute_now(&self, request: &ActionRequest) -> ActionOutcome {
        match &request.action {
            RegisteredAction::RestartService { service_id, .. } => {
                match self.control.restart(service_id) {
                    Ok(()) => ActionOutcome::Success,
                    Err(_) => ActionOutcome::Failed,
                }
            }
        }
    }

    fn audit_attempt(
        &self,
        request: &ActionRequest,
        confirmed: bool,
        outcome: ActionOutcome,
        error_code: Option<&str>,
        duration: std::time::Duration,
    ) {
        self.audit.record(&AuditEntry {
            id: self.next_id("aud"),
            timestamp: now_unix(),
            source: *self.audit_source.lock(),
            session_id: request.session_id.clone(),
            action_type: request.action.action_type().to_string(),
            target_id: request.action.target_id().to_string(),
            risk: request.risk,
            confirmed,
            result: outcome,
            duration_ms: duration.as_millis() as u64,
            error_code: error_code.map(str::to_string),
        });
    }

    fn cooldown_violation(&self, action: &RegisteredAction) -> Option<String> {
        let key = action.target_id().to_string();
        let now = now_unix();
        let last = self.cooldowns.lock().get(&key).copied();
        match last {
            Some(at) if now - at < self.config.cooldown_secs.max(0) => Some(format!(
                "{} 秒内已执行过同一操作，请稍后再试",
                self.config.cooldown_secs
            )),
            _ => None,
        }
    }

    fn record_cooldown(&self, action: &RegisteredAction) {
        self.cooldowns
            .lock()
            .insert(action.target_id().to_string(), now_unix());
    }

    fn session_limit(&self, session_id: &str) -> Option<String> {
        let used = self
            .session_counts
            .lock()
            .get(session_id)
            .copied()
            .unwrap_or(0);
        if used >= self.config.max_system_per_session {
            return Some("本会话的系统操作次数已达上限".to_string());
        }
        self.session_counts
            .lock()
            .insert(session_id.to_string(), used + 1);
        None
    }

    fn next_id(&self, prefix: &str) -> String {
        let mut sequence = self.sequence.lock();
        *sequence += 1;
        format!("{prefix}-{}-{:06}", now_unix(), *sequence)
    }
}

/// 当前 Unix 秒（application 已有 `time` 模块则复用）。
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// 便捷构造：`ActionRequest`（摘要由服务生成，不来自模型原文，§55）。
#[must_use]
pub fn restart_request(service_id: &str, session_id: &str, display_name: &str) -> ActionRequest {
    ActionRequest {
        action: RegisteredAction::RestartService {
            service_id: service_id.to_string(),
            expect: devtoolbox_core::server::HealthStatus::Healthy,
        },
        session_id: session_id.to_string(),
        summary: format!("重启服务「{display_name}」"),
        risk: ActionRisk::System,
        created_at: now_unix(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::registry::ServiceRegistryService;
    use devtoolbox_core::server::{HealthCheckKind, ServiceDescriptor, ServiceProviderType};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeControl {
        restarts: AtomicUsize,
        fail: bool,
    }

    impl ServiceControlPort for FakeControl {
        fn restart(&self, _service_id: &str) -> Result<(), String> {
            self.restarts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err("launchctl_kickstart_failed".into());
            }
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct MemoryAudit {
        entries: Mutex<Vec<AuditEntry>>,
    }

    impl ActionAuditPort for MemoryAudit {
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

    struct AlwaysDenyPolicy;

    impl ActionRiskPolicy for AlwaysDenyPolicy {
        fn authorize(
            &self,
            _action: &RegisteredAction,
            _trust: SessionTrust,
        ) -> ActionAuthorizationDecision {
            ActionAuthorizationDecision::denied("policy_denied", "策略拒绝")
        }
    }

    fn service(id: &str, allow: bool) -> ServiceDescriptor {
        ServiceDescriptor {
            id: id.into(),
            display_name: "Self Tools".into(),
            provider_ref: "com.example.self-tools".into(),
            provider_type: ServiceProviderType::Launchd,
            health_check: HealthCheckKind::Launchd,
            allowed_actions: if allow {
                vec!["restart".into()]
            } else {
                Vec::new()
            },
            ..ServiceDescriptor::default()
        }
    }

    fn setup(
        control_fail: bool,
        allow: bool,
        policy: Option<Arc<dyn ActionRiskPolicy>>,
        config: Option<SafeActionConfig>,
    ) -> (
        Arc<SafeActionService>,
        Arc<FakeControl>,
        Arc<MemoryAudit>,
        ActionRequest,
    ) {
        let control = Arc::new(FakeControl {
            restarts: AtomicUsize::new(0),
            fail: control_fail,
        });
        let audit = Arc::new(MemoryAudit::default());
        let services = Arc::new(ServiceRegistryService::new(
            vec![service("self-tools", allow)],
            Arc::new(super::super::service::NoopProbe),
        ));
        let service = Arc::new(SafeActionService::new(
            services,
            control.clone(),
            Arc::new(InMemoryConfirmationStore::default()),
            audit.clone(),
            policy.unwrap_or_else(|| Arc::new(DefaultActionRiskPolicy)),
            config.unwrap_or_default(),
        ));
        let request = restart_request("self-tools", "session-1", "Self Tools");
        (service, control, audit, request)
    }

    #[test]
    fn system_action_requires_confirmation() {
        let (service, control, _, request) = setup(false, true, None, None);
        let plan = service.plan(&request, SessionTrust::LocalDesktop);
        match plan {
            ActionPlan::ConfirmationRequired(confirmation) => {
                assert_eq!(confirmation.risk, ActionRisk::System);
                assert!(!confirmation.human_readable_summary.is_empty());
                assert_eq!(confirmation.request_fingerprint, request.fingerprint());
            }
            other => panic!("expected confirmation required, got {other:?}"),
        }
        assert_eq!(
            control.restarts.load(Ordering::SeqCst),
            0,
            "未确认不得执行（§71）"
        );
    }

    #[test]
    fn confirmed_action_executes_and_audits_success() {
        let (service, control, audit, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        let outcome = service
            .confirm_and_execute(&confirmation.id, &request)
            .expect("execute");
        assert_eq!(outcome, ActionOutcome::Success);
        assert_eq!(control.restarts.load(Ordering::SeqCst), 1);

        let entries = audit.recent(10);
        assert!(
            entries
                .iter()
                .any(|entry| entry.result == ActionOutcome::Success
                    && entry.confirmed
                    && entry.action_type == "services.restart")
        );
    }

    #[test]
    fn unconfirmed_execution_is_denied() {
        let (service, control, audit, request) = setup(false, true, None, None);
        // 没有 plan → 没有票据：直接 confirm 必须 Denied。
        let outcome = service
            .confirm_and_execute("cfm-does-not-exist", &request)
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Denied);
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
        assert!(
            audit
                .recent(10)
                .iter()
                .any(|entry| entry.result == ActionOutcome::Denied),
            "DENIED 也必须审计（§64）"
        );
    }

    #[test]
    fn confirmation_is_single_use() {
        let (service, control, _, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        assert_eq!(
            service
                .confirm_and_execute(&confirmation.id, &request)
                .expect("first"),
            ActionOutcome::Success
        );
        // 重放 → Denied，且不再执行。
        assert_eq!(
            service
                .confirm_and_execute(&confirmation.id, &request)
                .expect("replay"),
            ActionOutcome::Denied
        );
        assert_eq!(control.restarts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn expired_confirmation_is_rejected() {
        let (service, control, audit, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        // 直接把票据改成「已过期」形态（等价于等 TTL 走完）。
        let mut stale = confirmation.clone();
        stale.expires_at = 1;
        service.confirmations_for_test().update(stale);
        let outcome = service
            .confirm_and_execute(&confirmation.id, &request)
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Expired);
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
        assert!(
            audit
                .recent(10)
                .iter()
                .any(|entry| entry.result == ActionOutcome::Expired),
            "过期也要审计"
        );
    }

    #[test]
    fn target_mismatch_after_confirmation_is_denied() {
        let (service, control, _, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        // 确认的是 self-tools，执行请求换成 geo-explorer（未注册 + 指纹不同）。
        let mut tampered = request.clone();
        tampered.action = RegisteredAction::restart_service("geo-explorer").expect("valid id");
        let outcome = service
            .confirm_and_execute(&confirmation.id, &tampered)
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Denied, "确认 A 不得执行 B（§58）");
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn unknown_service_is_denied() {
        let (service, control, _, _) = setup(false, true, None, None);
        let request = restart_request("sshd", "session-1", "sshd");
        let plan = service.plan(&request, SessionTrust::LocalDesktop);
        match plan {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "unknown_service"),
            other => panic!("expected denied, got {other:?}"),
        }
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn action_not_in_allowed_list_is_denied() {
        let (service, _, _, request) = setup(false, false, None, None);
        match service.plan(&request, SessionTrust::LocalDesktop) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "action_not_allowed"),
            other => panic!("expected denied, got {other:?}"),
        }
    }

    #[test]
    fn untrusted_session_is_denied() {
        let (service, control, _, request) = setup(false, true, None, None);
        match service.plan(&request, SessionTrust::RemoteUntrusted) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "untrusted_session"),
            other => panic!("expected denied, got {other:?}"),
        }
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn restart_failure_is_audited() {
        let (service, _, audit, request) = setup(true, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        let outcome = service
            .confirm_and_execute(&confirmation.id, &request)
            .expect("outcome");
        assert_eq!(outcome, ActionOutcome::Failed);
        let entries = audit.recent(10);
        assert!(
            entries
                .iter()
                .any(|entry| entry.result == ActionOutcome::Failed)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.error_code.as_deref() == Some("launchctl_kickstart_failed")),
            "失败须带稳定错误码"
        );
    }

    #[test]
    fn rapid_repeated_restart_is_rate_limited() {
        let (service, control, _, request) = setup(false, true, None, None);
        // 第一次：确认并执行成功。
        let ActionPlan::ConfirmationRequired(first) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        assert_eq!(
            service
                .confirm_and_execute(&first.id, &request)
                .expect("first"),
            ActionOutcome::Success
        );
        // 第二次（冷却内）：必须 Denied。
        match service.plan(&request, SessionTrust::LocalDesktop) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "cooldown"),
            other => panic!("expected cooldown denial, got {other:?}"),
        }
        assert_eq!(control.restarts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn session_system_action_limit_enforced() {
        let config = SafeActionConfig {
            confirmation_ttl_secs: 60,
            cooldown_secs: 0, // 关掉 cooldown，单独验证会话上限
            max_system_per_session: 2,
        };
        let (service, _, _, request) = setup(false, true, None, Some(config));
        for _ in 0..2 {
            let ActionPlan::ConfirmationRequired(confirmation) =
                service.plan(&request, SessionTrust::LocalDesktop)
            else {
                panic!("expected confirmation");
            };
            assert_eq!(
                service
                    .confirm_and_execute(&confirmation.id, &request)
                    .expect("ok"),
                ActionOutcome::Success
            );
        }
        match service.plan(&request, SessionTrust::LocalDesktop) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "session_limit"),
            other => panic!("expected session limit, got {other:?}"),
        }
    }

    #[test]
    fn policy_denial_short_circuits() {
        let (service, control, _, request) =
            setup(false, true, Some(Arc::new(AlwaysDenyPolicy)), None);
        match service.plan(&request, SessionTrust::LocalDesktop) {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "policy_denied"),
            other => panic!("expected denied, got {other:?}"),
        }
        assert_eq!(control.restarts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn confirmation_is_bound_to_its_session() {
        // §103：票据只属于签发它的 session（MCP client_id）。
        let (service, _control, _audit, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        let mut other_client = request.clone();
        other_client.session_id = "someone-else".to_string();
        assert_eq!(
            service
                .confirm_and_execute(&confirmation.id, &other_client)
                .expect("outcome"),
            ActionOutcome::Denied,
            "其它 session 不得消费票据"
        );
        // 原 session 仍可执行一次。
        assert_eq!(
            service
                .confirm_and_execute(&confirmation.id, &request)
                .expect("outcome"),
            ActionOutcome::Success
        );
    }

    #[test]
    fn cancel_marks_ticket_cancelled() {
        let (service, _, _, request) = setup(false, true, None, None);
        let ActionPlan::ConfirmationRequired(confirmation) =
            service.plan(&request, SessionTrust::LocalDesktop)
        else {
            panic!("expected confirmation");
        };
        assert_eq!(
            service.cancel(&confirmation.id).expect("cancel"),
            ActionOutcome::Cancelled
        );
        assert_eq!(
            service
                .confirm_and_execute(&confirmation.id, &request)
                .expect("after cancel"),
            ActionOutcome::Denied,
            "取消后不得执行"
        );
    }
}
