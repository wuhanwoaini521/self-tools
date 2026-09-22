//! Multi-Agent 应用层（V9 Gates 2-7）。
//!
//! 组成：
//! - `executor`：单次 agent run（load profile → prompt → 共享 tool loop → 结果）；
//! - `orchestrator`：decide → plan → delegate → parallel → review → merge；
//! - `profiles`：research / planner / reviewer（+ synthesizer P1）静态注册表；
//! - `prompt`：`AgentPromptBuilder`（共享 core policy + role + envelope + context
//!   + allowed tools，§110：不复制 4 份巨型 prompt）。
//!
//! 铁律：**能力只能来自 ToolRegistry**；子 Agent capability = parent ∩ profile ∩ task
//! （§34-§36）；worker 不可再委派（depth = 1，§49）。

pub mod decision_engine;
pub mod decision_eval;
pub mod decision_rule;
#[cfg(test)]
pub mod decision_security_tests;
#[cfg(test)]
pub mod failure_injection_tests;
pub mod executor;
pub mod orchestrator;
pub mod profiles;
pub mod prompt;

pub use decision_eval::{DecisionEvalReport, DecisionEvalHarness, golden_decision_cases};
pub use decision_engine::AgentDecisionEngine;
pub use decision_rule::{RuleDecisionProvider, decide_by_rule};
pub use executor::{AgentExecutor, AgentExecutorDeps, RunOutcome};
pub use orchestrator::{
    DelegationDecision, ExecutionPlan, OrchestrationService, OrchestrationTrace, PlanTask,
};
pub use profiles::{default_registry, research_profile, reviewer_profile, planner_profile};
