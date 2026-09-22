//! 标准 Worker Profiles（V9 §19-§23）。
//!
//! **只注册已知 profile**（§108）：模型不能创建或修改 agent 类型。
//! 禁止 Domain Agent（§24）：History/Travel/Files/Server 都是 **Tools**，
//! 不是 Agent。

use devtoolbox_core::agents::{AgentDescriptor, AgentRegistry, AgentRole};
use devtoolbox_core::personal_ai::ToolRisk;

/// 检索 worker（§20）：READ only。
#[must_use]
pub fn research_profile() -> AgentDescriptor {
    AgentDescriptor {
        id: "research".into(),
        role: AgentRole::Research,
        description: "检索 / 取证 / 比较来源 / 收集证据（只读）".into(),
        model_profile: "fast".into(),
        max_risk: ToolRisk::Read,
        // §3/§A1：最小权限 —— 只读域 + 只读状态类；写 / open / restart 需 task 显式申请。
        allowed_modules: vec![
            "memory".into(),
            "documents".into(),
            "files".into(),
            "knowledge".into(),
            "history".into(),
            "travel".into(),
            "geography".into(),
            "language".into(),
            "server".into(),
        ],
        // §93：长期记忆写入不给 worker；打开 / 重启等敏感入口显式排除。
        denied_tools: vec![
            "memory.save".into(),
            "memory.archive".into(),
            "memory.update".into(),
            "files.open".into(),
            "apps.open".into(),
            "services.restart".into(),
        ],
        max_steps: 4,
        max_tokens: 8_000,
        timeout_ms: 60_000,
        can_delegate: false,
    }
}

/// 规划 worker（§21）：无业务写权限。
#[must_use]
pub fn planner_profile() -> AgentDescriptor {
    AgentDescriptor {
        id: "planner".into(),
        role: AgentRole::Planner,
        description: "任务分解 / 排序 / 依赖识别（只读推理，不调业务写）".into(),
        model_profile: "balanced".into(),
        max_risk: ToolRisk::Read,
        allowed_modules: Vec::new(),
        denied_tools: vec![
            "memory.save".into(),
            "memory.archive".into(),
            "files.open".into(),
            "apps.open".into(),
            "services.restart".into(),
        ],
        max_steps: 2,
        max_tokens: 6_000,
        timeout_ms: 45_000,
        can_delegate: false,
    }
}

/// 审查 worker（§22）：与 producer 分离；READ only。
#[must_use]
pub fn reviewer_profile() -> AgentDescriptor {
    AgentDescriptor {
        id: "reviewer".into(),
        role: AgentRole::Reviewer,
        description: "证据核查 / 矛盾检测 / 完整性 / 无依据结论（只读）".into(),
        model_profile: "strong".into(),
        max_risk: ToolRisk::Read,
        allowed_modules: Vec::new(),
        denied_tools: vec![
            "memory.save".into(),
            "memory.archive".into(),
            "files.open".into(),
            "apps.open".into(),
            "services.restart".into(),
        ],
        max_steps: 3,
        max_tokens: 8_000,
        timeout_ms: 60_000,
        can_delegate: false,
    }
}

/// 综合 worker（§23，P1）。
#[must_use]
pub fn synthesizer_profile() -> AgentDescriptor {
    AgentDescriptor {
        id: "synthesizer".into(),
        role: AgentRole::Synthesizer,
        description: "合并多个 worker 输出 / 去重 / 结构化结果（P1）".into(),
        model_profile: "balanced".into(),
        max_risk: ToolRisk::Read,
        allowed_modules: Vec::new(),
        denied_tools: vec![
            "memory.save".into(),
            "files.open".into(),
            "apps.open".into(),
            "services.restart".into(),
        ],
        max_steps: 2,
        max_tokens: 8_000,
        timeout_ms: 45_000,
        can_delegate: false,
    }
}

/// 默认注册表（research / planner / reviewer；synthesizer 可选）。
#[must_use]
pub fn default_registry() -> AgentRegistry {
    let mut registry = AgentRegistry::new();
    registry.register(research_profile());
    registry.register(planner_profile());
    registry.register(reviewer_profile());
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_three_workers() {
        let registry = default_registry();
        assert_eq!(registry.ids(), vec!["planner".to_string(), "research".to_string(), "reviewer".to_string()]);
    }

    #[test]
    fn every_profile_is_read_only_and_cannot_delegate() {
        for profile in [research_profile(), planner_profile(), reviewer_profile(), synthesizer_profile()] {
            assert!(profile.is_read_only(), "{} 必须 READ only", profile.id);
            assert!(!profile.can_delegate, "{} 不允许再委派（depth=1）", profile.id);
        }
    }

    #[test]
    fn sensitive_entries_are_denied_for_all_workers() {
        // §93：子 Agent 禁止 memory.save。
        for profile in [research_profile(), planner_profile(), reviewer_profile(), synthesizer_profile()] {
            assert!(
                profile.denied_tools.iter().any(|tool| tool == "memory.save"),
                "{} 必须显式拒绝 memory.save",
                profile.id
            );
            assert!(
                profile.denied_tools.iter().any(|tool| tool == "services.restart"),
                "{} 必须显式拒绝 services.restart（§A1）",
                profile.id
            );
        }
    }

    #[test]
    fn profiles_are_not_domain_agents() {
        // §24：不存在 HistoryAgent / TravelAgent / ServerAgent。
        let registry = default_registry();
        for id in registry.ids() {
            assert!(
                !id.contains("history") && !id.contains("travel") && !id.contains("server"),
                "不得注册 Domain Agent: {id}"
            );
        }
    }

    #[test]
    fn reviewer_uses_stronger_profile_than_research() {
        // §83：不同角色可用不同 model profile。
        assert_eq!(research_profile().model_profile, "fast");
        assert_eq!(reviewer_profile().model_profile, "strong");
    }
}
