//! 启动配置校验（V11 §51）：非法配置 → **拒绝启动**，绝不静默降级。
//!
//! 校验项（Goal §51）：
//! - invalid port（非 1..=65535 或无法解析）
//! - unsafe bind（生产模式绑定 0.0.0.0 但未显式允许远程暴露）
//! - invalid path（数据/配置目录不可写）
//! - negative limit（负数上限）
//! - bad threshold（阈值越界）
//! - remote MCP without auth（远程开启但无身份提供者）
//! - missing required dirs（必需目录缺失）
//!
//! **Safe Defaults（§52）**：远程 MCP OFF、SYSTEM 确认 ON、文件任意访问不可能、
//! Multi-Agent 有界、Decision/Jev 失败回落开、远程写 fail-closed。

use std::path::Path;

use crate::operations::DeployMode;
use crate::settings::{AppSettings, McpSettings};

/// 单个校验失败（稳定错误码 + 人类可读说明；不含 secret）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigViolation {
    pub code: &'static str,
    pub detail: String,
}

impl ConfigViolation {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// 校验结果：Ok = 可启动；Err = 必须修复后才能启动。
pub type ConfigReport = Result<(), Vec<ConfigViolation>>;

/// 服务配置（bind + 部署模式 + 路径）。
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    /// 绑定地址（`host:port`）。
    pub bind: String,
    /// 是否显式允许非 loopback 暴露（`SELF_TOOLS_ALLOW_REMOTE=1`）。
    pub allow_remote: bool,
    /// 部署模式。
    pub mode: DeployMode,
    /// 数据目录（必须可写）。
    pub data_dir: std::path::PathBuf,
    /// 配置目录（必须可写）。
    pub config_dir: std::path::PathBuf,
    /// 是否已装配身份提供者（远程 MCP 前置条件）。
    pub identity_configured: bool,
}

/// 校验绑定地址：必须是合法 `host:port`；
/// 生产模式非 loopback 需显式允许（§52 fail-closed）。
fn validate_bind(config: &ServiceConfig) -> Vec<ConfigViolation> {
    let mut violations = Vec::new();
    let raw = config.bind.trim();
    if raw.is_empty() {
        violations.push(ConfigViolation::new("invalid_bind", "bind 为空"));
        return violations;
    }
    let Some((host, port)) = raw.rsplit_once(':') else {
        violations.push(ConfigViolation::new(
            "invalid_bind",
            format!("bind 必须是 host:port：{raw}"),
        ));
        return violations;
    };
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        violations.push(ConfigViolation::new("invalid_bind", "bind host 为空"));
    }
    match port.trim().parse::<u16>() {
        Ok(0) => violations.push(ConfigViolation::new("invalid_port", "端口 0 无效")),
        Ok(_) => {}
        Err(_) => violations.push(ConfigViolation::new(
            "invalid_port",
            format!("端口非法（须 1..=65535）：{port}"),
        )),
    }
    // 非 loopback 绑定必须在生产模式显式允许（§52 fail-closed）。
    let loopback = host == "localhost"
        || host == "127.0.0.1"
        || host.starts_with("127.")
        || host == "::1"
        || host == "[::1]";
    if !loopback && config.mode == DeployMode::Production && !config.allow_remote {
        violations.push(ConfigViolation::new(
            "unsafe_bind",
            "生产模式绑定非 loopback 需要 SELF_TOOLS_ALLOW_REMOTE=1（默认 fail-closed）",
        ));
    }
    violations
}

/// 校验目录：存在或可创建，且可写（创建探针文件）。
fn validate_writable_dir(path: &Path, code: &'static str) -> Option<ConfigViolation> {
    if let Err(error) = std::fs::create_dir_all(path) {
        return Some(ConfigViolation::new(
            code,
            format!("目录不可创建 {}：{error}", path.display()),
        ));
    }
    let probe = path.join(".write-probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(error) => Some(ConfigViolation::new(
            code,
            format!("目录不可写 {}：{error}", path.display()),
        )),
    }
}

/// 校验 MCP 设置：远程开启必须有身份提供者（§52）。
fn validate_mcp(mcp: &McpSettings, identity_configured: bool) -> Vec<ConfigViolation> {
    let mut violations = Vec::new();
    if mcp.remote_enabled && !identity_configured {
        violations.push(ConfigViolation::new(
            "remote_mcp_without_auth",
            "MCP 远程访问已开启但未配置身份提供者（拒绝启动：fail-closed）",
        ));
    }
    if mcp.http_enabled {
        let bind = mcp.bind.trim();
        let loopback = bind.starts_with("127.") || bind == "localhost" || bind == "::1" || bind == "[::1]";
        if !loopback && mcp.remote_enabled && !identity_configured {
            violations.push(ConfigViolation::new(
                "mcp_bind_unsafe",
                format!("MCP HTTP 绑定非 loopback（{bind}）且无身份提供者"),
            ));
        }
        if mcp.port == 0 {
            violations.push(ConfigViolation::new("invalid_port", "MCP HTTP 端口 0 无效"));
        }
    }
    violations
}

/// 校验数值设置：负上限、越界阈值。
fn validate_limits(settings: &AppSettings) -> Vec<ConfigViolation> {
    let mut violations = Vec::new();
    let knowledge = &settings.knowledge;
    if knowledge.max_indexed_files == 0 {
        violations.push(ConfigViolation::new(
            "negative_limit",
            "knowledge.max_indexed_files 为 0（无意义；至少 1）",
        ));
    }
    if knowledge.max_document_bytes == 0 {
        violations.push(ConfigViolation::new(
            "negative_limit",
            "knowledge.max_document_bytes 为 0",
        ));
    }
    if knowledge.max_read_chars == 0 {
        violations.push(ConfigViolation::new(
            "negative_limit",
            "knowledge.max_read_chars 为 0",
        ));
    }
    let server = &settings.server;
    if server.confirmation_ttl_secs < 30 || server.confirmation_ttl_secs > 120 {
        violations.push(ConfigViolation::new(
            "bad_threshold",
            format!(
                "server.confirmation_ttl_secs={} 越界（须 30..=120）",
                server.confirmation_ttl_secs
            ),
        ));
    }
    if server.cooldown_secs < 0 || server.max_system_per_session == 0 {
        violations.push(ConfigViolation::new(
            "bad_threshold",
            "server.cooldown_secs/max_system_per_session 越界",
        ));
    }
    // 阈值比率必须在 0..=1。
    let thresholds = &server.thresholds;
    for (name, value) in [
        ("cpu_warn_ratio", thresholds.cpu_warn_ratio),
        ("memory_warn_ratio", thresholds.memory_warn_ratio),
        ("disk_warn_ratio", thresholds.disk_warn_ratio),
        ("disk_critical_ratio", thresholds.disk_critical_ratio),
    ] {
        if !(0.0..=1.0).contains(&value) {
            violations.push(ConfigViolation::new(
                "bad_threshold",
                format!("server.thresholds.{name}={value} 越界（须 0..=1）"),
            ));
        }
    }
    // 决策层：worker 上限必须有界；Jev 超时有界。
    let decision = &settings.decision;
    if decision.max_workers == 0 {
        violations.push(ConfigViolation::new(
            "negative_limit",
            "decision.max_workers 为 0（Multi-Agent 必须有界）",
        ));
    }
    if decision.max_workers > 32 {
        violations.push(ConfigViolation::new(
            "negative_limit",
            format!("decision.max_workers={} 超过硬上限 32", decision.max_workers),
        ));
    }
    if decision.jev_timeout_secs == 0 || decision.jev_timeout_secs > 60 {
        violations.push(ConfigViolation::new(
            "bad_threshold",
            format!(
                "decision.jev_timeout_secs={} 越界（须 1..=60）",
                decision.jev_timeout_secs
            ),
        ));
    }
    // 决策模式字符串必须可解析（未配置 Jev key 时的 active 会被 effective_mode 自动降级）。
    if crate::agents::DecisionMode::parse(&decision.mode).is_none() {
        violations.push(ConfigViolation::new(
            "bad_threshold",
            format!("decision.mode=\"{}\" 无法解析（rule|jev_shadow|jev_active）", decision.mode),
        ));
    }
    violations
}

/// 校验文件根：路径必须存在（不存在 = 配置错误，不允许隐式创建用户目录）。
fn validate_file_roots(settings: &AppSettings) -> Vec<ConfigViolation> {
    let mut violations = Vec::new();
    for root in &settings.knowledge.file_roots {
        if root.path.trim().is_empty() {
            violations.push(ConfigViolation::new(
                "invalid_path",
                format!("knowledge 文件根 {} 路径为空", root.id),
            ));
            continue;
        }
        if !Path::new(&root.path).exists() {
            violations.push(ConfigViolation::new(
                "invalid_path",
                format!("knowledge 文件根 {} 不存在：{}", root.id, root.path),
            ));
        }
    }
    for root in &settings.knowledge.document_roots {
        if root.path.trim().is_empty() {
            violations.push(ConfigViolation::new(
                "invalid_path",
                format!("documents 根 {} 路径为空", root.id),
            ));
            continue;
        }
        if !Path::new(&root.path).exists() {
            violations.push(ConfigViolation::new(
                "invalid_path",
                format!("documents 根 {} 不存在：{}", root.id, root.path),
            ));
        }
    }
    violations
}

/// 完整启动校验。
pub fn validate_startup(settings: &AppSettings, service: &ServiceConfig) -> ConfigReport {
    let mut violations = Vec::new();
    violations.extend(validate_bind(service));
    if let Some(violation) = validate_writable_dir(&service.data_dir, "invalid_path") {
        violations.push(violation);
    }
    if let Some(violation) = validate_writable_dir(&service.config_dir, "invalid_path") {
        violations.push(violation);
    }
    if let Some(violation) = validate_writable_dir(&service.config_dir, "missing_required_dir") {
        violations.push(violation);
    }
    violations.extend(validate_mcp(&settings.server.mcp, service.identity_configured));
    violations.extend(validate_limits(settings));
    violations.extend(validate_file_roots(settings));
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DecisionSettings;
    use crate::operations::DeployMode;

    fn service(bind: &str, mode: DeployMode) -> ServiceConfig {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.keep();
        ServiceConfig {
            bind: bind.to_string(),
            allow_remote: false,
            mode,
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            identity_configured: false,
        }
    }

    #[test]
    fn loopback_development_passes() {
        let settings = AppSettings::default();
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        assert!(report.is_ok(), "{report:?}");
    }

    #[test]
    fn invalid_port_is_rejected() {
        let settings = AppSettings::default();
        let report = validate_startup(&settings, &service("127.0.0.1:0", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "invalid_port"), "{violations:?}");
    }

    #[test]
    fn malformed_bind_is_rejected() {
        let settings = AppSettings::default();
        let report = validate_startup(&settings, &service("not-an-address", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "invalid_bind"), "{violations:?}");
    }

    #[test]
    fn production_non_loopback_requires_opt_in() {
        let settings = AppSettings::default();
        let report = validate_startup(&settings, &service("0.0.0.0:8080", DeployMode::Production));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "unsafe_bind"), "{violations:?}");
    }

    #[test]
    fn production_non_loopback_allowed_with_flag() {
        let settings = AppSettings::default();
        let mut config = service("0.0.0.0:8080", DeployMode::Production);
        config.allow_remote = true;
        assert!(validate_startup(&settings, &config).is_ok());
    }

    #[test]
    fn remote_mcp_without_identity_is_rejected() {
        let mut settings = AppSettings::default();
        settings.server.mcp.remote_enabled = true;
        settings.server.mcp.http_enabled = true;
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(
            violations.iter().any(|v| v.code == "remote_mcp_without_auth"),
            "{violations:?}"
        );
    }

    #[test]
    fn remote_mcp_with_identity_passes() {
        let mut settings = AppSettings::default();
        settings.server.mcp.remote_enabled = true;
        settings.server.mcp.http_enabled = true;
        let mut config = service("127.0.0.1:8080", DeployMode::Development);
        config.identity_configured = true;
        assert!(validate_startup(&settings, &config).is_ok());
    }

    #[test]
    fn bad_decision_mode_is_rejected() {
        let mut settings = AppSettings::default();
        settings.decision = DecisionSettings {
            mode: "teleport".into(),
            ..DecisionSettings::default()
        };
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "bad_threshold"), "{violations:?}");
    }

    #[test]
    fn bad_confirmation_ttl_is_rejected() {
        let mut settings = AppSettings::default();
        settings.server.confirmation_ttl_secs = 5;
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "bad_threshold"), "{violations:?}");
    }

    #[test]
    fn out_of_range_threshold_is_rejected() {
        let mut settings = AppSettings::default();
        settings.server.thresholds.cpu_warn_ratio = 1.5;
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "bad_threshold"), "{violations:?}");
    }

    #[test]
    fn missing_file_root_is_rejected() {
        let mut settings = AppSettings::default();
        settings.knowledge.file_roots.push(crate::files::KnowledgeRoot::new(
            "ghost",
            "不存在",
            "/definitely/not/here/self-tools-test",
        ));
        let report = validate_startup(&settings, &service("127.0.0.1:8080", DeployMode::Development));
        let violations = report.expect_err("must fail");
        assert!(violations.iter().any(|v| v.code == "invalid_path"), "{violations:?}");
    }

    #[test]
    fn violations_never_contain_secret_values() {
        let mut settings = AppSettings::default();
        settings.decision.jev_api_key = Some("ts_super_secret_key".into());
        settings.decision.mode = "jev_active".into();
        settings.server.mcp.remote_enabled = true;
        let report = validate_startup(&settings, &service("bad", DeployMode::Development));
        let violations = report.expect_err("must fail");
        let text = violations
            .iter()
            .map(|v| format!("{} {}", v.code, v.detail))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("ts_super_secret_key"), "校验输出不得含 key: {text}");
    }
}
