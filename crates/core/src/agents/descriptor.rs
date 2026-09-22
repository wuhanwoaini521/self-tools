//! Agent 角色与描述符（V9 §17-§24/§34-§36）。

use serde::{Deserialize, Serialize};

use crate::personal_ai::{ToolRisk, ToolSpec};

/// Agent 工作角色（§19：第一版 3+1，不建 Domain Agent）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// 检索 / 取证 / 比较（§20）：默认 READ only。
    #[default]
    Research,
    /// 任务分解 / 排序 / 依赖识别（§21）。
    Planner,
    /// 证据核查 / 矛盾检测 / 完整性（§22）：与 producer 分离。
    Reviewer,
    /// 合并多个 worker 输出（§23，P1）。
    Synthesizer,
}

impl AgentRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AgentRole::Research => "research",
            AgentRole::Planner => "planner",
            AgentRole::Reviewer => "reviewer",
            AgentRole::Synthesizer => "synthesizer",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "research" => Some(AgentRole::Research),
            "planner" => Some(AgentRole::Planner),
            "reviewer" => Some(AgentRole::Reviewer),
            "synthesizer" => Some(AgentRole::Synthesizer),
            _ => None,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AgentRole::Research => "检索",
            AgentRole::Planner => "规划",
            AgentRole::Reviewer => "审查",
            AgentRole::Synthesizer => "综合",
        }
    }
}

/// Agent 描述符（§18）。
///
/// **不可由模型修改或创建**（§107/§108）：只能静态注册进 `AgentRegistry`。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentDescriptor {
    /// 稳定 id（`research` / `planner` / `reviewer` / `synthesizer`）。
    pub id: String,
    pub role: AgentRole,
    pub description: String,
    /// 模型画像（§83/§84）：`fast` / `balanced` / `strong`；映射到 Settings，
    /// 未配置时回落到默认 `ChatModelProvider`（§85）。
    pub model_profile: String,
    /// 该角色允许的最高风险（§4/§88：默认 `Read`）。
    pub max_risk: ToolRisk,
    /// 允许的工具名前缀（模块名，如 `history` / `knowledge`）；
    /// 空 = 只受 `max_risk` 约束。
    #[serde(default)]
    pub allowed_modules: Vec<String>,
    /// 显式拒绝的工具名（§25 `denied_tools` 的静态部分）。
    #[serde(default)]
    pub denied_tools: Vec<String>,
    /// 单 run 最大工具轮数。
    pub max_steps: usize,
    /// 单 run 最大 token（输入+输出）。
    pub max_tokens: u32,
    /// 单 run 超时（毫秒）。
    pub timeout_ms: u64,
    /// V9 恒 false（§48/§49：depth = 1，worker 不能再委派）。
    pub can_delegate: bool,
}

impl AgentDescriptor {
    /// 该角色默认是否 READ only。
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        matches!(self.max_risk, ToolRisk::Read)
    }

    /// 工具是否被此 descriptor 允许（**静态**层：risk + module + denied）。
    #[must_use]
    pub fn allows_tool(&self, spec: &ToolSpec) -> bool {
        if self.denied_tools.iter().any(|name| name == &spec.name) {
            return false;
        }
        if risk_rank(spec.risk) > risk_rank(self.max_risk) {
            return false;
        }
        if self.allowed_modules.is_empty() {
            return true;
        }
        self.allowed_modules
            .iter()
            .any(|module| module == &spec.module)
    }
}

/// 风险序（用于「不超过 max_risk」比较）。
#[must_use]
pub fn risk_rank(risk: ToolRisk) -> u8 {
    match risk {
        ToolRisk::Read => 0,
        ToolRisk::SafeWrite => 1,
        ToolRisk::SensitiveWrite => 2,
        ToolRisk::System => 3,
    }
}

/// 委派能力集（§34-§36）：parent ∩ profile ∩ task 的结果。
///
/// **不变量**（§36）：`child ⊆ parent` —— 由 `intersect` 的结构保证，
/// 并由 `assert_subset_of` 在运行时双重校验。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DelegatedCapabilitySet {
    /// 允许执行的工具名（已过滤）。
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// 显式拒绝的工具名（优先于 allowed）。
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl DelegatedCapabilitySet {
    /// 三源交集（§35）。
    #[must_use]
    pub fn intersect(
        parent: &[String],
        profile_allows: impl Fn(&str) -> bool,
        required: &[String],
    ) -> Self {
        let required_non_empty = !required.is_empty();
        let allowed: Vec<String> = parent
            .iter()
            .filter(|name| profile_allows(name))
            .filter(|name| !required_non_empty || required.iter().any(|want| want == *name))
            .cloned()
            .collect();
        Self {
            allowed_tools: allowed,
            denied_tools: Vec::new(),
        }
    }

    /// 追加显式拒绝（§25 `denied_tools`）。
    #[must_use]
    pub fn with_denied(mut self, denied: Vec<String>) -> Self {
        self.denied_tools = denied;
        self.allowed_tools
            .retain(|name| !self.denied_tools.iter().any(|blocked| blocked == name));
        self
    }

    /// 工具是否在能力集内。
    #[must_use]
    pub fn allows(&self, tool_name: &str) -> bool {
        if self.denied_tools.iter().any(|blocked| blocked == tool_name) {
            return false;
        }
        self.allowed_tools.iter().any(|name| name == tool_name)
    }

    /// §36 不变量：child ⊆ parent。违反即 fail-closed（返回 false 由调用方拒绝）。
    #[must_use]
    pub fn is_subset_of(&self, parent: &[String]) -> bool {
        self.allowed_tools
            .iter()
            .all(|name| parent.iter().any(|have| have == name))
    }
}

/// Agent 注册表（§17：与 ModuleRegistry/ToolRegistry 同构）。
#[derive(Debug, Default)]
pub struct AgentRegistry {
    agents: Vec<AgentDescriptor>,
}

impl AgentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 profile（重复 id → 替换；保持装配简单）。
    pub fn register(&mut self, descriptor: AgentDescriptor) {
        self.agents.retain(|agent| agent.id != descriptor.id);
        self.agents.push(descriptor);
    }

    #[must_use]
    pub fn get(&self, agent_id: &str) -> Option<&AgentDescriptor> {
        self.agents.iter().find(|agent| agent.id == agent_id)
    }

    /// 全部 profile（顺序稳定）。
    #[must_use]
    pub fn descriptors(&self) -> Vec<AgentDescriptor> {
        let mut list = self.agents.clone();
        list.sort_by(|left, right| left.id.cmp(&right.id));
        list
    }

    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.descriptors().into_iter().map(|agent| agent.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::ToolSpec;

    fn spec(name: &str, risk: ToolRisk, module: &str) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: "t".into(),
            input_schema: serde_json::json!({"type": "object"}),
            risk,
            module: module.into(),
        }
    }

    fn research_profile() -> AgentDescriptor {
        AgentDescriptor {
            id: "research".into(),
            role: AgentRole::Research,
            description: "检索取证".into(),
            model_profile: "fast".into(),
            max_risk: ToolRisk::Read,
            allowed_modules: Vec::new(),
            denied_tools: vec!["memory.save".into()],
            max_steps: 4,
            max_tokens: 8_000,
            timeout_ms: 60_000,
            can_delegate: false,
        }
    }

    #[test]
    fn role_round_trips() {
        for role in [
            AgentRole::Research,
            AgentRole::Planner,
            AgentRole::Reviewer,
            AgentRole::Synthesizer,
        ] {
            assert_eq!(AgentRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(AgentRole::parse("domain-agent"), None);
    }

    #[test]
    fn descriptor_blocks_write_and_denied_tools() {
        let profile = research_profile();
        assert!(profile.is_read_only());
        // READ 工具允许。
        assert!(profile.allows_tool(&spec("history.search", ToolRisk::Read, "history")));
        // SafeWrite 超出 max_risk → 拒。
        assert!(!profile.allows_tool(&spec("memory.save", ToolRisk::SafeWrite, "memory")));
        // 显式 denied → 拒（即使它是 Read）。
        assert!(!profile.allows_tool(&spec("memory.save", ToolRisk::Read, "memory")));
        // 模块白名单。
        let scoped = AgentDescriptor {
            allowed_modules: vec!["history".into()],
            ..research_profile()
        };
        assert!(scoped.allows_tool(&spec("history.search", ToolRisk::Read, "history")));
        assert!(!scoped.allows_tool(&spec("memory.search", ToolRisk::Read, "memory")));
    }

    #[test]
    fn capability_intersection_never_exceeds_parent() {
        // §35/§36：parent ∩ profile ∩ task。
        let parent = vec![
            "history.search".to_string(),
            "memory.search".to_string(),
            "memory.save".to_string(),
        ];
        let profile = research_profile();
        let set = DelegatedCapabilitySet::intersect(
            &parent,
            |name| {
                // 用 descriptor 的静态规则（需要 spec；这里用名字模拟）。
                !name.starts_with("memory.save")
            },
            &["history.search".to_string()],
        );
        assert_eq!(set.allowed_tools, vec!["history.search".to_string()]);
        assert!(set.is_subset_of(&parent), "§36：child ⊆ parent");
        assert!(set.allows("history.search"));
        assert!(!set.allows("memory.search"), "不在 task 要求内");
        let _ = profile;
    }

    #[test]
    fn denied_tools_win_over_allowed() {
        let set = DelegatedCapabilitySet {
            allowed_tools: vec!["a".into(), "b".into()],
            denied_tools: Vec::new(),
        }
        .with_denied(vec!["b".into()]);
        assert!(set.allows("a"));
        assert!(!set.allows("b"));
        assert_eq!(set.allowed_tools, vec!["a".to_string()]);
    }

    #[test]
    fn registry_registers_and_replaces_profiles() {
        let mut registry = AgentRegistry::new();
        registry.register(research_profile());
        registry.register(AgentDescriptor {
            id: "reviewer".into(),
            ..research_profile()
        });
        assert_eq!(registry.ids(), vec!["research".to_string(), "reviewer".to_string()]);
        // 同 id 替换（不重复）。
        registry.register(AgentDescriptor {
            description: "更新".into(),
            ..research_profile()
        });
        assert_eq!(registry.get("research").expect("research").description, "更新");
        assert!(registry.get("planner").is_none());
    }
}
