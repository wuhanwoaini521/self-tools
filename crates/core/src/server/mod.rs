//! Server 领域契约（V7 Track A，§12-§25）。
//!
//! 家庭服务器域：**只描述能力与状态，不执行任何系统操作**。
//! 平台细节（macOS launchd / df / sysctl）全部在 infrastructure；
//! 本模块只放纯数据 + 纯策略（健康评估、阈值、id 校验、URL 校验）。
//!
//! 三条铁律对应 V7 Plan §1.3：
//! 1. 本模块与 application 对 `std::process::Command` 的引用为 0；
//! 2. 模型可见的只有 [`ServiceDescriptor::id`] 这类稳定标识，
//!    `provider_ref`（launchd label / container name）由 infrastructure 映射；
//! 3. 任何「无法证明安全」的判定都 fail-closed（§8：`Unknown` 而非乐观假设）。

use std::path::{Component, Path};

pub mod action;
pub mod health;
pub mod logs;
pub mod metrics;
pub mod registry;

pub use action::{
    ActionAuthorizationDecision, ActionOutcome, ActionRequest, ActionRisk,
    ActionRiskPolicy, AuditEntry, Confirmation, ConfirmationState, DefaultActionRiskPolicy,
    RegisteredAction, SessionTrust,
};
pub use health::{
    HealthReason, HealthReport, HealthStatus, Thresholds, evaluate_memory, evaluate_storage,
};
pub use logs::{LogReadRequest, LogReadResult, LogWindow};
pub use metrics::{CpuMetrics, MemoryMetrics, Platform, StorageMetrics, SystemMetrics, VolumeKind};
pub use registry::{
    is_http_url,
    ApplicationDescriptor, ApplicationStatus, HealthCheckKind, LogSource, ServiceDescriptor,
    ServiceProviderType, ServiceStatus,
};

/// 服务 / 应用 / automation 的稳定 id 校验（V7 §35/§36/§155）。
///
/// 只允许小写英数字与 `.` / `_` / `-`：从契约层封死 `foo; rm -rf /`、
/// `--label`、换行与空串等注入形态。**模型永远只能传这种 id。**
#[must_use]
pub fn is_valid_id(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return false;
    }
    if trimmed != raw {
        return false;
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap_or('_');
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}

/// 路径是否包含 `..` 组件（日志 / 配置路径校验复用）。
#[must_use]
pub fn contains_traversal(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids_accept_registry_shapes() {
        for ok in [
            "self-tools",
            "self_tools",
            "geo.explorer",
            "a",
            "0service",
            "com.example.backend",
        ] {
            assert!(is_valid_id(ok), "应接受: {ok}");
        }
    }

    #[test]
    fn invalid_ids_reject_injection_shapes() {
        // §155：模型传入的 service_id 必须无法拼成命令或参数。
        for bad in [
            "",
            "   ",
            "foo; rm -rf /",
            "foo && reboot",
            "--label",
            "Foo",
            "foo bar",
            "foo\nbar",
            "../etc",
            "a".repeat(65).as_str(),
            "-leading-dash",
            ".hidden",
            "com.example/../../etc",
        ] {
            assert!(!is_valid_id(bad), "应拒绝: {bad:?}");
        }
    }

    #[test]
    fn traversal_detection_covers_parent_components() {
        assert!(contains_traversal(Path::new("/var/log/../../etc/passwd")));
        assert!(contains_traversal(Path::new("logs/../x")));
        assert!(!contains_traversal(Path::new("/var/log/system.log")));
    }
}
