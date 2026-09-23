//! 健康模型（V7 §20-§22）。
//!
//! 四态而非 bool（§20），且**必须可解释**（§21：`HealthReason` 说明为什么 DEGRADED）。
//! 阈值配置化（§22）——本模块只放纯评估函数，不含 magic number。

use serde::{Deserialize, Serialize};

use super::metrics::{MemoryMetrics, StorageMetrics};

/// 健康状态（§20）。`Unknown` 是**合法且常见**的结果（探测失败 ≠ 不健康）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    #[default]
    Unknown,
}

impl HealthStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            HealthStatus::Healthy => "正常",
            HealthStatus::Degraded => "降级",
            HealthStatus::Unhealthy => "异常",
            HealthStatus::Unknown => "未知",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "healthy" => Some(HealthStatus::Healthy),
            "degraded" => Some(HealthStatus::Degraded),
            "unhealthy" => Some(HealthStatus::Unhealthy),
            "unknown" => Some(HealthStatus::Unknown),
            _ => None,
        }
    }

    /// 严重度排序（用于聚合取最差）。
    #[must_use]
    pub fn severity(self) -> u8 {
        match self {
            HealthStatus::Healthy => 0,
            HealthStatus::Unknown => 1,
            HealthStatus::Degraded => 2,
            HealthStatus::Unhealthy => 3,
        }
    }

    /// 聚合：取参与项中最差的状态；无参与项 → `Unknown`。
    #[must_use]
    pub fn worst(statuses: impl IntoIterator<Item = HealthStatus>) -> HealthStatus {
        statuses
            .into_iter()
            .max_by_key(|status| status.severity())
            .unwrap_or(HealthStatus::Unknown)
    }
}

/// 健康原因（§21：可解释；`detail` 只放稳定标识，不放内容）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthReason {
    /// 稳定机器可读码：`disk_usage` / `memory_usage` / `cpu_usage` /
    /// `metrics_unavailable` / `service_unavailable` / `health_endpoint_failed`。
    pub code: String,
    /// 人类可读说明（不含敏感内容）。
    pub detail: String,
}

impl HealthReason {
    #[must_use]
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }
}

/// 健康评估阈值（§22：全部配置化，禁止散落 magic number）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    pub disk_warn_ratio: f32,
    pub disk_critical_ratio: f32,
    pub memory_warn_ratio: f32,
    pub cpu_warn_ratio: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            disk_warn_ratio: 0.80,
            disk_critical_ratio: 0.92,
            memory_warn_ratio: 0.85,
            cpu_warn_ratio: 0.90,
        }
    }
}

impl Thresholds {
    /// 归一化到合法区间（防止配置写出 0 / >1 导致全部告警或全部沉默）。
    #[must_use]
    pub fn normalized(self) -> Self {
        let clamp = |value: f32| value.clamp(0.01, 1.0);
        Self {
            disk_warn_ratio: clamp(self.disk_warn_ratio),
            disk_critical_ratio: clamp(self.disk_critical_ratio),
            memory_warn_ratio: clamp(self.memory_warn_ratio),
            cpu_warn_ratio: clamp(self.cpu_warn_ratio),
        }
    }
}

/// 一份健康报告：总状态 + 全部原因（§19/§21）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    pub overall: HealthStatus,
    pub reasons: Vec<HealthReason>,
}

impl HealthReport {
    #[must_use]
    pub fn healthy() -> Self {
        Self {
            overall: HealthStatus::Healthy,
            reasons: Vec::new(),
        }
    }

    #[must_use]
    pub fn unknown(reason: HealthReason) -> Self {
        Self {
            overall: HealthStatus::Unknown,
            reasons: vec![reason],
        }
    }

    /// 合并（取最差状态，原因累加）。
    #[must_use]
    pub fn merge(mut self, other: &HealthReport) -> Self {
        self.overall = HealthStatus::worst([self.overall, other.overall]);
        self.reasons.extend(other.reasons.iter().cloned());
        self
    }
}

/// 存储健康（§16/§17：只评估可见卷；临时/虚拟卷不参与告警）。
#[must_use]
pub fn evaluate_storage(volumes: &[StorageMetrics], thresholds: &Thresholds) -> HealthReport {
    let thresholds = thresholds.normalized();
    let mut worst = HealthStatus::Healthy;
    let mut reasons: Vec<HealthReason> = Vec::new();
    let mut considered = 0usize;
    for volume in volumes.iter().filter(|entry| entry.kind.is_user_visible()) {
        considered += 1;
        let Some(ratio) = volume.usage_ratio() else {
            continue;
        };
        if ratio >= thresholds.disk_critical_ratio {
            worst = HealthStatus::Unhealthy;
            reasons.push(HealthReason::new(
                "disk_usage",
                format!("{} 使用率 {:.0}% 达到危险阈值", volume.mount, ratio * 100.0),
            ));
        } else if ratio >= thresholds.disk_warn_ratio {
            worst = HealthStatus::worst([worst, HealthStatus::Degraded]);
            reasons.push(HealthReason::new(
                "disk_usage",
                format!("{} 使用率 {:.0}% 超过警告阈值", volume.mount, ratio * 100.0),
            ));
        }
    }
    if considered == 0 {
        return HealthReport::unknown(HealthReason::new(
            "metrics_unavailable",
            "没有可评估的存储卷",
        ));
    }
    HealthReport {
        overall: worst,
        reasons,
    }
}

/// 内存健康。
#[must_use]
pub fn evaluate_memory(memory: &MemoryMetrics, thresholds: &Thresholds) -> HealthReport {
    let Some(ratio) = memory.usage_ratio() else {
        return HealthReport::unknown(HealthReason::new("metrics_unavailable", "内存指标不可用"));
    };
    let thresholds = thresholds.normalized();
    if ratio >= thresholds.memory_warn_ratio {
        return HealthReport {
            overall: HealthStatus::Degraded,
            reasons: vec![HealthReason::new(
                "memory_usage",
                format!("内存使用率 {:.0}% 超过警告阈值", ratio * 100.0),
            )],
        };
    }
    HealthReport::healthy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::metrics::VolumeKind;

    fn volume(mount: &str, used: u64, total: u64, kind: VolumeKind) -> StorageMetrics {
        StorageMetrics {
            mount: mount.to_string(),
            used_bytes: used,
            total_bytes: total,
            available_bytes: total - used,
            kind,
        }
    }

    #[test]
    fn status_severity_orders_unknown_between_healthy_and_degraded() {
        assert!(
            HealthStatus::Unknown.severity() > HealthStatus::Healthy.severity()
                && HealthStatus::Unknown.severity() < HealthStatus::Degraded.severity()
        );
        assert_eq!(
            HealthStatus::worst([HealthStatus::Healthy, HealthStatus::Unhealthy]),
            HealthStatus::Unhealthy
        );
        assert_eq!(
            HealthStatus::worst(Vec::new()),
            HealthStatus::Unknown,
            "无参与项 → Unknown（不乐观）"
        );
    }

    #[test]
    fn disk_thresholds_produce_warn_then_critical() {
        let thresholds = Thresholds::default();
        let warn = evaluate_storage(&[volume("/", 85, 100, VolumeKind::SystemDisk)], &thresholds);
        assert_eq!(warn.overall, HealthStatus::Degraded);
        assert_eq!(warn.reasons[0].code, "disk_usage");

        let critical =
            evaluate_storage(&[volume("/", 95, 100, VolumeKind::SystemDisk)], &thresholds);
        assert_eq!(critical.overall, HealthStatus::Unhealthy);

        let ok = evaluate_storage(&[volume("/", 10, 100, VolumeKind::SystemDisk)], &thresholds);
        assert_eq!(ok.overall, HealthStatus::Healthy);
        assert!(ok.reasons.is_empty());
    }

    #[test]
    fn temporary_volumes_do_not_trigger_alerts() {
        let thresholds = Thresholds::default();
        let report = evaluate_storage(
            &[volume("/Volumes/ram", 99, 100, VolumeKind::Temporary)],
            &thresholds,
        );
        assert_eq!(
            report.overall,
            HealthStatus::Unknown,
            "只有临时卷 → 无可评估项"
        );
        assert_eq!(report.reasons[0].code, "metrics_unavailable");
    }

    #[test]
    fn memory_missing_metrics_is_unknown_not_healthy() {
        let report = evaluate_memory(&MemoryMetrics::default(), &Thresholds::default());
        assert_eq!(report.overall, HealthStatus::Unknown);
    }

    #[test]
    fn thresholds_clamp_absurd_config() {
        let clamped = Thresholds {
            disk_warn_ratio: 0.0,
            disk_critical_ratio: 5.0,
            memory_warn_ratio: -1.0,
            cpu_warn_ratio: 0.5,
        }
        .normalized();
        assert!(clamped.disk_warn_ratio >= 0.01 && clamped.disk_critical_ratio <= 1.0);
        assert!(clamped.memory_warn_ratio >= 0.01);
    }
}
