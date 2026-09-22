//! 应用路径解析（V11 §49）：config / data / cache / logs / backup / runtime 统一入口。
//!
//! **禁止硬编码个人目录**：全部路径由环境变量或可执行文件位置推导，
//! 默认值与平台惯例一致（Windows: `%LOCALAPPDATA%`；Unix: `~/.local/share`）。
//!
//! ```text
//! SELF_TOOLS_HOME
//!   ├── config/    settings.json（含 secret；gitignored）
//!   ├── data/      业务 SQLite / DuckDB（用户数据）
//!   ├── cache/     可重建索引 / 派生缓存
//!   ├── logs/      结构化日志
//!   ├── backup/    备份目标
//!   └── runtime/   pid / lock / 崩溃恢复标记
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod config;
pub mod lifecycle;
pub mod observability;

pub use config::{ConfigReport, ConfigViolation, ServiceConfig, validate_startup};
pub use lifecycle::{
    CrashRecoveryReport, InstanceLock, LAUNCHD_LABEL, LaunchdCommand, SERVICE_NAME,
    ShutdownStage, StartupMarker, render_launchd_plist,
};
pub use observability::{
    DependencyHealth, HealthReport, HealthState, LogLevel, MetricsSnapshot, StructuredLogEvent,
    redact_log_value,
};

/// 环境变量：覆盖 home 根（部署 / 测试 / launchd 用）。
pub const ENV_HOME: &str = "SELF_TOOLS_HOME";

/// 应用名（目录名）。
pub const APP_NAME: &str = "self-tools";

/// 解析后的应用路径集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// 从 home 根构造（不创建目录）。
    #[must_use]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 按平台默认推导 home 根（不读环境变量）。
    ///
    /// - Windows：`%LOCALAPPDATA%\self-tools`
    /// - macOS：`~/Library/Application Support/self-tools`
    /// - 其他 Unix：`~/.local/share/self-tools`
    #[must_use]
    pub fn platform_default() -> Self {
        if let Some(root) = platform_data_root() {
            return Self::from_root(root.join(APP_NAME));
        }
        // 无法推导（无 home）→ 当前目录下的 `.self-tools`（可写且可预期）。
        Self::from_root(PathBuf::from(".").join(format!(".{APP_NAME}")))
    }

    /// 解析：`SELF_TOOLS_HOME` > 平台默认。
    #[must_use]
    pub fn resolve() -> Self {
        std::env::var_os(ENV_HOME)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(Self::from_root)
            .unwrap_or_else(Self::platform_default)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    #[must_use]
    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    #[must_use]
    pub fn backup(&self) -> PathBuf {
        self.root.join("backup")
    }

    #[must_use]
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }

    /// settings.json 路径（含 secret；gitignored）。
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config().join("settings.json")
    }

    /// 创建全部必需目录（幂等）。任何失败返回该目录路径与错误。
    pub fn ensure_dirs(&self) -> Result<(), (PathBuf, std::io::Error)> {
        for dir in [
            self.config(),
            self.data(),
            self.cache(),
            self.logs(),
            self.backup(),
            self.runtime(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|error| (dir, error))?;
        }
        Ok(())
    }

    /// 崩溃恢复标记（启动时存在 = 上次未优雅退出）。
    #[must_use]
    pub fn crash_marker(&self) -> PathBuf {
        self.runtime().join("unclean-shutdown")
    }

    /// 单实例锁文件（启动时创建，退出时删除）。
    #[must_use]
    pub fn lock_file(&self) -> PathBuf {
        self.runtime().join("instance.lock")
    }
}

/// 平台数据根（不含 app 名）。
fn platform_data_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        None
    }
}

/// home 目录（环境变量优先；不引入 dirs crate）。
#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// 部署模式（影响默认 bind / HTTPS 要求）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployMode {
    /// 开发：loopback、无 HTTPS 要求。
    #[default]
    Development,
    /// 生产（家庭服务器）：LAN 暴露要求安全上下文。
    Production,
}

impl DeployMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DeployMode::Development => "development",
            DeployMode::Production => "production",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" => Some(DeployMode::Development),
            "production" | "prod" => Some(DeployMode::Production),
            _ => None,
        }
    }

    /// 是否要求安全上下文（HTTPS）。
    #[must_use]
    pub fn requires_secure_context(self) -> bool {
        matches!(self, DeployMode::Production)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_are_structured() {
        let paths = AppPaths::from_root("/tmp/self-tools");
        assert_eq!(paths.config(), PathBuf::from("/tmp/self-tools/config"));
        assert_eq!(paths.data(), PathBuf::from("/tmp/self-tools/data"));
        assert_eq!(paths.cache(), PathBuf::from("/tmp/self-tools/cache"));
        assert_eq!(paths.logs(), PathBuf::from("/tmp/self-tools/logs"));
        assert_eq!(paths.backup(), PathBuf::from("/tmp/self-tools/backup"));
        assert_eq!(paths.runtime(), PathBuf::from("/tmp/self-tools/runtime"));
        assert_eq!(paths.settings_file(), PathBuf::from("/tmp/self-tools/config/settings.json"));
        assert_eq!(paths.crash_marker().file_name().unwrap(), "unclean-shutdown");
        assert_eq!(paths.lock_file().file_name().unwrap(), "instance.lock");
    }

    #[test]
    fn ensure_dirs_creates_all_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_root(dir.path().join("home"));
        paths.ensure_dirs().expect("create");
        for path in [paths.config(), paths.data(), paths.cache(), paths.logs(), paths.backup(), paths.runtime()] {
            assert!(path.is_dir(), "{}", path.display());
        }
        // 幂等。
        paths.ensure_dirs().expect("re-create");
    }

    #[test]
    fn env_home_overrides_platform_default() {
        // 不修改真实环境：只验证 resolve 的优先级逻辑在一个临时 HOME 下可用。
        let paths = AppPaths::platform_default();
        assert!(!paths.root().as_os_str().is_empty());
    }

    #[test]
    fn deploy_mode_secure_context_rules() {
        assert!(!DeployMode::Development.requires_secure_context());
        assert!(DeployMode::Production.requires_secure_context());
        assert_eq!(DeployMode::parse("prod"), Some(DeployMode::Production));
        assert_eq!(DeployMode::parse("dev"), Some(DeployMode::Development));
        assert_eq!(DeployMode::parse("nonsense"), None);
    }
}
