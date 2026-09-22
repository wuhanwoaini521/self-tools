//! 系统就绪度契约（V11 §123-§127）。
//!
//! 依赖方向：`core::readiness` 无内部依赖。它只表达「哪些子系统已就绪、
//! 程度如何」的统一形状，不持有任何探测实现（application 提供探测端口，
//! infrastructure / 组合根装配具体探测）。
//!
//! 安全约束：`ReadinessCheck.detail` 是**面向展示的受控文本**，只允许
//! 「已配置 / 未配置 / 能力布尔」这类陈述，严禁 API key、base URL、模型名
//! 或任何 secret 泄漏（§126）。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Check ids（V11 §124，稳定契约：前端与测试按 id 检索，不可随意改名）
// ---------------------------------------------------------------------------

/// 探测项 id（稳定字符串契约）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessCheckId {
    /// 桌面 / 服务端进程本身（始终 Ready）。
    Backend,
    /// 本地 SQLite 主库可连接。
    Database,
    /// Personal AI 模型提供方（onn-ai 提供）。
    AiProvider,
    /// 图片 / 视觉能力开关。
    Vision,
    /// 决策引擎可用性。
    Decision,
    /// JEV 决策提供方可用性。
    Jev,
    /// 搜索能力（联网检索）可用性。
    Search,
    /// 本地 MCP 服务器连接性。
    McpLocal,
    /// 远程 MCP 服务器连接性。
    McpRemote,
    /// 家庭服务器可达性。
    HomeServer,
    /// 备份 / 恢复能力可用性。
    Backup,
    /// PWA 安全上下文（https 或 localhost）状态。
    PwaSecureContext,
    /// 设备会话 / 绑定状态。
    DeviceSession,
}

impl ReadinessCheckId {
    /// 全部探测项（报告必须包含的完整集合，§125）。
    pub const ALL: [ReadinessCheckId; 13] = [
        ReadinessCheckId::Backend,
        ReadinessCheckId::Database,
        ReadinessCheckId::AiProvider,
        ReadinessCheckId::Vision,
        ReadinessCheckId::Decision,
        ReadinessCheckId::Jev,
        ReadinessCheckId::Search,
        ReadinessCheckId::McpLocal,
        ReadinessCheckId::McpRemote,
        ReadinessCheckId::HomeServer,
        ReadinessCheckId::Backup,
        ReadinessCheckId::PwaSecureContext,
        ReadinessCheckId::DeviceSession,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReadinessCheckId::Backend => "backend",
            ReadinessCheckId::Database => "database",
            ReadinessCheckId::AiProvider => "ai_provider",
            ReadinessCheckId::Vision => "vision",
            ReadinessCheckId::Decision => "decision",
            ReadinessCheckId::Jev => "jev",
            ReadinessCheckId::Search => "search",
            ReadinessCheckId::McpLocal => "mcp_local",
            ReadinessCheckId::McpRemote => "mcp_remote",
            ReadinessCheckId::HomeServer => "home_server",
            ReadinessCheckId::Backup => "backup",
            ReadinessCheckId::PwaSecureContext => "pwa_secure_context",
            ReadinessCheckId::DeviceSession => "device_session",
        }
    }

    /// 前端展示标签（中文；不含任何配置值）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ReadinessCheckId::Backend => "后端进程",
            ReadinessCheckId::Database => "本地数据库",
            ReadinessCheckId::AiProvider => "AI 提供方",
            ReadinessCheckId::Vision => "视觉能力",
            ReadinessCheckId::Decision => "决策引擎",
            ReadinessCheckId::Jev => "JEV 决策",
            ReadinessCheckId::Search => "搜索能力",
            ReadinessCheckId::McpLocal => "本地 MCP",
            ReadinessCheckId::McpRemote => "远程 MCP",
            ReadinessCheckId::HomeServer => "家庭服务器",
            ReadinessCheckId::Backup => "备份恢复",
            ReadinessCheckId::PwaSecureContext => "PWA 安全上下文",
            ReadinessCheckId::DeviceSession => "设备会话",
        }
    }

    /// 从稳定字符串解析（未知 id 返回 `None`，便于宽松反序列化）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        Self::ALL
            .into_iter()
            .find(|id| id.as_str() == trimmed)
    }
}

// ---------------------------------------------------------------------------
// Status / Check / Report
// ---------------------------------------------------------------------------

/// 单项探测结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStatus {
    /// 已就绪。
    Ready,
    /// 降级可用（功能不完整但不阻塞主流程）。
    Degraded,
    /// 未配置（用户尚未提供必要设置，属于正常初始状态）。
    NotConfigured,
    /// 探测失败（阻塞主流程）。
    Failed,
}

impl ReadinessStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReadinessStatus::Ready => "ready",
            ReadinessStatus::Degraded => "degraded",
            ReadinessStatus::NotConfigured => "not_configured",
            ReadinessStatus::Failed => "failed",
        }
    }

    /// 是否属于「阻断性问题」（聚合规则使用，§125）。
    #[must_use]
    pub fn is_blocking(self) -> bool {
        matches!(self, ReadinessStatus::Failed)
    }
}

/// 单项探测结果。`detail` 只承载受控文本（§126）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadinessCheck {
    /// 稳定 id（`ReadinessCheckId::as_str`）。
    pub id: String,
    /// 展示标签（中文）。
    pub label: String,
    pub status: ReadinessStatus,
    /// 受控说明文本：只允许「已配置 / 未配置 / 能力布尔」类陈述。
    pub detail: String,
    /// 该状态是否阻断主流程。
    pub blocking: bool,
}

impl ReadinessCheck {
    /// 以稳定 id 构造（label 取 `ReadinessCheckId::label`）。
    #[must_use]
    pub fn new(id: ReadinessCheckId, status: ReadinessStatus, detail: impl Into<String>) -> Self {
        Self::with_label(id.as_str().to_string(), id.label().to_string(), status, detail)
    }

    /// 完全自定义（测试 / 特殊探测用）。
    #[must_use]
    pub fn with_label(
        id: String,
        label: String,
        status: ReadinessStatus,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id,
            label,
            status,
            detail: detail.into(),
            blocking: status.is_blocking(),
        }
    }

    /// 构造「能力未开启」探测：状态固定为 `NotConfigured`。
    #[must_use]
    pub fn not_configured(id: ReadinessCheckId, detail: impl Into<String>) -> Self {
        Self::new(id, ReadinessStatus::NotConfigured, detail)
    }
}

/// 就绪度报告（§123）：13 项探测 + 聚合结论 + 生成时间戳。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub overall: ReadinessStatus,
    pub checks: Vec<ReadinessCheck>,
    /// Unix epoch 秒。
    pub generated_at: i64,
}

impl ReadinessReport {
    /// 由探测结果聚合（§125）：
    /// 任一项 `Failed` 且 `blocking` → `Failed`；否则任一项 `Degraded` →
    /// `Degraded`；否则 `Ready`。`NotConfigured` 不影响 overall。
    #[must_use]
    pub fn aggregate(checks: Vec<ReadinessCheck>) -> Self {
        let overall = aggregate_status(&checks);
        Self {
            overall,
            checks,
            generated_at: 0,
        }
    }

    /// 附加生成时间戳（由调用方注入时钟，便于测试确定性）。
    #[must_use]
    pub fn generated_at(mut self, generated_at: i64) -> Self {
        self.generated_at = generated_at;
        self
    }

    #[must_use]
    pub fn check(&self, id: ReadinessCheckId) -> Option<&ReadinessCheck> {
        self.checks
            .iter()
            .find(|check| check.id == id.as_str())
    }
}

/// 聚合规则（§125）：阻断失败优先，其次降级，否则就绪。
/// `NotConfigured` 是正常初始状态，不影响 overall。
#[must_use]
pub fn aggregate_status(checks: &[ReadinessCheck]) -> ReadinessStatus {
    if checks
        .iter()
        .any(|check| check.blocking && check.status == ReadinessStatus::Failed)
    {
        ReadinessStatus::Failed
    } else if checks
        .iter()
        .any(|check| check.status == ReadinessStatus::Degraded)
    {
        ReadinessStatus::Degraded
    } else {
        ReadinessStatus::Ready
    }
}

/// 诊断视图（§127）：与 `ReadinessCheck` 同源的只读诊断项。
///
/// 复用同一形状，因为诊断面板与就绪面板需要的信息一致；
/// 预留下钻字段位由服务层扩展。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub id: String,
    pub label: String,
    pub status: ReadinessStatus,
    /// 受控说明文本（无 secret，§126）。
    pub detail: String,
    /// 是否阻断主流程。
    pub blocking: bool,
}

impl From<ReadinessCheck> for DiagnosticCheck {
    fn from(check: ReadinessCheck) -> Self {
        Self {
            id: check.id,
            label: check.label,
            status: check.status,
            detail: check.detail,
            blocking: check.blocking,
        }
    }
}

impl From<&ReadinessCheck> for DiagnosticCheck {
    fn from(check: &ReadinessCheck) -> Self {
        Self {
            id: check.id.clone(),
            label: check.label.clone(),
            status: check.status,
            detail: check.detail.clone(),
            blocking: check.blocking,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(id: ReadinessCheckId) -> ReadinessCheck {
        ReadinessCheck::new(id, ReadinessStatus::Ready, "已配置")
    }

    #[test]
    fn check_ids_match_stable_strings() {
        // 契约字符串不可漂移：前端按 id 检索（§124）。
        let expected: Vec<&str> = vec![
            "backend",
            "database",
            "ai_provider",
            "vision",
            "decision",
            "jev",
            "search",
            "mcp_local",
            "mcp_remote",
            "home_server",
            "backup",
            "pwa_secure_context",
            "device_session",
        ];
        let actual: Vec<&str> = ReadinessCheckId::ALL
            .iter()
            .map(|id| id.as_str())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn check_ids_round_trip_through_string() {
        for id in ReadinessCheckId::ALL {
            assert_eq!(ReadinessCheckId::parse(id.as_str()), Some(id));
        }
        assert_eq!(ReadinessCheckId::parse("unknown"), None);
        assert_eq!(ReadinessCheckId::parse("  backend  "), Some(ReadinessCheckId::Backend));
    }

    #[test]
    fn statuses_serialize_snake_case() {
        let encoded = serde_json::to_string(&ReadinessStatus::NotConfigured).unwrap();
        assert_eq!(encoded, "\"not_configured\"");
        let decoded: ReadinessStatus = serde_json::from_str("\"ready\"").unwrap();
        assert_eq!(decoded, ReadinessStatus::Ready);
    }

    #[test]
    fn check_new_fills_label_and_blocking_from_status() {
        let failed = ReadinessCheck::new(ReadinessCheckId::Database, ReadinessStatus::Failed, "连接失败");
        assert_eq!(failed.id, "database");
        assert_eq!(failed.label, "本地数据库");
        assert!(failed.blocking);

        let degraded = ReadinessCheck::new(ReadinessCheckId::Search, ReadinessStatus::Degraded, "部分降级");
        assert!(!degraded.blocking);
    }

    #[test]
    fn aggregate_prefers_failed_then_degraded_then_ready() {
        let checks = vec![ready(ReadinessCheckId::Backend)];
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Ready);

        let mut checks = vec![ready(ReadinessCheckId::Backend)];
        checks.push(ReadinessCheck::new(
            ReadinessCheckId::Search,
            ReadinessStatus::NotConfigured,
            "未配置",
        ));
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Ready);

        checks[1] = ReadinessCheck::new(ReadinessCheckId::Search, ReadinessStatus::Degraded, "降级");
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Degraded);

        checks[1] = ReadinessCheck::new(ReadinessCheckId::Search, ReadinessStatus::Failed, "失败");
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Failed);
    }

    #[test]
    fn non_blocking_failure_does_not_fail_overall() {
        // `with_label` 对 Failed 默认置阻断（安全侧），探测方显式声明非阻断时
        // 由 blocking 字段表达：单一可选源失败不得拖垮 overall。
        let mut checks = vec![ReadinessCheck::new(
            ReadinessCheckId::Search,
            ReadinessStatus::Failed,
            "部分来源不可用",
        )];
        assert!(checks[0].blocking);
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Failed);

        // 探测方显式降级为非阻断项后，聚合与构造必须一致。
        checks[0].blocking = false;
        assert!(!checks[0].blocking);
        assert_eq!(aggregate_status(&checks), ReadinessStatus::Ready);
    }

    #[test]
    fn report_aggregate_then_generated_at() {
        let report = ReadinessReport::aggregate(vec![ready(ReadinessCheckId::Backend)])
            .generated_at(1_700_000_000);
        assert_eq!(report.overall, ReadinessStatus::Ready);
        assert_eq!(report.generated_at, 1_700_000_000);
        assert_eq!(report.check(ReadinessCheckId::Backend).unwrap().status, ReadinessStatus::Ready);
        assert!(report.check(ReadinessCheckId::Database).is_none());
    }

    #[test]
    fn diagnostic_check_converts_from_check_and_reference() {
        let check = ReadinessCheck::new(ReadinessCheckId::Vision, ReadinessStatus::Ready, "可用");
        let owned: DiagnosticCheck = check.clone().into();
        let borrowed: DiagnosticCheck = (&check).into();
        assert_eq!(owned, borrowed);
        assert_eq!(owned.id, "vision");
        assert_eq!(owned.status, ReadinessStatus::Ready);
    }

    #[test]
    fn report_serialization_uses_snake_case_status() {
        let report = ReadinessReport::aggregate(vec![
            ready(ReadinessCheckId::Backend),
            ReadinessCheck::new(
                ReadinessCheckId::AiProvider,
                ReadinessStatus::NotConfigured,
                "未配置",
            ),
        ]);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"overall\":\"ready\""));
        assert!(json.contains("\"id\":\"ai_provider\""));
        assert!(json.contains("\"status\":\"not_configured\""));
    }
}
