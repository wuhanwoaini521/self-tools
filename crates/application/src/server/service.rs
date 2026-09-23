//! 服务器状态服务（V7 §18-§22）。
//!
//! `get_status` 产 compact summary（§19）；健康评估用 core 纯函数
//! （阈值配置化，§22）。provider 失败 → `Unknown` / `Degraded`，**不 panic**
//! （Gate 2 Case B）。

use std::sync::Arc;

use devtoolbox_core::server::{
    ApplicationStatus, HealthReason, HealthReport, HealthStatus, ServiceDescriptor, ServiceStatus,
    SystemMetrics, Thresholds, evaluate_memory, evaluate_storage,
};

use super::ports::{ApplicationProbePort, ServiceProbePort, SystemMetricsProvider, server_error};
use super::registry::ApplicationRegistryService;
use super::registry::ServiceRegistryService;

/// 服务器配置（§22 阈值 + §117 缓存窗口）。
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub thresholds: Thresholds,
    /// 健康状态缓存秒数（0 = 不缓存）。
    pub health_cache_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            thresholds: Thresholds::default(),
            health_cache_secs: 5,
        }
    }
}

/// 紧凑的服务器状态（§19：UI 与 AI 共用同一形状）。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServerStatus {
    pub hostname: String,
    pub platform: String,
    pub os_version: String,
    pub uptime_secs: u64,
    pub cpu_usage_ratio: Option<f32>,
    pub cpu_cores: u32,
    pub memory_usage_ratio: Option<f32>,
    pub memory_total_bytes: u64,
    /// 使用率最高的可见卷（§19 storage warning）。
    pub tightest_volume: Option<String>,
    pub tightest_volume_ratio: Option<f32>,
    pub health: HealthReport,
    pub services: Vec<ServiceStatus>,
    pub apps: Vec<ApplicationStatus>,
}

impl ServerStatus {
    /// 服务告警数（§19 service warning）。
    #[must_use]
    pub fn service_warnings(&self) -> usize {
        self.services
            .iter()
            .filter(|status| status.status != HealthStatus::Healthy)
            .count()
    }

    /// 应用告警数。
    #[must_use]
    pub fn app_warnings(&self) -> usize {
        self.apps
            .iter()
            .filter(|status| status.status != HealthStatus::Healthy)
            .count()
    }
}

/// 服务器服务（§18：Read 面）。
pub struct ServerService {
    metrics: Arc<dyn SystemMetricsProvider>,
    services: Arc<ServiceRegistryService>,
    apps: Arc<ApplicationRegistryService>,
    config: ServerConfig,
}

impl ServerService {
    #[must_use]
    pub fn new(
        metrics: Arc<dyn SystemMetricsProvider>,
        services: Arc<ServiceRegistryService>,
        apps: Arc<ApplicationRegistryService>,
        config: ServerConfig,
    ) -> Self {
        Self {
            metrics,
            services,
            apps,
            config,
        }
    }

    /// 系统指标（§15/§16）；provider 失败 → 受控错误（不 panic）。
    pub fn metrics(&self) -> Result<SystemMetrics, crate::error::ApplicationError> {
        self.metrics
            .metrics()
            .map_err(|message| server_error("metrics_unavailable", message))
    }

    /// 存储卷（§17：默认只返回用户可见卷）。
    pub fn storage(
        &self,
    ) -> Result<Vec<devtoolbox_core::server::StorageMetrics>, crate::error::ApplicationError> {
        Ok(self
            .metrics()?
            .visible_storage()
            .into_iter()
            .cloned()
            .collect())
    }

    /// CPU / 内存明细（§18：可合并为 status 的一部分，工具单独暴露）。
    pub fn cpu(
        &self,
    ) -> Result<devtoolbox_core::server::CpuMetrics, crate::error::ApplicationError> {
        Ok(self.metrics()?.cpu)
    }

    pub fn memory(
        &self,
    ) -> Result<devtoolbox_core::server::MemoryMetrics, crate::error::ApplicationError> {
        Ok(self.metrics()?.memory)
    }

    /// 健康报告（§20/§21：四态 + 可解释原因）。
    pub fn health(&self) -> Result<HealthReport, crate::error::ApplicationError> {
        let metrics = self.metrics()?;
        Ok(self.evaluate_health(&metrics))
    }

    /// 纯评估（可单测；不触 provider）。
    #[must_use]
    pub fn evaluate_health(&self, metrics: &SystemMetrics) -> HealthReport {
        let thresholds = self.config.thresholds.normalized();
        if metrics.is_unknown() {
            return HealthReport::unknown(HealthReason::new(
                "metrics_unavailable",
                "系统指标采样不可用",
            ));
        }
        let mut report = evaluate_storage(&metrics.storage, &thresholds);
        report = report.merge(&evaluate_memory(&metrics.memory, &thresholds));
        if let Some(ratio) = metrics.cpu.usage_ratio
            && ratio >= thresholds.cpu_warn_ratio
        {
            report = report.merge(&HealthReport {
                overall: HealthStatus::Degraded,
                reasons: vec![HealthReason::new(
                    "cpu_usage",
                    format!("CPU 使用率 {:.0}% 超过警告阈值", ratio * 100.0),
                )],
            });
        }
        for (service, status) in self.services.status_all() {
            if status.status == HealthStatus::Unhealthy {
                report = report.merge(&HealthReport {
                    overall: HealthStatus::Degraded,
                    reasons: vec![HealthReason::new(
                        "service_unavailable",
                        format!("服务 {} 状态异常", service.id),
                    )],
                });
            }
        }
        report
    }

    /// compact summary（§19）。
    pub fn status(&self) -> Result<ServerStatus, crate::error::ApplicationError> {
        let metrics = self.metrics()?;
        let health = self.evaluate_health(&metrics);
        let services = self
            .services
            .status_all()
            .into_iter()
            .map(|(_, status)| status)
            .collect();
        let apps = self
            .apps
            .status_all()
            .into_iter()
            .map(|(_, status)| status)
            .collect();
        let tightest = metrics.tightest_storage();
        Ok(ServerStatus {
            hostname: metrics.hostname.clone(),
            platform: metrics.platform.as_str().to_string(),
            os_version: metrics.os_version.clone(),
            uptime_secs: metrics.uptime_secs,
            cpu_usage_ratio: metrics.cpu.usage_ratio,
            cpu_cores: metrics.cpu.logical_cores,
            memory_usage_ratio: metrics.memory.usage_ratio(),
            memory_total_bytes: metrics.memory.total_bytes,
            tightest_volume: tightest.map(|volume| volume.mount.clone()),
            tightest_volume_ratio: tightest.and_then(|volume| volume.usage_ratio()),
            health,
            services,
            apps,
        })
    }

    /// 注册服务 / 应用清单（工具与 UI 共用）。
    #[must_use]
    pub fn registered_services(&self) -> Vec<ServiceDescriptor> {
        self.services.list()
    }

    #[must_use]
    pub fn registered_apps(&self) -> Vec<devtoolbox_core::server::ApplicationDescriptor> {
        self.apps.list()
    }
}

/// 探活实现：把「任意 target 的健康探测」收敛为注册表驱动（§50：SSRF 边界）。
pub struct NoopProbe;

impl SystemMetricsProvider for NoopProbe {
    fn metrics(&self) -> Result<SystemMetrics, String> {
        Err("metrics provider not configured".to_string())
    }
}

impl ServiceProbePort for NoopProbe {
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        ServiceStatus {
            service_id: service.id.clone(),
            status: HealthStatus::Unknown,
            detail: "probe not configured".into(),
            checked_at: 0,
        }
    }
}

impl ApplicationProbePort for NoopProbe {
    fn probe(&self, app: &devtoolbox_core::server::ApplicationDescriptor) -> ApplicationStatus {
        ApplicationStatus {
            app_id: app.id.clone(),
            status: HealthStatus::Unknown,
            detail: "probe not configured".into(),
            checked_at: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::server::{MemoryMetrics, StorageMetrics, VolumeKind};

    struct FakeMetrics {
        metrics: SystemMetrics,
        fail: bool,
    }

    impl SystemMetricsProvider for FakeMetrics {
        fn metrics(&self) -> Result<SystemMetrics, String> {
            if self.fail {
                return Err("sampler crashed".into());
            }
            Ok(self.metrics.clone())
        }
    }

    fn volume(mount: &str, used: u64, total: u64, kind: VolumeKind) -> StorageMetrics {
        StorageMetrics {
            mount: mount.into(),
            used_bytes: used,
            total_bytes: total,
            available_bytes: total - used,
            kind,
        }
    }

    fn service_with(
        metrics: SystemMetrics,
        services: Vec<ServiceDescriptor>,
        apps: Vec<devtoolbox_core::server::ApplicationDescriptor>,
        fail: bool,
    ) -> ServerService {
        ServerService::new(
            Arc::new(FakeMetrics { metrics, fail }),
            Arc::new(ServiceRegistryService::new(services, Arc::new(NoopProbe))),
            Arc::new(ApplicationRegistryService::new(apps, Arc::new(NoopProbe))),
            ServerConfig::default(),
        )
    }

    fn healthy_metrics() -> SystemMetrics {
        SystemMetrics {
            hostname: "mac-studio".into(),
            platform: devtoolbox_core::server::Platform::MacOs,
            os_version: "12.7".into(),
            architecture: "arm64".into(),
            uptime_secs: 1_209_600,
            cpu: devtoolbox_core::server::CpuMetrics {
                usage_ratio: Some(0.18),
                logical_cores: 10,
                load_average: Some((1.2, 1.5, 1.4)),
            },
            memory: MemoryMetrics {
                total_bytes: 32 << 30,
                used_bytes: 13 << 30,
                available_bytes: Some(19 << 30),
            },
            storage: vec![volume("/", 60 << 30, 100 << 30, VolumeKind::SystemDisk)],
            sampled_at: 1_700_000_000,
        }
    }

    #[test]
    fn status_reports_compact_summary() {
        let service = service_with(healthy_metrics(), vec![], vec![], false);
        let status = service.status().expect("status");
        assert_eq!(status.hostname, "mac-studio");
        assert_eq!(status.platform, "macos");
        assert_eq!(status.cpu_usage_ratio, Some(0.18));
        assert_eq!(status.memory_usage_ratio, Some(0.40625));
        assert_eq!(status.tightest_volume.as_deref(), Some("/"));
        assert_eq!(status.health.overall, HealthStatus::Healthy);
        assert_eq!(status.service_warnings(), 0);
    }

    #[test]
    fn provider_failure_degrades_without_panicking() {
        let service = service_with(SystemMetrics::default(), vec![], vec![], true);
        let error = service
            .status()
            .expect_err("provider failure → controlled error");
        match error {
            crate::error::ApplicationError::Server { reason, .. } => {
                assert_eq!(reason, "metrics_unavailable")
            }
            other => panic!("unexpected: {other:?}"),
        }
        // 全未知指标 → Unknown 而非 Healthy（§8 fail-closed）。
        let unknown = service_with(SystemMetrics::default(), vec![], vec![], false);
        assert_eq!(
            unknown.health().expect("health").overall,
            HealthStatus::Unknown
        );
    }

    #[test]
    fn disk_threshold_produces_degraded_with_reason() {
        let mut metrics = healthy_metrics();
        metrics.storage = vec![volume("/", 85 << 30, 100 << 30, VolumeKind::SystemDisk)];
        let service = service_with(metrics, vec![], vec![], false);
        let health = service.health().expect("health");
        assert_eq!(health.overall, HealthStatus::Degraded);
        assert_eq!(health.reasons[0].code, "disk_usage");
        assert!(health.reasons[0].detail.contains('/'), "原因须指明卷");
    }

    #[test]
    fn cpu_threshold_degrades() {
        let mut metrics = healthy_metrics();
        metrics.cpu.usage_ratio = Some(0.95);
        let service = service_with(metrics, vec![], vec![], false);
        let health = service.health().expect("health");
        assert_eq!(health.overall, HealthStatus::Degraded);
        assert!(
            health
                .reasons
                .iter()
                .any(|reason| reason.code == "cpu_usage")
        );
    }

    #[test]
    fn storage_hides_temporary_volumes() {
        let mut metrics = healthy_metrics();
        metrics
            .storage
            .push(volume("/Volumes/ram", 99, 100, VolumeKind::Temporary));
        let service = service_with(metrics, vec![], vec![], false);
        let storage = service.storage().expect("storage");
        assert_eq!(storage.len(), 1, "临时卷默认隐藏");
        assert_eq!(storage[0].mount, "/");
    }
}
