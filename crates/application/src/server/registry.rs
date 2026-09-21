//! 注册表服务（V7 §26-§48）。
//!
//! 注册表来源：组合根注入的描述符清单（settings / 配置文件）。
//! **只管理显式注册的条目**；未注册 → 稳定拒绝码（§35/§154）。

use std::sync::Arc;

use parking_lot::RwLock;

use devtoolbox_core::server::{
    ApplicationDescriptor, ApplicationStatus, HealthStatus, ServiceDescriptor, ServiceStatus,
    is_valid_id,
};

use super::ports::{ApplicationProbePort, ServiceProbePort, server_error};

/// 服务注册表（§26）。
pub struct ServiceRegistryService {
    services: RwLock<Vec<ServiceDescriptor>>,
    probe: Arc<dyn ServiceProbePort>,
}

impl ServiceRegistryService {
    #[must_use]
    pub fn new(services: Vec<ServiceDescriptor>, probe: Arc<dyn ServiceProbePort>) -> Self {
        Self {
            services: RwLock::new(services),
            probe,
        }
    }

    /// 全部已注册服务（顺序稳定，便于 UI 与测试）。
    #[must_use]
    pub fn list(&self) -> Vec<ServiceDescriptor> {
        self.services.read().clone()
    }

    /// 按 id 取注册服务；未注册 → `None`（调用方负责 DENIED）。
    #[must_use]
    pub fn get(&self, service_id: &str) -> Option<ServiceDescriptor> {
        self.services
            .read()
            .iter()
            .find(|service| service.id == service_id)
            .cloned()
    }

    /// 模型入参 → 注册服务（§35：未注册 DENIED）。
    pub fn resolve(&self, service_id: &str) -> Result<ServiceDescriptor, crate::error::ApplicationError> {
        if !is_valid_id(service_id) {
            return Err(server_error("invalid_service_id", "服务 id 不合法"));
        }
        self.get(service_id)
            .ok_or_else(|| server_error("unknown_service", "该服务未在注册表中"))
    }

    /// 探测单个注册服务。
    pub fn status(&self, service_id: &str) -> Result<ServiceStatus, crate::error::ApplicationError> {
        let service = self.resolve(service_id)?;
        Ok(self.probe.probe(&service))
    }

    /// 批量探测（§116：并行由组合根的探活实现负责，此处只做聚合）。
    pub fn status_all(&self) -> Vec<(ServiceDescriptor, ServiceStatus)> {
        self.list()
            .into_iter()
            .map(|service| {
                let status = self.probe.probe(&service);
                (service, status)
            })
            .collect()
    }
}

/// 应用注册表（§42）。
pub struct ApplicationRegistryService {
    apps: RwLock<Vec<ApplicationDescriptor>>,
    probe: Arc<dyn ApplicationProbePort>,
}

impl ApplicationRegistryService {
    #[must_use]
    pub fn new(apps: Vec<ApplicationDescriptor>, probe: Arc<dyn ApplicationProbePort>) -> Self {
        Self {
            apps: RwLock::new(apps),
            probe,
        }
    }

    #[must_use]
    pub fn list(&self) -> Vec<ApplicationDescriptor> {
        self.apps.read().clone()
    }

    #[must_use]
    pub fn get(&self, app_id: &str) -> Option<ApplicationDescriptor> {
        self.apps
            .read()
            .iter()
            .find(|app| app.id == app_id)
            .cloned()
    }

    /// 模型入参 → 注册应用（§48/§154）。
    pub fn resolve(&self, app_id: &str) -> Result<ApplicationDescriptor, crate::error::ApplicationError> {
        if !is_valid_id(app_id) {
            return Err(server_error("invalid_app_id", "应用 id 不合法"));
        }
        self.get(app_id)
            .ok_or_else(|| server_error("unknown_app", "该应用未在注册表中"))
    }

    pub fn status(&self, app_id: &str) -> Result<ApplicationStatus, crate::error::ApplicationError> {
        let app = self.resolve(app_id)?;
        Ok(self.probe.probe(&app))
    }

    pub fn status_all(&self) -> Vec<(ApplicationDescriptor, ApplicationStatus)> {
        self.list()
            .into_iter()
            .map(|app| {
                let status = self.probe.probe(&app);
                (app, status)
            })
            .collect()
    }
}

/// 从配置装载注册表：过滤非法条目（§27/§43 的 `is_valid`）。
#[must_use]
pub fn valid_services(services: Vec<ServiceDescriptor>) -> Vec<ServiceDescriptor> {
    services
        .into_iter()
        .filter(|service| service.is_valid())
        .collect()
}

#[must_use]
pub fn valid_apps(apps: Vec<ApplicationDescriptor>) -> Vec<ApplicationDescriptor> {
    apps.into_iter().filter(|app| app.is_valid()).collect()
}

/// 未知状态构造（探活失败的统一降级，§20）。
#[must_use]
pub fn unknown_status(id: &str, detail: impl Into<String>, now: i64) -> ServiceStatus {
    ServiceStatus {
        service_id: id.to_string(),
        status: HealthStatus::Unknown,
        detail: detail.into(),
        checked_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::server::{HealthCheckKind, ServiceProviderType};

    struct FakeProbe;

    impl ServiceProbePort for FakeProbe {
        fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
            ServiceStatus {
                service_id: service.id.clone(),
                status: HealthStatus::Healthy,
                detail: "fake".into(),
                checked_at: 1,
            }
        }
    }

    impl ApplicationProbePort for FakeProbe {
        fn probe(&self, app: &ApplicationDescriptor) -> ApplicationStatus {
            ApplicationStatus {
                app_id: app.id.clone(),
                status: HealthStatus::Healthy,
                detail: "fake".into(),
                checked_at: 1,
            }
        }
    }

    fn service(id: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: id.into(),
            display_name: "Self Tools".into(),
            provider_ref: "com.example.self-tools".into(),
            provider_type: ServiceProviderType::Launchd,
            health_check: HealthCheckKind::Launchd,
            allowed_actions: vec!["restart".into()],
            ..ServiceDescriptor::default()
        }
    }

    #[test]
    fn registered_service_is_visible_and_probeable() {
        let registry = ServiceRegistryService::new(vec![service("self-tools")], Arc::new(FakeProbe));
        assert_eq!(registry.list().len(), 1);
        let status = registry.status("self-tools").expect("registered");
        assert_eq!(status.status, HealthStatus::Healthy);
    }

    #[test]
    fn unknown_or_invalid_service_is_denied() {
        let registry = ServiceRegistryService::new(vec![service("self-tools")], Arc::new(FakeProbe));
        let unknown = registry.resolve("sshd").expect_err("unregistered");
        match unknown {
            crate::error::ApplicationError::Server { reason, .. } => assert_eq!(reason, "unknown_service"),
            other => panic!("unexpected: {other:?}"),
        }
        // §155：注入形态在 id 校验层就被拒（稳定码，不回显输入）。
        let injected = registry.resolve("foo; rm -rf /").expect_err("injection");
        match injected {
            crate::error::ApplicationError::Server { reason, .. } => {
                assert_eq!(reason, "invalid_service_id")
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn invalid_descriptors_are_filtered_at_load() {
        let services = valid_services(vec![service("ok-one"), service("bad id!")]);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].id, "ok-one");

        let apps = valid_apps(vec![
            devtoolbox_core::server::ApplicationDescriptor {
                id: "app".into(),
                name: "App".into(),
                url: "http://127.0.0.1:8080".into(),
                ..Default::default()
            },
            devtoolbox_core::server::ApplicationDescriptor {
                id: "bad".into(),
                name: "Bad".into(),
                url: "javascript:alert(1)".into(),
                ..Default::default()
            },
        ]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "app");
    }
}
