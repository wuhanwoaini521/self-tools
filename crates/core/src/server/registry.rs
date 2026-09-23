//! 注册表契约（V7 §26-§48）。
//!
//! `ServiceRegistry` / `ApplicationRegistry` 都是**显式注册**（§26/§42）：
//! 未注册的目标一律 DENIED（§35/§154）。模型可见面只有稳定 `id`；
//! `provider_ref`（launchd label / container name）由 infrastructure 映射（§36）。

use serde::{Deserialize, Serialize};

use super::health::HealthStatus;
use super::is_valid_id;

/// 服务提供方类型（§28）。V7 实现 `Launchd` 与 `Http`；`Docker` 为 P1。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceProviderType {
    /// macOS launchd（`provider_ref` = label）。
    #[default]
    Launchd,
    /// HTTP 探活（`provider_ref` = 注册表内的 health URL id）。
    Http,
    /// 本地进程（P1）。
    Process,
    /// Docker 容器（P1，且禁止 `docker exec`）。
    Docker,
}

impl ServiceProviderType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceProviderType::Launchd => "launchd",
            ServiceProviderType::Http => "http",
            ServiceProviderType::Process => "process",
            ServiceProviderType::Docker => "docker",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "launchd" => Some(ServiceProviderType::Launchd),
            "http" => Some(ServiceProviderType::Http),
            "process" => Some(ServiceProviderType::Process),
            "docker" => Some(ServiceProviderType::Docker),
            _ => None,
        }
    }
}

/// 日志来源（§37-§39）：只能来自 descriptor 的注册路径。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogSource {
    /// 稳定 id（模型不可见；用于审计与 UI）。
    pub id: String,
    /// 展示名。
    pub display_name: String,
    /// 平台侧路径（infrastructure 解析；**不是**模型入参）。
    pub path: String,
}

/// 健康检查方式（§49/§50：URL 必须来自注册表，禁止模型提供）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[derive(Default)]
pub enum HealthCheckKind {
    /// 不探活。
    #[default]
    None,
    /// launchd 状态（PID 存在即活）。
    Launchd,
    /// HTTP GET；`url` 来自注册表（SSRF 边界）。
    Http { url: String },
}

/// 服务描述符（§27）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub provider_type: ServiceProviderType,
    /// 平台侧引用（launchd label / container name / URL id）——**永不出现在工具入参**。
    pub provider_ref: String,
    pub health_check: HealthCheckKind,
    /// 日志来源（空 = 无注册日志）。
    #[serde(default)]
    pub log_sources: Vec<LogSource>,
    /// 显式允许的操作（如 `["restart"]`）；未声明 → 操作 DENIED（§35）。
    #[serde(default)]
    pub allowed_actions: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ServiceDescriptor {
    /// 是否允许某操作（§35：白名单，缺省拒绝）。
    #[must_use]
    pub fn allows(&self, action: &str) -> bool {
        self.allowed_actions.iter().any(|allowed| allowed == action)
    }

    /// 注册表合法性（id 形态 + 必填字段）。
    #[must_use]
    pub fn is_valid(&self) -> bool {
        is_valid_id(&self.id)
            && !self.display_name.trim().is_empty()
            && !self.provider_ref.trim().is_empty()
    }
}

/// 应用描述符（§43）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplicationDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 注册 URL（§47：只允许 http/https）。
    pub url: String,
    /// 可选健康检查 URL（§49/§50）。
    #[serde(default)]
    pub health_url: Option<String>,
    /// 关联服务 id（可选；用于状态聚合）。
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ApplicationDescriptor {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        is_valid_id(&self.id)
            && !self.name.trim().is_empty()
            && is_http_url(&self.url)
            && self.health_url.as_deref().is_none_or(is_http_url)
            && self.service_id.as_deref().is_none_or(is_valid_id)
    }
}

/// URL scheme 白名单（§47）。
///
/// 只放行 `http` / `https`；`javascript:` / `file:` / `data:` / `shell:` 等
/// 一律拒绝（前端 `openPath` / 新标签页打开都基于此）。
#[must_use]
pub fn is_http_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((scheme, _)) = trimmed.split_once(':') else {
        return false;
    };
    matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
}

/// 运行时健康快照（registry + 探测结果的合并视图）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub service_id: String,
    pub status: HealthStatus,
    /// 人类可读状态（launchd 原始状态 / HTTP code）。
    pub detail: String,
    /// 最近一次探测时间（Unix 秒）。
    pub checked_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApplicationStatus {
    pub app_id: String,
    pub status: HealthStatus,
    pub detail: String,
    pub checked_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(id: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: id.into(),
            display_name: "Self Tools".into(),
            provider_ref: "com.example.self-tools".into(),
            allowed_actions: vec!["restart".into()],
            ..ServiceDescriptor::default()
        }
    }

    #[test]
    fn service_allows_only_registered_actions() {
        let descriptor = service("self-tools");
        assert!(descriptor.allows("restart"));
        assert!(!descriptor.allows("stop"), "未声明 → 拒绝");
        assert!(!descriptor.allows("exec"));
        let bare = ServiceDescriptor {
            allowed_actions: Vec::new(),
            ..service("bare")
        };
        assert!(!bare.allows("restart"), "空白名单 → 全部拒绝");
    }

    #[test]
    fn service_validation_rejects_bad_ids_and_empty_refs() {
        assert!(service("self-tools").is_valid());
        assert!(!service("foo; rm -rf /").is_valid());
        assert!(
            !ServiceDescriptor {
                provider_ref: "  ".into(),
                ..service("ok-id")
            }
            .is_valid()
        );
    }

    #[test]
    fn app_urls_require_http_scheme() {
        let app = |url: &str| ApplicationDescriptor {
            id: "app".into(),
            name: "App".into(),
            url: url.into(),
            ..ApplicationDescriptor::default()
        };
        assert!(app("http://127.0.0.1:8080").is_valid());
        assert!(app("https://home.example.com/app").is_valid());
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "shell:open -a Terminal",
            "",
            "not a url",
        ] {
            assert!(!app(bad).is_valid(), "应拒绝: {bad:?}");
        }
        assert!(
            !ApplicationDescriptor {
                health_url: Some("file:///x".into()),
                ..app("http://ok")
            }
            .is_valid(),
            "health_url 同样走白名单"
        );
    }

    #[test]
    fn provider_type_round_trips() {
        assert_eq!(
            ServiceProviderType::parse("launchd"),
            Some(ServiceProviderType::Launchd)
        );
        assert_eq!(
            ServiceProviderType::parse("docker"),
            Some(ServiceProviderType::Docker)
        );
        assert_eq!(ServiceProviderType::parse("k8s"), None);
    }
}
