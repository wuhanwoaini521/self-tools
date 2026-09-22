//! 编排预算（V9 §52-§57）。
//!
//! 每个用户请求拥有一个全局 `AgentBudget`；child 的消耗从 parent 扣减，
//! 因此 **child 总和永远不可能超过 parent**（§54）。
//! 预算检查是纯函数（可单测）；扣减由 runtime 串行执行（唤醒顺序确定）。

use serde::{Deserialize, Serialize};

/// Agent 预算（§52/§53）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentBudget {
    /// 本次请求最多启动多少个 agent run（含 parent 自己）。
    pub max_agents: usize,
    /// 所有 run 的 tool 轮数上限总和。
    pub max_steps: usize,
    /// 所有 run 的工具调用次数上限总和。
    pub max_tool_calls: usize,
    /// 所有 run 的 token 上限总和（输入+输出）。
    pub max_tokens: u32,
    /// 整个编排的墙钟上限（毫秒）。
    pub max_duration_ms: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_agents: 4,
            max_steps: 16,
            max_tool_calls: 24,
            max_tokens: 60_000,
            max_duration_ms: 120_000,
        }
    }
}

/// 已消耗量（与预算同Shape；`spent` 永远 ≤ budget）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub agents: usize,
    pub steps: usize,
    pub tool_calls: usize,
    pub tokens: u32,
    pub elapsed_ms: u64,
}

impl BudgetUsage {
    #[must_use]
    pub fn combine(self, other: BudgetUsage) -> Self {
        Self {
            agents: self.agents.saturating_add(other.agents),
            steps: self.steps.saturating_add(other.steps),
            tool_calls: self.tool_calls.saturating_add(other.tool_calls),
            tokens: self.tokens.saturating_add(other.tokens),
            elapsed_ms: self.elapsed_ms.max(other.elapsed_ms),
        }
    }
}

/// 预算裁决（§60：达到上限时**安全停止**，不是 panic）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetVerdict {
    /// 仍在预算内。
    Within,
    /// 超出上限：附带稳定原因码。
    Exceeded(&'static str),
}

impl BudgetVerdict {
    #[must_use]
    pub fn is_within(&self) -> bool {
        matches!(self, BudgetVerdict::Within)
    }

    #[must_use]
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            BudgetVerdict::Within => None,
            BudgetVerdict::Exceeded(reason) => Some(reason),
        }
    }
}

/// 纯预算检查（§120 的测试面）。
#[must_use]
pub fn check_budget(budget: &AgentBudget, used: &BudgetUsage) -> BudgetVerdict {
    if used.agents > budget.max_agents {
        return BudgetVerdict::Exceeded("max_agents");
    }
    if used.steps > budget.max_steps {
        return BudgetVerdict::Exceeded("max_steps");
    }
    if used.tool_calls > budget.max_tool_calls {
        return BudgetVerdict::Exceeded("max_tool_calls");
    }
    if used.tokens > budget.max_tokens {
        return BudgetVerdict::Exceeded("max_tokens");
    }
    if used.elapsed_ms > budget.max_duration_ms {
        return BudgetVerdict::Exceeded("max_duration");
    }
    BudgetVerdict::Within
}

/// 还能再启动一个 agent 吗（§50 有界并发的判定输入）。
#[must_use]
pub fn can_start_agent(budget: &AgentBudget, used: &BudgetUsage) -> bool {
    used.agents < budget.max_agents && check_budget(budget, used).is_within()
}

/// 子预算：从父预算派生（§54）。
///
/// child 的各维度上限 = min(profile 上限, 父剩余)；父剩余为 0 → 子预算为 0
/// （调用方据此直接拒绝，而不是启动一个必然超限的 run）。
#[must_use]
pub fn child_budget(
    budget: &AgentBudget,
    used: &BudgetUsage,
    max_steps: usize,
    max_tokens: u32,
    timeout_ms: u64,
    now_ms: u64,
) -> AgentBudget {
    let remaining_steps = budget.max_steps.saturating_sub(used.steps);
    let remaining_tokens = budget.max_tokens.saturating_sub(used.tokens);
    let remaining_ms = budget.max_duration_ms.saturating_sub(used.elapsed_ms);
    AgentBudget {
        // child 自身只占 1 个 agent 名额；剩余名额留给兄弟。
        max_agents: 1,
        max_steps: max_steps.min(remaining_steps),
        max_tokens: max_tokens.min(remaining_tokens),
        max_tool_calls: budget.max_tool_calls.saturating_sub(used.tool_calls),
        max_duration_ms: timeout_ms.min(remaining_ms).max(now_ms.min(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_budget_is_within() {
        assert!(check_budget(&AgentBudget::default(), &BudgetUsage::default()).is_within());
    }

    #[test]
    fn each_dimension_has_a_stable_reason() {
        let budget = AgentBudget::default();
        for (used, reason) in [
            (
                BudgetUsage { agents: 5, ..BudgetUsage::default() },
                "max_agents",
            ),
            (
                BudgetUsage { steps: 17, ..BudgetUsage::default() },
                "max_steps",
            ),
            (
                BudgetUsage { tool_calls: 25, ..BudgetUsage::default() },
                "max_tool_calls",
            ),
            (
                BudgetUsage { tokens: 60_001, ..BudgetUsage::default() },
                "max_tokens",
            ),
            (
                BudgetUsage { elapsed_ms: 120_001, ..BudgetUsage::default() },
                "max_duration",
            ),
        ] {
            let verdict = check_budget(&budget, &used);
            assert_eq!(verdict.reason(), Some(reason), "{used:?}");
        }
    }

    #[test]
    fn boundary_is_inclusive() {
        let budget = AgentBudget::default();
        let at_limit = BudgetUsage {
            agents: 4,
            steps: 16,
            tool_calls: 24,
            tokens: 60_000,
            elapsed_ms: 120_000,
        };
        assert!(check_budget(&budget, &at_limit).is_within(), "恰好用满仍算在内");
        assert!(!can_start_agent(&budget, &at_limit), "名额已满不得再启动");
    }

    #[test]
    fn child_budget_never_exceeds_parent_remaining() {
        let budget = AgentBudget::default();
        let used = BudgetUsage {
            steps: 10,
            tokens: 50_000,
            elapsed_ms: 100_000,
            ..BudgetUsage::default()
        };
        let child = child_budget(&budget, &used, 8, 20_000, 30_000, 0);
        assert_eq!(child.max_steps, 6, "父剩余 6 < profile 8");
        assert_eq!(child.max_tokens, 10_000, "父剩余 10k < profile 20k");
        assert_eq!(child.max_duration_ms, 20_000, "父剩余 20s < profile 30s");
        assert_eq!(child.max_agents, 1);

        // 父已用尽 → 子预算归零（调用方拒绝启动）。
        let exhausted = child_budget(&budget, &BudgetUsage {
            steps: 16,
            tokens: 60_000,
            elapsed_ms: 120_000,
            ..BudgetUsage::default()
        }, 8, 20_000, 30_000, 0);
        assert_eq!(exhausted.max_steps, 0);
        assert_eq!(exhausted.max_tokens, 0);
    }

    #[test]
    fn usage_combine_sums_and_takes_max_elapsed() {
        let left = BudgetUsage { agents: 1, steps: 2, elapsed_ms: 30, ..Default::default() };
        let right = BudgetUsage { agents: 1, steps: 3, elapsed_ms: 50, ..Default::default() };
        let combined = left.combine(right);
        assert_eq!(combined.agents, 2);
        assert_eq!(combined.steps, 5);
        assert_eq!(combined.elapsed_ms, 50, "墙钟取 max（并行）");
    }
}
