//! 任务信封（V9 §25-§33）。
//!
//! 内部 agent 间协议（§99：不用 MCP A2A）。子任务只获得**必要** context
//! （§26/§27）：优先传 ID/reference，worker 需要时通过 Tool retrieve ——
//! 不下发整个会话 / 全 Memory / 全 Documents。

use serde::{Deserialize, Serialize};

use super::descriptor::DelegatedCapabilitySet;

/// 任务优先级（§25）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Agent run 状态（§30）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunState {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl AgentRunState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AgentRunState::Pending => "pending",
            AgentRunState::Running => "running",
            AgentRunState::Completed => "completed",
            AgentRunState::Failed => "failed",
            AgentRunState::Cancelled => "cancelled",
            AgentRunState::TimedOut => "timed_out",
        }
    }

    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            AgentRunState::Completed | AgentRunState::Failed | AgentRunState::Cancelled | AgentRunState::TimedOut
        )
    }
}

/// 任务信封（§25）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskEnvelope {
    pub task_id: String,
    /// 父任务 id（顶层 = 用户请求 id；worker 不允许再派生 → 恒为顶层，§49）。
    pub parent_task_id: String,
    /// 目标（一句话，可判定完成）。
    pub objective: String,
    /// 详细指令（角色化提示的 task 部分）。
    pub instructions: String,
    /// context 引用（§27：ID / 短引用，不是大文本）。
    #[serde(default)]
    pub context_refs: Vec<String>,
    /// 任务要求的工具（空 = 用 capability 全集）。
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// 委派能力集（intersect 结果）。
    pub capabilities: DelegatedCapabilitySet,
    pub max_steps: usize,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    /// 绝对截止（Unix 秒；0 = 无）。
    #[serde(default)]
    pub deadline: i64,
    /// 期望输出 schema（§98：优先结构化）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    pub priority: TaskPriority,
    /// 追踪 id（§71/§80）。
    pub trace_id: String,
}

impl TaskEnvelope {
    /// 结构化自校验（模型/编排器产出的唯一入口）。
    ///
    /// 返回稳定错误码（不回显内容），与 V6/V7 的 `is_valid_id` 同风格。
    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::agents::is_valid_task_id(&self.task_id) {
            return Err("invalid_task_id");
        }
        if !crate::agents::is_valid_task_id(&self.parent_task_id) {
            return Err("invalid_parent_task_id");
        }
        if self.objective.trim().is_empty() {
            return Err("empty_objective");
        }
        if self.max_steps == 0 || self.max_tokens == 0 || self.timeout_ms == 0 {
            return Err("invalid_budget");
        }
        if !self.capabilities.is_subset_of(&self.capabilities.allowed_tools) {
            // 恒真；保留断言形状以便将来 parent 传入时校验。
            return Err("capability_not_subset");
        }
        Ok(())
    }

    /// 是否已过绝对截止。
    #[must_use]
    pub fn is_past_deadline(&self, now: i64) -> bool {
        self.deadline > 0 && now >= self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::descriptor::DelegatedCapabilitySet;

    fn envelope() -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".into(),
            parent_task_id: "req-1".into(),
            objective: "收集服务器不稳定证据".into(),
            instructions: "检索日志与文档，列出证据".into(),
            context_refs: vec!["service:self-tools".into()],
            required_tools: vec!["services.get_logs".into()],
            capabilities: DelegatedCapabilitySet {
                allowed_tools: vec!["services.get_logs".into()],
                denied_tools: Vec::new(),
            },
            max_steps: 4,
            max_tokens: 8_000,
            timeout_ms: 60_000,
            deadline: 0,
            output_schema: Some(serde_json::json!({"type": "object"})),
            priority: TaskPriority::Normal,
            trace_id: "trace-1".into(),
        }
    }

    #[test]
    fn valid_envelope_passes() {
        assert!(envelope().validate().is_ok());
    }

    #[test]
    fn invalid_envelopes_are_rejected_with_stable_codes() {
        let mut bad = envelope();
        bad.task_id = "task; rm -rf /".into();
        assert_eq!(bad.validate(), Err("invalid_task_id"));

        let mut bad = envelope();
        bad.objective = "   ".into();
        assert_eq!(bad.validate(), Err("empty_objective"));

        let mut bad = envelope();
        bad.max_steps = 0;
        assert_eq!(bad.validate(), Err("invalid_budget"));
    }

    #[test]
    fn deadline_is_optional_and_monotonic() {
        let mut task = envelope();
        assert!(!task.is_past_deadline(1_000), "deadline = 0 表示无限制");
        task.deadline = 100;
        assert!(!task.is_past_deadline(99));
        assert!(task.is_past_deadline(100));
    }

    #[test]
    fn terminal_states_are_recognized() {
        assert!(AgentRunState::Completed.is_terminal());
        assert!(AgentRunState::TimedOut.is_terminal());
        assert!(!AgentRunState::Running.is_terminal());
        assert!(!AgentRunState::Pending.is_terminal());
        assert_eq!(AgentRunState::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn task_id_validation_blocks_injection() {
        assert!(crate::agents::is_valid_task_id("task-1"));
        assert!(crate::agents::is_valid_task_id("req-2026-09-22-abc"));
        assert!(!crate::agents::is_valid_task_id(""));
        assert!(!crate::agents::is_valid_task_id("a b"));
        assert!(!crate::agents::is_valid_task_id("../etc"));
        assert!(!crate::agents::is_valid_task_id(&"x".repeat(80)));
    }
}
