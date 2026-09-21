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
    ActionAuditPort, ConfirmationStorePort, SafeActionService, ServiceControlPort,
};
pub use logs::LogRedactor;
pub use ports::{
    ApplicationProbePort, HealthProbePort, LogTailPort, ServiceProbePort, SystemMetricsProvider,
};
pub use registry::{ApplicationRegistryService, ServiceRegistryService};
pub use service::{ServerConfig, ServerService, ServerStatus};
