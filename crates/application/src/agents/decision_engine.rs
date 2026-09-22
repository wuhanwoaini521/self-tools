//! Agent 决策引擎装配（V10 §13/§35）：把 core 的 `DecisionEngine` 与
//! Rule/Jev provider 组合成 `OrchestrationService` 可用的路由入口。
//!
//! **执行边界（§35）**：本类型只做「路由」。执行、授权、工具循环、预算强制
//! 全部留在 `OrchestrationService` / `AgentExecutor`。

use std::sync::Arc;

use devtoolbox_core::agents::{
    DecisionEngine, DecisionMode, DecisionProvider, DecisionTelemetry,
};

/// `OrchestrationService` 使用的决策门面。
#[derive(Clone)]
pub struct AgentDecisionEngine {
    engine: DecisionEngine,
}

impl AgentDecisionEngine {
    /// 仅规则（未配置 Jev 时的默认形态）。
    #[must_use]
    pub fn rule_only() -> Self {
        Self {
            engine: DecisionEngine::rule_only(Arc::new(super::decision_rule::RuleDecisionProvider::new())),
        }
    }

    /// 从 core engine 包装（组合根装配用）。
    #[must_use]
    pub fn from_engine(engine: DecisionEngine) -> Self {
        Self { engine }
    }

    /// 供组合根构造：rule + 可选 model provider + mode。
    #[must_use]
    pub fn new(
        model: Option<Arc<dyn DecisionProvider>>,
        mode: DecisionMode,
        timeout_secs: Option<u64>,
        max_parallelism: usize,
    ) -> Self {
        Self::from_engine(DecisionEngine::new(
            Arc::new(super::decision_rule::RuleDecisionProvider::new()),
            model,
            mode,
            timeout_secs,
            max_parallelism,
        ))
    }

    #[must_use]
    pub fn mode(&self) -> DecisionMode {
        self.engine.mode()
    }

    /// Jev 是否已配置（设置 UI 只显示布尔）。
    #[must_use]
    pub fn model_configured(&self) -> bool {
        self.engine.model_configured()
    }

    /// 生成 `DecisionRequest`（把 orchestration 侧已知信号填进最小化契约）。
    #[must_use]
    pub fn request(
        &self,
        request_id: &str,
        message: &str,
        app_module: Option<&str>,
        app_page: Option<&str>,
        entity_kind: Option<&str>,
        available_workers: &[String],
        available_tool_groups: &[String],
        budget_tier: devtoolbox_core::agents::BudgetTier,
    ) -> devtoolbox_core::agents::DecisionRequest {
        use devtoolbox_core::agents::{
            BudgetTier, DECISION_MESSAGE_MAX_CHARS, DecisionRequest,
        };
        let _ = BudgetTier::Full;
        let message_truncated: String = message
            .chars()
            .take(DECISION_MESSAGE_MAX_CHARS)
            .collect();
        // 跨模块启发式：多个模块关键词命中（与 Rule provider 同一组词，§18）。
        let lowered = message_truncated.to_lowercase();
        let cross_module_hint = ["日志", "文档", "记忆", "服务器", "history", "documents", "memory", "server"]
            .iter()
            .filter(|needle| lowered.contains(**needle))
            .count()
            >= 2;
        let explicit_deep = lowered.contains("深入研究")
            || lowered.contains("深入分析")
            || lowered.contains("详细分析")
            || lowered.contains("全面比较")
            || lowered.contains("系统性");
        let multi_agent_off = lowered.contains("不要使用多 agent")
            || lowered.contains("别用多 agent")
            || lowered.contains("不用多智能体");
        let external_evidence_needed = lowered.contains("搜索")
            || lowered.contains("检索")
            || lowered.contains("查一下")
            || lowered.contains("最新");
        DecisionRequest {
            request_id: request_id.to_string(),
            message: message_truncated,
            module: app_module.map(str::to_string),
            page: app_page.map(str::to_string),
            entity_kind: entity_kind.map(str::to_string),
            cross_module_hint,
            external_evidence_needed,
            explicit_deep,
            multi_agent_off,
            available_workers: available_workers.to_vec(),
            available_tool_groups: available_tool_groups.to_vec(),
            budget_tier,
            capability_constraints: vec![
                "read_only".into(),
                "no_memory_write".into(),
                "no_system".into(),
            ],
        }
    }

    /// 执行一次决策（永远成功返回；含遥测）。
    pub async fn decide(
        &self,
        request: &devtoolbox_core::agents::DecisionRequest,
    ) -> (devtoolbox_core::agents::DecisionResult, DecisionTelemetry) {
        self.engine.decide(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workers() -> Vec<String> {
        vec!["research".into(), "planner".into(), "reviewer".into()]
    }

    #[tokio::test]
    async fn rule_only_engine_routes_direct_for_simple_request() {
        let engine = AgentDecisionEngine::rule_only();
        assert_eq!(engine.mode(), DecisionMode::Rule);
        assert!(!engine.model_configured());
        let request = engine.request(
            "req-1",
            "你好",
            None,
            None,
            None,
            &workers(),
            &[],
            devtoolbox_core::agents::BudgetTier::Full,
        );
        let (result, telemetry) = engine.decide(&request).await;
        assert_eq!(result.strategy, devtoolbox_core::agents::DecisionStrategy::Direct);
        assert_eq!(telemetry.provider, "rule");
    }

    #[tokio::test]
    async fn request_minimizes_private_context() {
        let engine = AgentDecisionEngine::rule_only();
        let long = "资".repeat(5000);
        let request = engine.request(
            "req-2",
            &long,
            Some("history"),
            Some("person-detail"),
            Some("person"),
            &workers(),
            &["history".into()],
            devtoolbox_core::agents::BudgetTier::Full,
        );
        assert_eq!(
            request.message.chars().count(),
            devtoolbox_core::agents::DECISION_MESSAGE_MAX_CHARS
        );
        // 只带实体类型，不带 id / label。
        assert_eq!(request.entity_kind.as_deref(), Some("person"));
        assert_eq!(request.module.as_deref(), Some("history"));
    }

    #[tokio::test]
    async fn cross_module_hint_detected_in_request_builder() {
        let engine = AgentDecisionEngine::rule_only();
        let request = engine.request(
            "req-3",
            "对比日志和文档",
            None,
            None,
            None,
            &workers(),
            &[],
            devtoolbox_core::agents::BudgetTier::Full,
        );
        assert!(request.cross_module_hint);
    }
}
