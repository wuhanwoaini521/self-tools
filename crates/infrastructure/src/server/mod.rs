//! 家庭服务器平台适配器（V7 Track A/B/C 的 infrastructure 侧）。
//!
//! **本目录是 `std::process::Command` 在整个 V7 中唯一允许出现的位置**
//! （Plan §1.3 / §24/§25）：
//! - 固定 executable + 固定 argument template；argv 全部字面量；
//! - 永不接受模型输出或任意字符串进 argv；
//! - 没有 `sh -c` / `bash -c` / `zsh -c`（Gate 9 静态断言）。
//!
//! 目标平台 macOS 12（launchd / sysctl / df）；其它平台返回 `Unknown`
//! 而不是报错（Gate 2：部分指标不可用不 panic）。

pub mod audit;
pub mod logs;
pub mod metrics;
pub mod probes;

pub use audit::{ServerActionAuditSqlite, test_support};
pub use logs::LocalLogTail;
pub use metrics::{LocalSystemMetrics, PlatformSystemMetrics};
pub use probes::{HttpHealthProbe, LaunchdServiceControl, LaunchdServiceProbe};
