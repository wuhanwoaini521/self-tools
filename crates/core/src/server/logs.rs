//! 日志契约（V7 §37-§41）。
//!
//! 日志是**不受信数据**（§41）：进入模型前必须脱敏（§40）并在 prompt 中
//! 明确标记。硬限制（行数 / 字节 / 时间窗）在这里定义默认值，
//! 具体上限由 `settings.server` 配置化后经 [`LogReadRequest`] 传入。

use serde::{Deserialize, Serialize};

use super::contains_traversal;
use std::path::Path;

/// 日志读取请求（§38：三硬限制）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogReadRequest {
    /// 注册服务 id（模型唯一可传的标识）。
    pub service_id: String,
    /// 注册日志源 id（来自 descriptor；缺省 = 第一个源）。
    #[serde(default)]
    pub log_source_id: Option<String>,
    /// 最多返回行数（0 = 用默认）。
    #[serde(default)]
    pub max_lines: usize,
    /// 最多返回字节（0 = 用默认）。
    #[serde(default)]
    pub max_bytes: usize,
    /// 只取最近 N 秒（0 = 不限，仍受其它上限约束）。
    #[serde(default)]
    pub max_age_secs: u64,
}

impl Default for LogReadRequest {
    fn default() -> Self {
        Self {
            service_id: String::new(),
            log_source_id: None,
            max_lines: 0,
            max_bytes: 0,
            max_age_secs: 0,
        }
    }
}

/// 读取结果（脱敏后；`redactions` 计数不含内容）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogReadResult {
    pub service_id: String,
    pub log_source_id: String,
    /// 脱敏后的日志文本（已截断）。
    pub text: String,
    /// 实际返回行数。
    pub lines: usize,
    /// 脱敏命中次数（只计数，不记录内容）。
    pub redactions: usize,
    /// 是否因上限截断。
    pub truncated: bool,
}

/// 默认 / 硬上限（§38；配置可在其内收紧，不能放宽）。
pub const DEFAULT_MAX_LINES: usize = 200;
pub const MAX_LINES_LIMIT: usize = 2_000;
pub const DEFAULT_MAX_BYTES: usize = 64 * 1_024;
pub const MAX_BYTES_LIMIT: usize = 1_024 * 1_024;
pub const DEFAULT_MAX_AGE_SECS: u64 = 300;
pub const MAX_AGE_LIMIT_SECS: u64 = 86_400;

/// 把调用方请求收敛到合法区间（0 = 默认；超过硬上限 → 截到硬上限）。
#[must_use]
pub fn clamp_limits(request: &LogReadRequest) -> (usize, usize, u64) {
    let lines = match request.max_lines {
        0 => DEFAULT_MAX_LINES,
        value => value.min(MAX_LINES_LIMIT),
    };
    let bytes = match request.max_bytes {
        0 => DEFAULT_MAX_BYTES,
        value => value.min(MAX_BYTES_LIMIT),
    };
    let age = match request.max_age_secs {
        0 => DEFAULT_MAX_AGE_SECS,
        value => value.min(MAX_AGE_LIMIT_SECS),
    };
    (lines, bytes, age)
}

/// 时间窗（§38）：日志按时间过滤时的范围描述。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogWindow {
    /// 起始 Unix 秒（含）。
    pub from_secs: i64,
    /// 结束 Unix 秒（含）；0 = 到现在。
    pub to_secs: i64,
}

impl LogWindow {
    #[must_use]
    pub fn last(seconds: u64, now: i64) -> Self {
        Self {
            from_secs: now.saturating_sub(seconds as i64),
            to_secs: 0,
        }
    }
}

/// 日志路径校验（§39）：必须来自注册表（本函数只做形态防御，
/// 「是否注册」由 registry 查询保证）。
#[must_use]
pub fn is_safe_log_path(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 1_024 {
        return false;
    }
    !contains_traversal(Path::new(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_uses_defaults_and_hard_caps() {
        let (lines, bytes, age) = clamp_limits(&LogReadRequest::default());
        assert_eq!((lines, bytes, age), (DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES, DEFAULT_MAX_AGE_SECS));

        let greedy = LogReadRequest {
            max_lines: 999_999,
            max_bytes: 999_999_999,
            max_age_secs: 999_999,
            ..LogReadRequest::default()
        };
        let (lines, bytes, age) = clamp_limits(&greedy);
        assert_eq!((lines, bytes, age), (MAX_LINES_LIMIT, MAX_BYTES_LIMIT, MAX_AGE_LIMIT_SECS));
    }

    #[test]
    fn log_paths_reject_traversal() {
        assert!(is_safe_log_path("/var/log/self-tools.log"));
        assert!(!is_safe_log_path("../../etc/passwd"));
        assert!(!is_safe_log_path("   "));
    }
}
