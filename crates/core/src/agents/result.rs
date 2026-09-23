//! 委派结果与 Review（V9 §29/§66-§70/§89）。
//!
//! 设计要点：
//! - **结构化优先**（§98）：`structured_output` 是 JSON，不靠大段自然语言互传；
//! - **Worker 输出 = untrusted**（§97）：`DelegationResult` 只在通过 schema 校验后
//!   才被 orchestrator 信任；`raw_text` 保留给 trace/调试，不进 prompt 拼装；
//! - **子 Agent 唯一写路径**（§89）：`ActionProposal`——worker 不能执行，只能提议。

use serde::{Deserialize, Serialize};

use super::task::AgentRunState;

/// 委派结果状态（§29/§30）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    #[default]
    Pending,
    Completed,
    /// 部分完成（§61：其它 worker 仍可用，最终回答需说明）。
    Partial,
    Failed,
    Cancelled,
    TimedOut,
}

impl DelegationStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DelegationStatus::Pending => "pending",
            DelegationStatus::Completed => "completed",
            DelegationStatus::Partial => "partial",
            DelegationStatus::Failed => "failed",
            DelegationStatus::Cancelled => "cancelled",
            DelegationStatus::TimedOut => "timed_out",
        }
    }

    /// run 状态 → 委派结果状态。
    #[must_use]
    pub fn from_run(state: AgentRunState) -> Self {
        match state {
            AgentRunState::Completed => DelegationStatus::Completed,
            AgentRunState::Failed => DelegationStatus::Failed,
            AgentRunState::Cancelled => DelegationStatus::Cancelled,
            AgentRunState::TimedOut => DelegationStatus::TimedOut,
            AgentRunState::Pending | AgentRunState::Running => DelegationStatus::Pending,
        }
    }

    #[must_use]
    pub fn is_usable(&self) -> bool {
        matches!(
            self,
            DelegationStatus::Completed | DelegationStatus::Partial
        )
    }
}

/// 工具调用记录（§29/§72：trace 用；只记名与结果，不记参数全文）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool: String,
    pub ok: bool,
    pub duration_ms: u64,
    /// 稳定错误码（失败时；不含内容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Token 用量（§55：复用 ChatModelProvider usage 的累计形状）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    #[must_use]
    pub fn combine(self, other: TokenUsage) -> Self {
        Self {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            total_tokens: self.total_tokens.saturating_add(other.total_tokens),
        }
    }
}

/// 委派结果（§29）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DelegationResult {
    pub task_id: String,
    /// 执行该任务的 agent id（§70 provenance）。
    pub agent_id: String,
    pub status: DelegationStatus,
    /// 结构化输出（§98：优先；merged / review 只信这个）。
    pub structured_output: serde_json::Value,
    /// 一句话摘要（给人看；JSON 的 `summary` 字段是机器契约）。
    #[serde(default)]
    pub summary: String,
    /// 来源（§69：source_id / path / url 等引用，不是正文）。
    #[serde(default)]
    pub sources: Vec<String>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub usage: TokenUsage,
    pub duration_ms: u64,
    /// 稳定错误码（失败/超时/取消）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<String>,
}

impl DelegationResult {
    /// 失败结果的便捷构造（§60）。
    #[must_use]
    pub fn failed(task_id: &str, agent_id: &str, reason: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            status: DelegationStatus::Failed,
            structured_output: serde_json::json!({"ok": false}),
            summary: String::new(),
            sources: Vec::new(),
            tool_calls: Vec::new(),
            usage: TokenUsage::default(),
            duration_ms: 0,
            errors: Some(reason.to_string()),
        }
    }

    /// 供 prompt 使用的**脱氧**视图（§97：只给结构化输出 + 摘要 + 来源）。
    #[must_use]
    pub fn trusted_view(&self) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "agent_id": self.agent_id,
            "status": self.status.as_str(),
            "output": self.structured_output,
            "summary": self.summary,
            "sources": self.sources,
        })
    }
}

/// 子 Agent 的行动提议（§89）：worker 无法执行，只能提议。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionProposal {
    /// 提议的动作类型（与 SafeAction 的 `RegisteredAction` 对齐，如 `services.restart`）。
    pub action_type: String,
    /// 目标 id（必须是已注册对象）。
    pub target_id: String,
    pub summary: String,
    /// 提议的风险等级。
    pub risk: ActionRisk,
    /// 为什么需要（reviewer / 用户判断依据）。
    #[serde(default)]
    pub rationale: String,
}

/// 提议风险（与 `ToolRisk` 对应的轻量枚举；worker 只到 SafeWrite/System 提议级）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRisk {
    #[default]
    Read,
    SafeWrite,
    System,
}

impl ActionRisk {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ActionRisk::Read => "read",
            ActionRisk::SafeWrite => "safe_write",
            ActionRisk::System => "system",
        }
    }

    /// 是否需要用户确认（§90：提议 → parent → SafeAction）。
    #[must_use]
    pub fn requires_confirmation(self) -> bool {
        !matches!(self, ActionRisk::Read)
    }
}

/// Review 结论（§66）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    #[default]
    Pass,
    NeedsFix,
    UnsupportedClaims,
    MissingEvidence,
    Contradiction,
}

impl ReviewVerdict {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewVerdict::Pass => "pass",
            ReviewVerdict::NeedsFix => "needs_fix",
            ReviewVerdict::UnsupportedClaims => "unsupported_claims",
            ReviewVerdict::MissingEvidence => "missing_evidence",
            ReviewVerdict::Contradiction => "contradiction",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "pass" => Some(ReviewVerdict::Pass),
            "needs_fix" => Some(ReviewVerdict::NeedsFix),
            "unsupported_claims" => Some(ReviewVerdict::UnsupportedClaims),
            "missing_evidence" => Some(ReviewVerdict::MissingEvidence),
            "contradiction" => Some(ReviewVerdict::Contradiction),
            _ => None,
        }
    }

    /// 是否允许一次 repair（§68：仅 NEEDS_FIX 触发，且全局最多 1 次）。
    #[must_use]
    pub fn allows_repair(self) -> bool {
        matches!(self, ReviewVerdict::NeedsFix)
    }
}

/// Review 发现（§65/§66：reviewer 的结构化输出）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub verdict: ReviewVerdict,
    /// 具体问题（指向 task_id / source，不含正文）。
    #[serde(default)]
    pub issues: Vec<String>,
    /// 复核通过的任务（provenance）。
    #[serde(default)]
    pub supported: Vec<String>,
}

impl ReviewFinding {
    /// 从模型 JSON 解析（§98：结构化校验；失败 → fail-closed 到 NEEDS_FIX）。
    #[must_use]
    pub fn from_json(value: &serde_json::Value) -> Self {
        let verdict = value
            .get("verdict")
            .and_then(serde_json::Value::as_str)
            .and_then(ReviewVerdict::parse)
            .unwrap_or(ReviewVerdict::NeedsFix);
        let issues = value
            .get("issues")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let supported = value
            .get("supported")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            verdict,
            issues,
            supported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_status_maps_from_run_state() {
        assert_eq!(
            DelegationStatus::from_run(AgentRunState::Completed),
            DelegationStatus::Completed
        );
        assert_eq!(
            DelegationStatus::from_run(AgentRunState::TimedOut),
            DelegationStatus::TimedOut
        );
        assert!(
            !DelegationStatus::from_run(AgentRunState::Running).is_usable(),
            "未完成的结果不可用"
        );
        assert!(DelegationStatus::Partial.is_usable(), "PARTIAL 仍可用");
    }

    #[test]
    fn trusted_view_strips_untrusted_fields() {
        let result = DelegationResult {
            task_id: "t-1".into(),
            agent_id: "research".into(),
            status: DelegationStatus::Completed,
            structured_output: serde_json::json!({"findings": ["a"]}),
            summary: "找到 1 条".into(),
            sources: vec!["service:self-tools".into()],
            tool_calls: vec![ToolCallRecord {
                tool: "services.get_logs".into(),
                ok: true,
                duration_ms: 12,
                error_code: None,
            }],
            usage: TokenUsage::default(),
            duration_ms: 30,
            errors: None,
        };
        let view = result.trusted_view();
        assert_eq!(view["agent_id"], "research");
        assert_eq!(view["output"]["findings"][0], "a");
        // §97：工具调用细节与用量不进 prompt 视图。
        assert!(view.get("tool_calls").is_none());
        assert!(view.get("usage").is_none());
        assert!(view.get("errors").is_none());
    }

    #[test]
    fn action_proposal_requires_confirmation_for_writes() {
        assert!(!ActionRisk::Read.requires_confirmation());
        assert!(ActionRisk::SafeWrite.requires_confirmation());
        assert!(ActionRisk::System.requires_confirmation());
    }

    #[test]
    fn review_verdict_round_trip_and_repair_policy() {
        for verdict in [
            ReviewVerdict::Pass,
            ReviewVerdict::NeedsFix,
            ReviewVerdict::UnsupportedClaims,
            ReviewVerdict::MissingEvidence,
            ReviewVerdict::Contradiction,
        ] {
            assert_eq!(ReviewVerdict::parse(verdict.as_str()), Some(verdict));
        }
        assert!(ReviewVerdict::NeedsFix.allows_repair());
        assert!(!ReviewVerdict::Pass.allows_repair());
        assert!(
            !ReviewVerdict::Contradiction.allows_repair(),
            "§68：只有 NEEDS_FIX 允许 repair"
        );
    }

    #[test]
    fn review_finding_fails_closed_on_bad_json() {
        // §98：结构校验失败 → NEEDS_FIX（不放行）。
        let finding = ReviewFinding::from_json(&serde_json::json!({"nonsense": true}));
        assert_eq!(finding.verdict, ReviewVerdict::NeedsFix);

        let ok = ReviewFinding::from_json(&serde_json::json!({
            "verdict": "pass",
            "supported": ["t-1"],
        }));
        assert_eq!(ok.verdict, ReviewVerdict::Pass);
        assert_eq!(ok.supported, vec!["t-1".to_string()]);
    }

    #[test]
    fn token_usage_accumulates() {
        let left = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
        };
        let right = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
        };
        assert_eq!(left.combine(right).total_tokens, 17);
    }
}
