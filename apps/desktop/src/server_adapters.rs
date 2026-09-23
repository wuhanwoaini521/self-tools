//! Home Server 组合根适配器（V7 Gates 3-7）。
//!
//! 把 infrastructure 的平台实现包装成 application 端口：
//! - `LocalSystemMetrics` → `SystemMetricsProvider`
//! - `LaunchdServiceProbe` / `LaunchdServiceControl` → `ServiceProbePort` / `ServiceControlPort`
//!   （**service_id → launchd label 的映射只发生在这里**，§36）
//! - `LocalLogTail` → `LogTailPort`
//! - HTTP 探活（应用 registry 的 health_url）→ `ApplicationProbePort`
//!
//! 依赖方向不变：desktop 是唯一组合根；`application → infrastructure` = 0。

use parking_lot::Mutex;
use std::collections::HashMap;

use devtoolbox_application::server::action::ServiceControlPort;
use devtoolbox_application::server::ports::LogTailPort;
use devtoolbox_application::server::ports::{
    ApplicationProbePort, ServiceProbePort, SystemMetricsProvider,
};
use devtoolbox_core::server::{
    ApplicationDescriptor, ApplicationStatus, HealthStatus, ServiceDescriptor, ServiceStatus,
    SystemMetrics,
};
use devtoolbox_infrastructure::server::{
    LaunchdServiceControl, LaunchdServiceProbe, LocalLogTail, LocalSystemMetrics,
};

/// 平台指标适配器。
#[derive(Debug, Default)]
pub struct SystemMetricsAdapter {
    inner: LocalSystemMetrics,
}

impl SystemMetricsProvider for SystemMetricsAdapter {
    fn metrics(&self) -> Result<SystemMetrics, String> {
        Ok(self.inner.sample())
    }
}

/// launchd 服务探活适配器（含 service_id → label 映射）。
#[derive(Debug, Default)]
pub struct LaunchdProbeAdapter {
    inner: LaunchdServiceProbe,
    /// service_id → provider_ref（label）。映射表在装配时从注册表快照生成。
    labels: Mutex<HashMap<String, String>>,
}

impl LaunchdProbeAdapter {
    #[must_use]
    pub fn new(services: &[ServiceDescriptor]) -> Self {
        let labels = services
            .iter()
            .map(|service| (service.id.clone(), service.provider_ref.clone()))
            .collect();
        Self {
            inner: LaunchdServiceProbe,
            labels: Mutex::new(labels),
        }
    }
}

impl ServiceProbePort for LaunchdProbeAdapter {
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        // 用注册表里的 provider_ref（不是模型输入）做一次「重解析」，
        // 保证即使调用方传入被篡改的 descriptor，探活仍以注册表为准。
        let label = self
            .labels
            .lock()
            .get(&service.id)
            .cloned()
            .unwrap_or_else(|| service.provider_ref.clone());
        let resolved = ServiceDescriptor {
            provider_ref: label,
            ..service.clone()
        };
        self.inner.probe(&resolved)
    }
}

/// launchd 重启适配器：service_id → label（§36：模型只传 id）。
#[derive(Debug, Default)]
pub struct LaunchdControlAdapter {
    inner: LaunchdServiceControl,
    labels: Mutex<HashMap<String, String>>,
}

impl LaunchdControlAdapter {
    #[must_use]
    pub fn new(services: &[ServiceDescriptor]) -> Self {
        let labels = services
            .iter()
            .map(|service| (service.id.clone(), service.provider_ref.clone()))
            .collect();
        Self {
            inner: LaunchdServiceControl,
            labels: Mutex::new(labels),
        }
    }
}

impl ServiceControlPort for LaunchdControlAdapter {
    fn restart(&self, service_id: &str) -> Result<(), String> {
        // 未注册的 id 在这里没有 label 可映射 → 拒绝（纵深防御；
        // 上层 registry 已在 plan 阶段拦过一次）。
        let label = self
            .labels
            .lock()
            .get(service_id)
            .cloned()
            .ok_or_else(|| "unmapped_service_id".to_string())?;
        self.inner.restart(&label)
    }
}

/// 本地日志读取适配器。
#[derive(Debug, Default)]
pub struct LogTailAdapter {
    inner: LocalLogTail,
}

impl LogTailPort for LogTailAdapter {
    fn tail(
        &self,
        service: &ServiceDescriptor,
        log_source_id: &str,
        max_lines: usize,
        max_bytes: usize,
        max_age_secs: u64,
    ) -> Result<devtoolbox_core::server::LogReadResult, String> {
        self.inner
            .tail(service, log_source_id, max_lines, max_bytes, max_age_secs)
    }
}

/// 应用探活适配器：只访问注册表内的 URL（§49/§50 SSRF 边界）。
#[derive(Debug, Default)]
pub struct HttpAppProbeAdapter {
    client: reqwest::Client,
}

impl HttpAppProbeAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl ApplicationProbePort for HttpAppProbeAdapter {
    fn probe(&self, app: &ApplicationDescriptor) -> ApplicationStatus {
        let checked_at = unix_now();
        let Some(url) = app.health_url.clone() else {
            return ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Unknown,
                detail: "未配置 health_url".into(),
                checked_at,
            };
        };
        if !devtoolbox_core::server::is_http_url(&url) {
            return ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Unknown,
                detail: "health_url 不在 http/https 白名单".into(),
                checked_at,
            };
        }
        // 同步探活：桌面端一次一个，超时由 client 控制（§115 失败隔离）。
        // 复用 Tauri 的 async runtime，不为探活再引入 tokio 依赖。
        let outcome = tauri::async_runtime::block_on(self.client.get(&url).send()).ok();
        match outcome {
            Some(response) if response.status().is_success() => ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Healthy,
                detail: format!("HTTP {}", response.status().as_u16()),
                checked_at,
            },
            Some(response) => ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Unhealthy,
                detail: format!("HTTP {}", response.status().as_u16()),
                checked_at,
            },
            None => ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Unknown,
                detail: "health 请求失败".into(),
                checked_at,
            },
        }
    }
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
    use devtoolbox_core::server::ServiceProviderType;

    fn service(id: &str, label: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: id.into(),
            display_name: "Self Tools".into(),
            provider_ref: label.into(),
            provider_type: ServiceProviderType::Launchd,
            ..ServiceDescriptor::default()
        }
    }

    #[test]
    fn control_adapter_maps_service_id_to_registered_label() {
        let adapter = LaunchdControlAdapter::new(&[service("self-tools", "com.example.backend")]);
        // 未注册 id → 无 label 可映射 → 拒绝。
        let error = adapter.restart("sshd").expect_err("unmapped");
        assert_eq!(error, "unmapped_service_id");
    }

    #[test]
    fn probe_adapter_uses_registry_label_not_caller_input() {
        let adapter = LaunchdProbeAdapter::new(&[service("self-tools", "com.example.backend")]);
        // 调用方传入被篡改的 provider_ref：适配器仍以注册表 label 为准。
        let tampered = ServiceDescriptor {
            provider_ref: "com.evil.other".into(),
            ..service("self-tools", "com.example.backend")
        };
        let status = adapter.probe(&tampered);
        // 只断言被篡改的引用不会改变目标：service_id 来自注册表。
        assert_eq!(status.service_id, "self-tools");
        // 平台差异：本机有 launchd（macOS runner）时可能真的探到服务状态，
        // 没有时是 Unknown。两者都不算「谎报」；detail 不含调用方输入。
        assert!(!status.detail.contains("com.evil.other"));
    }

    #[test]
    fn metrics_adapter_samples_without_panicking() {
        let metrics = SystemMetricsAdapter::default().metrics().expect("metrics");
        assert!(!metrics.architecture.is_empty());
    }
}
