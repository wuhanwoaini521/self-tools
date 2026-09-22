//! Multi-Agent 编排核心契约（V9 Gate 1，§17-§36）。
//!
//! 三条铁律对应 V9 Plan §2：
//! 1. **Agent != Business Module**（§24）：Agent 只代表工作角色 / 任务策略 /
//!    能力权限，不重新实现任何业务能力——能力仍在 `ToolRegistry`；
//! 2. **Least Privilege**（§3-§5）：子 Agent 的 capability 是
//!    `parent ∩ profile ∩ task` 的子集，默认 READ only；
//! 3. **Bounded**（§6/§48/§49）：depth = 1、max agents、max steps/tokens、
//!    timeout 全部在契约里显式表达，不靠约定。
//!
//! 本模块是**纯数据 + 纯函数**（可单测、可序列化）；运行时在
//! `application::agents`。

pub mod budget;
pub mod decision;
pub mod descriptor;
pub mod result;
pub mod task;

pub use budget::{AgentBudget, BudgetUsage, BudgetVerdict, can_start_agent, check_budget, child_budget};
pub use decision::{
    BudgetTier, DECISION_MESSAGE_MAX_CHARS, DecisionConfidence, DecisionEngine, DecisionMode,
    DecisionProvider, DecisionProviderError, DecisionRequest, DecisionResult, DecisionShadowRecord,
    DecisionStrategy, DecisionTelemetry, decision_timeout,
};
pub use descriptor::{AgentDescriptor, AgentRegistry, AgentRole, DelegatedCapabilitySet};
pub use result::{
    ActionProposal, ActionRisk, DelegationResult, DelegationStatus, ReviewFinding, ReviewVerdict,
    TokenUsage, ToolCallRecord,
};
pub use task::{AgentRunState, TaskEnvelope, TaskPriority};

/// task / trace id 校验（与 V6 `is_valid_id` 同风格：契约层封死注入）。
#[must_use]
pub fn is_valid_task_id(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 72 || trimmed != raw {
        return false;
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap_or('_');
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}
