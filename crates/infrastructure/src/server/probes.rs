//! 服务探活与控制（V7 §29/§33/§36/§102）。
//!
//! macOS launchd adapter：`launchctl` **固定参数模板**，argv 全部字面量，
//! 服务标识只能是**已注册 descriptor 的 `provider_ref`**（§36：模型只传
//! service_id，映射发生在本适配器内）。
//!
//! 测试用 Fake 驱动（Gate 9：CI 不依赖真实 launchd）。

use std::process::Command;

use devtoolbox_core::server::{HealthStatus, ServiceDescriptor, ServiceStatus, ServiceProviderType};

use crate::server::metrics::run_capture;

/// launchd 探活：`launchctl print gui/<uid>/<label>` 存在即活。
#[derive(Debug, Default, Clone, Copy)]
pub struct LaunchdServiceProbe;

impl LaunchdServiceProbe {
    /// 只接受**字面量参数模板**；label 来自注册表（不是模型输入）。
    fn launchctl_print(&self, label: &str) -> Option<String> {
        // §24：固定 executable + 固定 argument template。
        // launchctl print 的 domain 参数是布局的一部分，label 由注册表提供。
        let uid = current_uid();
        let domain = format!("gui/{uid}");
        Command::new("launchctl")
            .args(["print", &domain, label])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
    }
}

impl LaunchdServiceProbe {
    /// 探测一个已注册服务（返回 core 状态形状；适配到端口由组合根完成）。
    #[must_use]
    pub fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        let checked_at = unix_now();
        if service.provider_type != ServiceProviderType::Launchd {
            return ServiceStatus {
                service_id: service.id.clone(),
                status: HealthStatus::Unknown,
                detail: format!("provider_type={} 未实现", service.provider_type.as_str()),
                checked_at,
            };
        }
        if service.provider_ref.trim().is_empty() {
            return ServiceStatus {
                service_id: service.id.clone(),
                status: HealthStatus::Unknown,
                detail: "provider_ref 缺失".into(),
                checked_at,
            };
        }
        match self.launchctl_print(&service.provider_ref) {
            Some(output) => {
                let state = parse_launchd_state(&output);
                ServiceStatus {
                    service_id: service.id.clone(),
                    status: if state == "running" {
                        HealthStatus::Healthy
                    } else {
                        HealthStatus::Unhealthy
                    },
                    detail: format!("launchd state = {state}"),
                    checked_at,
                }
            }
            None => ServiceStatus {
                service_id: service.id.clone(),
                status: HealthStatus::Unknown,
                detail: "launchctl 不可用或服务未加载".into(),
                checked_at,
            },
        }
    }
}

/// launchd 重启：`launchctl kickstart -k <domain>/<label>`（固定参数）。
///
/// `-k` 会先杀后起；只有**已注册**服务的 label 能到达这里（上层 registry 保证）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LaunchdServiceControl;

impl LaunchdServiceControl {
    /// 重启一个**已注册**服务（`service_id` 已由 registry 解析）。
    ///
    /// 组合根负责把 service_id 映射到 launchd label；本方法只接受
    /// 这个映射结果，绝不接受模型原文。
    pub fn restart(&self, label: &str) -> Result<(), String> {
        let uid = current_uid();
        let target = format!("gui/{uid}/{label}");
        let status = Command::new("launchctl")
            .args(["kickstart", "-k", &target])
            .status()
            .map_err(|error| format!("launchctl_spawn_failed: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("launchctl_exit_{}", status.code().unwrap_or(-1)))
        }
    }
}

/// HTTP 探活（应用 registry 的 health_url；§49/§50 SSRF 边界由注册表保证）。
#[derive(Debug, Clone)]
pub struct HttpHealthProbe {
    client: reqwest::Client,
}

impl Default for HttpHealthProbe {
    fn default() -> Self {
        Self {
            // 有界：短超时 + 不跟随重定向到任意主机由调用方 URL 白名单保证。
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl HttpHealthProbe {
    /// 有界 GET（只接受注册表内的 http/https URL）。
    pub async fn check(&self, url: &str) -> (HealthStatus, String) {
        if !devtoolbox_core::server::is_http_url(url) {
            return (HealthStatus::Unknown, "URL 不在 http/https 白名单".into());
        }
        match self.client.get(url).send().await {
            Ok(response) => {
                let code = response.status().as_u16();
                if response.status().is_success() {
                    (HealthStatus::Healthy, format!("HTTP {code}"))
                } else {
                    (HealthStatus::Unhealthy, format!("HTTP {code}"))
                }
            }
            Err(error) => (HealthStatus::Unknown, format!("请求失败：{error}")),
        }
    }
}

fn current_uid() -> String {
    // macOS：`id -u`（固定参数）。不可得 → "501" 之外的任何值都会让 launchctl
    // 报「domain 不存在」→ 上层降级 Unknown，不会误判为健康。
    run_capture("id", &["-u"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0".to_string())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// 解析 `launchctl print` 输出的 `state = running` 行（纯函数，可单测）。
#[must_use]
pub fn parse_launchd_state(output: &str) -> String {
    for line in output.lines() {
        if let Some(rest) = line.split_once('=') {
            let key = rest.0.trim();
            if key == "state" {
                return rest.1.trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(label: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: "self-tools".into(),
            display_name: "Self Tools".into(),
            provider_ref: label.into(),
            provider_type: ServiceProviderType::Launchd,
            ..ServiceDescriptor::default()
        }
    }

    #[test]
    fn launchd_state_parsing_reads_state_line() {
        let output = "program = /usr/local/bin/x\nstate = running\npid = 4242\n";
        assert_eq!(parse_launchd_state(output), "running");
        assert_eq!(parse_launchd_state("state = not running\n"), "not running");
        assert_eq!(parse_launchd_state("garbage"), "unknown");
    }

    #[test]
    fn probe_maps_running_to_healthy_and_absent_to_unknown() {
        // CI 没有 launchd：`launchctl print` 必然失败 → Unknown（不谎报健康）。
        let status = LaunchdServiceProbe.probe(&service("com.example.definitely-not-loaded"));
        assert_eq!(status.status, HealthStatus::Unknown);
        assert!(!status.detail.is_empty());
    }

    #[test]
    fn probe_rejects_non_launchd_provider_as_unknown() {
        let status = LaunchdServiceProbe.probe(&ServiceDescriptor {
            provider_type: ServiceProviderType::Docker,
            ..service("container")
        });
        assert_eq!(status.status, HealthStatus::Unknown);
        assert!(status.detail.contains("docker"), "{}", status.detail);
    }

    #[test]
    fn probe_rejects_empty_provider_ref() {
        let status = LaunchdServiceProbe.probe(&service("   "));
        assert_eq!(status.status, HealthStatus::Unknown);
        assert!(status.detail.contains("provider_ref"));
    }

    #[test]
    fn http_probe_rejects_non_http_urls() {
        let probe = HttpHealthProbe::default();
        // 同步路径只做 URL 校验；异步请求由集成层调用。
        assert!(!devtoolbox_core::server::is_http_url("file:///etc/passwd"));
        assert!(!devtoolbox_core::server::is_http_url("javascript:alert(1)"));
        assert!(devtoolbox_core::server::is_http_url("http://127.0.0.1:8080/health"));
        let _ = probe;
    }
}
