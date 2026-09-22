//! 家庭服务器域（V7 Track A/B/C，§12-§71）。
//!
//! application 只依赖 core 契约与端口；平台细节（launchd / df / sysctl /
//! HTTP 探活 / 日志文件读取）全部由 infrastructure 实现，
//! 由 `apps/desktop` 组合根装配（与 V5/V6 同形态）。

pub mod action;
pub mod logs;
pub mod ports;
pub mod registry;
pub mod service;

pub use action::{
    ActionAuditPort, ActionPlan, ConfirmationStorePort, InMemoryActionAudit,
    InMemoryConfirmationStore,
    SafeActionConfig, SafeActionService, ServiceControlPort, restart_request,
};
// core 契约经 application 一并暴露（组合根只依赖 application + core）。
pub use devtoolbox_core::server::{
    ActionOutcome, ActionRequest, ActionRisk, AuditEntry, Confirmation, ConfirmationState,
    RegisteredAction, SessionTrust,
};
pub use logs::LogRedactor;
pub use ports::{
    ApplicationProbePort, HealthProbePort, LogTailPort, ServiceProbePort, SystemMetricsProvider,
};
pub use registry::{ApplicationRegistryService, ServiceRegistryService};
pub use service::{ServerConfig, ServerService, ServerStatus};
