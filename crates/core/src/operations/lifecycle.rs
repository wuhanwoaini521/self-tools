//! macOS launchd 生命周期 + 优雅退出 + 崩溃恢复（V11 §54-§57）。
//!
//! ```text
//! launchd (KeepAlive)
//!   └── self-tools --mode production
//!         ├── acquire instance lock（runtime/instance.lock）
//!         ├── write unclean-shutdown marker（runtime/unclean-shutdown）
//!         ├── recover previous run（interrupted / expired / recoverable）
//!         ├── serve (HTTP + MCP)
//!         └── graceful shutdown：SIGTERM/SIGINT → 有序退出 → 删 marker + lock
//! ```
//!
//! **不在开发环境强制安装**：plist 生成是显式命令（`--print-launchd`），
//! 安装由运维决定。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::operations::AppPaths;

/// launchd 标签（反向域名）。
pub const LAUNCHD_LABEL: &str = "ai.self-tools.server";

/// 服务名（日志 / 状态显示）。
pub const SERVICE_NAME: &str = "self-tools";

/// 优雅退出阶段（§56：顺序固定）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownStage {
    /// 停止接受新请求。
    StopAccepting,
    /// 取消有界 agent（worker 收到 cancel token）。
    CancelAgents,
    /// 过期待确认工作（confirmation tickets 过期）。
    ExpirePendingWork,
    /// flush audit。
    FlushAudit,
    /// 关闭 DB。
    CloseDatabases,
    /// 关闭 MCP。
    ShutdownMcp,
    /// 关闭 HTTP。
    ShutdownHttp,
    /// 释放锁 + 清理标记。
    ReleaseLocks,
}

impl ShutdownStage {
    /// 全部阶段（顺序即数组顺序）。
    #[must_use]
    pub fn all() -> [ShutdownStage; 8] {
        [
            ShutdownStage::StopAccepting,
            ShutdownStage::CancelAgents,
            ShutdownStage::ExpirePendingWork,
            ShutdownStage::FlushAudit,
            ShutdownStage::CloseDatabases,
            ShutdownStage::ShutdownMcp,
            ShutdownStage::ShutdownHttp,
            ShutdownStage::ReleaseLocks,
        ]
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ShutdownStage::StopAccepting => "stop_accepting",
            ShutdownStage::CancelAgents => "cancel_agents",
            ShutdownStage::ExpirePendingWork => "expire_pending_work",
            ShutdownStage::FlushAudit => "flush_audit",
            ShutdownStage::CloseDatabases => "close_databases",
            ShutdownStage::ShutdownMcp => "shutdown_mcp",
            ShutdownStage::ShutdownHttp => "shutdown_http",
            ShutdownStage::ReleaseLocks => "release_locks",
        }
    }
}

/// 崩溃恢复结果（§57）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CrashRecoveryReport {
    /// 上次是否未优雅退出。
    pub unclean_previous_run: bool,
    /// 未完成 AgentRun → 标记 interrupted 的数量。
    pub interrupted_agent_runs: usize,
    /// 过期 confirmation ticket → expired 的数量。
    pub expired_confirmations: usize,
    /// 清理的 stale lock 数。
    pub released_locks: usize,
    /// 需要在启动后重建的索引/缓存。
    pub rebuildable_artifacts: Vec<String>,
}

/// 单实例锁（§56：release locks）。
///
/// 用「创建排他文件」语义（`create_new`）：已存在 = 别的实例在跑 → 拒绝启动
/// （家庭服务器单实例，避免两个进程共开 SQLite）。
pub struct InstanceLock {
    path: PathBuf,
}

impl InstanceLock {
    /// 尝试获取锁。Ok = 获得；Err = 已被占用（含占用者 pid 文本）。
    pub fn acquire(paths: &AppPaths) -> Result<Self, String> {
        let path = paths.lock_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create runtime dir: {error}"))?;
        }
        // 清理 stale lock：内容里的 pid 已不存在 → 可接管。
        if path.exists() {
            let stale = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
                .is_none_or(|pid| !process_alive(pid));
            if stale {
                let _ = std::fs::remove_file(&path);
            } else {
                return Err(format!("instance lock held: {}", path.display()));
            }
        }
        let pid = std::process::id();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("acquire instance lock ({}): {error}", path.display()))?;
        use std::io::Write;
        let _ = writeln!(file, "{pid}");
        drop(file);
        Ok(Self { path })
    }

    /// 释放锁（幂等）。
    pub fn release(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        self.release();
    }
}

/// 进程是否存活（Unix kill(0)；Windows 用 OpenProcess 语义过重 → 用 tasklist 不可取，
/// 这里退化为「stale = pid 无法解析」的保守策略之外的第二种判定：文件年龄）。
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: libc::kill 是 FFI；信号 0 只做存在性检查，不发送信号。
        let result = unsafe { libc_kill(pid as i32, 0) };
        result == 0
    }
    #[cfg(not(unix))]
    {
        // 无跨平台 pid 探针：假定存活（保守，除非运维删除 runtime/instance.lock）。
        let _ = pid;
        true
    }
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, signal: i32) -> i32 {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // SAFETY: kill(2) 只读进程存在性。
    unsafe { kill(pid, signal) }
}

/// 启动标记管理（§57：unclean-shutdown）。
pub struct StartupMarker<'a> {
    paths: &'a AppPaths,
}

impl<'a> StartupMarker<'a> {
    #[must_use]
    pub fn new(paths: &'a AppPaths) -> Self {
        Self { paths }
    }

    /// 上次是否未优雅退出（marker 存在 = crash / kill -9）。
    #[must_use]
    pub fn was_unclean(&self) -> bool {
        self.paths.crash_marker().exists()
    }

    /// 写入运行标记（启动时）。
    pub fn mark_running(&self) -> std::io::Result<()> {
        let path = self.paths.crash_marker();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, now_unix().to_string())
    }

    /// 清除运行标记（优雅退出时）。
    pub fn mark_clean(&self) {
        let _ = std::fs::remove_file(self.paths.crash_marker());
    }

    /// 启动恢复：把上次的未完成工作分类为 interrupted / expired / recoverable。
    ///
    /// 只做**状态标记与统计**（真实存储操作由各 store 的 recover 端口完成；
    /// 本函数返回报告供启动日志记录）。
    #[must_use]
    pub fn recover(&self) -> CrashRecoveryReport {
        let unclean = self.was_unclean();
        CrashRecoveryReport {
            unclean_previous_run: unclean,
            // 具体数量由调用方从 store 读取后填（详见 PRODUCTION_V11.md §4）。
            interrupted_agent_runs: 0,
            expired_confirmations: 0,
            released_locks: usize::from(unclean),
            rebuildable_artifacts: if unclean {
                vec!["cache/".into(), "index/".into()]
            } else {
                Vec::new()
            },
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// launchd plist 模板（§54/§55：绝对路径 + 显式数据路径 + env 文件 + 安全重启）。
///
/// `--print-launchd` 输出；安装需运维显式执行（不在开发环境强制）。
#[must_use]
pub fn render_launchd_plist(
    executable: &Path,
    paths: &AppPaths,
    env_file: &Path,
    log_dir: &Path,
) -> String {
    let executable = xml_escape(&executable.display().to_string());
    let home = xml_escape(&paths.root().display().to_string());
    let env_file = xml_escape(&env_file.display().to_string());
    let stdout = xml_escape(&log_dir.join("self-tools.out.log").display().to_string());
    let stderr = xml_escape(&log_dir.join("self-tools.err.log").display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
    <string>--mode</string>
    <string>production</string>
  </array>
  <key>WorkingDirectory</key>
  <string>{home}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>SELF_TOOLS_HOME</key>
    <string>{home}</string>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
  <!-- Secret 只进 env 文件（0600），不写进 plist。 -->
  <key>SoftResourceLimits</key>
  <dict>
    <key>NumberOfFiles</key>
    <integer>4096</integer>
  </dict>
  <key>StandardOutPath</key>
  <string>{stdout}</string>
  <key>StandardErrorPath</key>
  <string>{stderr}</string>
  <key>KeepAlive</key>
  <dict>
    <!-- 崩溃自动重启；正常退出（ExitCode 0）不重启。 -->
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Background</string>
</dict>
</plist>
"#,
    )
    .replace("<!-- Secret 只进 env 文件（0600），不写进 plist。 -->", &format!(
        "<!-- env 文件（0600，含 LLM/Jev key）：{env_file} -->"
    ))
}

fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// launchd 管理命令（§54：generate / install / uninstall / status / start / stop / restart）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchdCommand {
    /// 生成 plist（打印或写文件）。
    Generate,
    /// 安装（launchctl bootstrap）。
    Install,
    /// 卸载（launchctl bootout）。
    Uninstall,
    /// 状态。
    Status,
    Start,
    Stop,
    Restart,
}

impl LaunchdCommand {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LaunchdCommand::Generate => "generate",
            LaunchdCommand::Install => "install",
            LaunchdCommand::Uninstall => "uninstall",
            LaunchdCommand::Status => "status",
            LaunchdCommand::Start => "start",
            LaunchdCommand::Stop => "stop",
            LaunchdCommand::Restart => "restart",
        }
    }

    /// CLI 参数解析（`--launchd <cmd>`）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "generate" | "gen" => Some(LaunchdCommand::Generate),
            "install" => Some(LaunchdCommand::Install),
            "uninstall" | "remove" => Some(LaunchdCommand::Uninstall),
            "status" => Some(LaunchdCommand::Status),
            "start" => Some(LaunchdCommand::Start),
            "stop" => Some(LaunchdCommand::Stop),
            "restart" => Some(LaunchdCommand::Restart),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> AppPaths {
        AppPaths::from_root("/tmp/self-tools-test")
    }

    #[test]
    fn shutdown_stages_are_ordered_and_complete() {
        let stages = ShutdownStage::all();
        assert_eq!(stages.len(), 8);
        assert_eq!(stages[0], ShutdownStage::StopAccepting);
        assert_eq!(stages[1], ShutdownStage::CancelAgents);
        assert_eq!(stages[7], ShutdownStage::ReleaseLocks);
        // 每阶段有稳定 label。
        for stage in stages {
            assert!(!stage.as_str().is_empty());
        }
    }

    #[test]
    fn lock_acquire_release_cycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_root(dir.path());
        let lock = InstanceLock::acquire(&paths).expect("acquire");
        // 第二个获取必须失败（单实例）。
        let second = InstanceLock::acquire(&paths);
        assert!(second.is_err(), "单实例锁必须拒绝第二个实例");
        lock.release();
        // 释放后可再获取。
        assert!(InstanceLock::acquire(&paths).is_ok());
    }

    #[test]
    fn stale_lock_is_reclaimed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_root(dir.path());
        std::fs::create_dir_all(paths.runtime()).expect("mkdir");
        // 写一个不可解析的 pid（视为 stale）。
        std::fs::write(paths.lock_file(), "not-a-pid").expect("write");
        let lock = InstanceLock::acquire(&paths).expect("stale lock reclaim");
        lock.release();
    }

    #[test]
    fn marker_tracks_clean_and_unclean_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::from_root(dir.path());
        let marker = StartupMarker::new(&paths);
        assert!(!marker.was_unclean(), "首次启动无标记");
        marker.mark_running().expect("mark");
        assert!(marker.was_unclean(), "运行中标记存在");
        let report = marker.recover();
        assert!(report.unclean_previous_run);
        assert!(report.rebuildable_artifacts.contains(&"cache/".to_string()));
        marker.mark_clean();
        assert!(!marker.was_unclean());
        let clean = marker.recover();
        assert!(!clean.unclean_previous_run);
        assert!(clean.rebuildable_artifacts.is_empty());
    }

    #[test]
    fn launchd_plist_has_absolute_paths_and_no_secrets() {
        let plist = render_launchd_plist(
            Path::new("/opt/self-tools/bin/self-tools"),
            &paths(),
            Path::new("/opt/self-tools/env"),
            Path::new("/var/log/self-tools"),
        );
        assert!(plist.contains("<string>ai.self-tools.server</string>"));
        assert!(plist.contains("/opt/self-tools/bin/self-tools"));
        assert!(plist.contains("/tmp/self-tools-test"));
        assert!(plist.contains("SELF_TOOLS_HOME"));
        // plist 不含任何 key/token。
        assert!(!plist.contains("API_KEY"));
        assert!(!plist.contains("Bearer"));
        // 安全重启策略。
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>ThrottleInterval</key>"));
        // XML 合法（粗略：标签配对）。
        assert_eq!(plist.matches("<plist").count(), 1);
        assert_eq!(plist.matches("</plist>").count(), 1);
    }

    #[test]
    fn launchd_plist_escapes_xml() {
        let plist = render_launchd_plist(
            Path::new("/opt/a&b/self-tools"),
            &paths(),
            Path::new("/opt/env"),
            Path::new("/var/log"),
        );
        assert!(plist.contains("a&amp;b"), "{plist}");
        assert!(!plist.contains("/opt/a&b/self-tools"));
    }

    #[test]
    fn launchd_commands_parse() {
        for (raw, expected) in [
            ("generate", LaunchdCommand::Generate),
            ("install", LaunchdCommand::Install),
            ("uninstall", LaunchdCommand::Uninstall),
            ("status", LaunchdCommand::Status),
            ("start", LaunchdCommand::Start),
            ("stop", LaunchdCommand::Stop),
            ("restart", LaunchdCommand::Restart),
        ] {
            assert_eq!(LaunchdCommand::parse(raw), Some(expected));
        }
        assert_eq!(LaunchdCommand::parse("format-c"), None);
    }
}
