//! 家庭服务器端口（V7 §14/§29/§37/§49）。
//!
//! 端口语义与 V5/V6 一致：application 定义、infrastructure 实现、desktop 装配。
//! 这里**没有**任何 `std::process` 引用——平台调用只存在于适配器。

use devtoolbox_core::server::{
    ApplicationDescriptor, LogReadResult, ServiceDescriptor, ServiceStatus,
    SystemMetrics,
};
use devtoolbox_core::server::{ApplicationStatus, HealthStatus};

/// 系统指标端口（§14）。
pub trait SystemMetricsProvider: Send + Sync {
    /// 采样系统指标。任何字段不可用都是合法返回（`Unknown` 由上层降级）。
    fn metrics(&self) -> Result<SystemMetrics, String>;
}

/// 服务探活端口（§29：launchd 状态 / HTTP 探活由 infra 实现）。
pub trait ServiceProbePort: Send + Sync {
    /// 探测单个已注册服务的状态（不得抛出；失败 → `Unknown` + detail）。
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus;
}

/// 应用探活端口（§49：有界 HTTP health check）。
pub trait ApplicationProbePort: Send + Sync {
    fn probe(&self, app: &ApplicationDescriptor) -> ApplicationStatus;
}

/// 通用健康探活（供 dashboard 批量使用；失败 → `Unknown`）。
pub trait HealthProbePort: Send + Sync {
    fn health(&self, target: &str) -> (HealthStatus, String);
}

/// 日志尾读端口（§37-§39：路径只能来自注册表）。
pub trait LogTailPort: Send + Sync {
    /// 读取注册日志源的尾部（已按硬限制截断；未脱敏——脱敏在 application）。
    fn tail(
        &self,
        service: &ServiceDescriptor,
        log_source_id: &str,
        max_lines: usize,
        max_bytes: usize,
        max_age_secs: u64,
    ) -> Result<LogReadResult, String>;
}

/// 端口错误 → `ApplicationError::Server`（稳定 reason，不回显内容）。
pub(crate) fn server_error(reason: &str, message: impl Into<String>) -> crate::error::ApplicationError {
    crate::error::ApplicationError::Server {
        reason: reason.to_string(),
        message: message.into(),
    }
}

/// 便捷：把端口 `Result` 映射为应用错误。
pub(crate) fn map_port_error<T>(reason: &str, result: Result<T, String>) -> Result<T, crate::error::ApplicationError> {
    result.map_err(|message| server_error(reason, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_error_carries_stable_reason() {
        let error = server_error("metrics_unavailable", "采样失败");
        match error {
            crate::error::ApplicationError::Server { reason, message } => {
                assert_eq!(reason, "metrics_unavailable");
                assert_eq!(message, "采样失败");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn map_port_error_preserves_message() {
        let result = map_port_error("probe_failed", Err::<(), _>("boom".into()));
        assert!(result.is_err());
        assert!(map_port_error("probe_failed", Ok::<(), String>(())).is_ok());
    }
}
