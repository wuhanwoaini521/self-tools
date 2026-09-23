//! 结构化日志 + 健康探针 + 指标（V11 §58-§63）。
//!
//! - `StructuredLogEvent`：字段化日志（无正文/secret）；
//! - `HealthProbe`：liveness / readiness / degraded 三态；
//! - `MetricsSnapshot`：请求/错误/tool/agent/decision/jev/mcp/safeaction/backup 计数。

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 结构化日志（§61-§62）
// ---------------------------------------------------------------------------

/// 日志级别。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// 结构化日志事件（§61 字段集）。
///
/// **隐私铁律（§62）**：`message` 与 `fields` 只允许事件标签 / 稳定错误码 /
/// 计数值；禁止完整 prompt、文档正文、Memory 内容、文件内容、key、token。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructuredLogEvent {
    /// Unix 毫秒。
    pub timestamp_ms: u64,
    pub level: LogLevel,
    /// 组件（如 `personal_ai` / `mcp` / `server` / `backup`）。
    pub component: String,
    /// 请求 id（短；不是 session 内容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// 来源（模块 / provider / transport）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 事件名（稳定 snake_case）。
    pub event: String,
    /// 耗时（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// 结果标签（ok / error / degraded / fallback）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// 附加计数字段（只放数字/短枚举）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl StructuredLogEvent {
    /// 构造（自动填时间戳）。
    #[must_use]
    pub fn new(component: &str, event: &str) -> Self {
        Self {
            timestamp_ms: now_ms(),
            level: LogLevel::Info,
            component: component.to_string(),
            request_id: None,
            session_id: None,
            trace_id: None,
            source: None,
            event: event.to_string(),
            duration_ms: None,
            result: None,
            fields: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }

    #[must_use]
    pub fn request(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    #[must_use]
    pub fn trace(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    #[must_use]
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    #[must_use]
    pub fn duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// 结果标签：`ok` / `error` / `degraded` / `fallback`。
    #[must_use]
    pub fn result(mut self, result: &str) -> Self {
        self.result = Some(result.to_string());
        self
    }

    /// 附加字段（只放短值；调用方负责不放正文）。
    #[must_use]
    pub fn field(mut self, key: &str, value: impl Into<String>) -> Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }

    /// 序列化为单行 JSON（tracing 之外的轻量落地）。
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

/// 脱敏：把疑似 secret 的键值替换为 `[REDACTED]`。
///
/// 用于任何进入日志的字段（双保险；调用方本就不该传 secret）。
#[must_use]
pub fn redact_log_value(key: &str, value: &str) -> String {
    let lowered = key.to_ascii_lowercase();
    if lowered.contains("key")
        || lowered.contains("token")
        || lowered.contains("secret")
        || lowered.contains("password")
        || lowered.contains("authorization")
        || lowered.contains("cookie")
    {
        return "[REDACTED]".into();
    }
    value.to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 健康（§58-§60）
// ---------------------------------------------------------------------------

/// 健康状态。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// 进程活着（liveness）。
    Alive,
    /// 可服务（readiness）。
    Ready,
    /// 部分能力不可用（外部依赖故障），核心仍工作。
    Degraded,
    /// 不可服务。
    #[default]
    Down,
}

impl HealthState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Alive => "alive",
            HealthState::Ready => "ready",
            HealthState::Degraded => "degraded",
            HealthState::Down => "down",
        }
    }

    /// HTTP 语义：ready/alive = 200；degraded = 200（仍服务）；down = 503。
    #[must_use]
    pub fn http_status(self) -> u16 {
        match self {
            HealthState::Down => 503,
            _ => 200,
        }
    }
}

/// 单个依赖的健康（§60：外部依赖不可用 = degraded，不是 down）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyHealth {
    pub name: String,
    pub state: HealthState,
    /// 稳定原因码（不含 secret）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 完整健康报告。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// liveness：进程是否响应。
    pub liveness: HealthState,
    /// readiness：核心依赖是否就绪。
    pub readiness: HealthState,
    /// 外部依赖（LLM / Jev / Search / MCP remote）单独列出。
    pub dependencies: Vec<DependencyHealth>,
    pub generated_at_ms: u64,
}

impl HealthReport {
    /// 汇总 liveness/readiness（核心依赖全 Ready → Ready；外部依赖故障 → Degraded）。
    #[must_use]
    pub fn summarize(core: Vec<DependencyHealth>, external: Vec<DependencyHealth>) -> Self {
        let readiness = if core.iter().any(|dep| dep.state == HealthState::Down) {
            HealthState::Down
        } else if core.iter().any(|dep| dep.state == HealthState::Degraded) {
            HealthState::Degraded
        } else {
            HealthState::Ready
        };
        let mut dependencies = core;
        let external_degraded = external.iter().any(|dep| dep.state != HealthState::Ready);
        dependencies.extend(external);
        Self {
            liveness: HealthState::Alive,
            readiness: if external_degraded && readiness == HealthState::Ready {
                HealthState::Degraded
            } else {
                readiness
            },
            dependencies,
            generated_at_ms: now_ms(),
        }
    }
}

// ---------------------------------------------------------------------------
// 指标（§63）
// ---------------------------------------------------------------------------

/// 指标快照（进程内计数；导出为 JSON 供 UI/运维读取）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub requests: u64,
    pub request_errors: u64,
    /// 工具调用总次数与总耗时（毫秒）。
    pub tool_calls: u64,
    pub tool_call_latency_ms: u64,
    /// Agent run 数（含 worker）。
    pub agent_runs: u64,
    /// 编排（非 direct）请求数。
    pub orchestrated_requests: u64,
    /// 决策 provider 命中次数（provider 名 → 次数）。
    #[serde(default)]
    pub decision_provider_hits: BTreeMap<String, u64>,
    /// Jev fallback 到 rule 的次数。
    pub jev_fallbacks: u64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub mcp_calls: u64,
    pub safe_actions: u64,
    pub health_failures: u64,
    pub backups: u64,
    pub backup_failures: u64,
}

impl MetricsSnapshot {
    /// 工具调用平均延迟（无调用 = 0）。
    #[must_use]
    pub fn avg_tool_latency_ms(&self) -> u64 {
        self.tool_call_latency_ms / self.tool_calls.max(1)
    }

    /// 编排率（0..1；无请求 = 0）。
    #[must_use]
    pub fn orchestration_rate(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.orchestrated_requests as f64 / self.requests as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_event_has_required_fields_and_redacts() {
        let event = StructuredLogEvent::new("personal_ai", "agent_request")
            .request("req-1")
            .trace("trace-1")
            .source("openai-compatible")
            .duration(12)
            .result("ok")
            .field("tool_calls", "2")
            .field("jev_api_key", "ts_secret_should_not_appear");
        let json = event.to_json();
        assert!(json.contains("\"event\":\"agent_request\""), "{json}");
        assert!(json.contains("\"component\":\"personal_ai\""));
        assert!(json.contains("\"duration_ms\":12"));
        assert!(json.contains("\"result\":\"ok\""));
        assert!(json.contains("\"request_id\":\"req-1\""));
        // 脱敏函数单独验证（builder 不做隐式脱敏，redact 是显式双保险）。
        assert_eq!(redact_log_value("api_key", "x"), "[REDACTED]");
        assert_eq!(redact_log_value("tool_calls", "2"), "2");
        assert!(!redact_log_value("jev_api_key", "ts_secret").contains("ts_secret"));
    }

    #[test]
    fn log_level_orders_and_serializes() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert_eq!(LogLevel::Info.as_str(), "info");
        let json = serde_json::to_value(LogLevel::Warn).unwrap();
        assert_eq!(json, "warn");
    }

    #[test]
    fn external_dependency_failure_is_degraded_not_down() {
        let report = HealthReport::summarize(
            vec![DependencyHealth {
                name: "database".into(),
                state: HealthState::Ready,
                reason: None,
            }],
            vec![DependencyHealth {
                name: "llm".into(),
                state: HealthState::Down,
                reason: Some("unavailable".into()),
            }],
        );
        assert_eq!(report.liveness, HealthState::Alive);
        assert_eq!(
            report.readiness,
            HealthState::Degraded,
            "外部依赖故障 = degraded"
        );
        assert_eq!(report.readiness.http_status(), 200);
    }

    #[test]
    fn core_failure_is_down() {
        let report = HealthReport::summarize(
            vec![DependencyHealth {
                name: "database".into(),
                state: HealthState::Down,
                reason: Some("open_failed".into()),
            }],
            Vec::new(),
        );
        assert_eq!(report.readiness, HealthState::Down);
        assert_eq!(report.readiness.http_status(), 503);
    }

    #[test]
    fn all_ready_is_ready() {
        let report = HealthReport::summarize(
            vec![DependencyHealth {
                name: "database".into(),
                state: HealthState::Ready,
                reason: None,
            }],
            vec![DependencyHealth {
                name: "jev".into(),
                state: HealthState::Ready,
                reason: None,
            }],
        );
        assert_eq!(report.readiness, HealthState::Ready);
    }

    #[test]
    fn metrics_derive_rates() {
        let snapshot = MetricsSnapshot {
            requests: 10,
            orchestrated_requests: 4,
            tool_calls: 8,
            tool_call_latency_ms: 80,
            ..MetricsSnapshot::default()
        };
        assert_eq!(snapshot.avg_tool_latency_ms(), 10);
        assert!((snapshot.orchestration_rate() - 0.4).abs() < f64::EPSILON);
        let empty = MetricsSnapshot::default();
        assert_eq!(empty.avg_tool_latency_ms(), 0);
        assert_eq!(empty.orchestration_rate(), 0.0);
    }

    #[test]
    fn health_report_serializes_stable_shape() {
        let report = HealthReport::summarize(Vec::new(), Vec::new());
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["liveness"], "alive");
        assert_eq!(json["readiness"], "ready");
        assert!(json["dependencies"].is_array());
    }
}
