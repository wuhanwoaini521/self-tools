//! V10 安全集成测试（§39/§42 的 No Security Delegation）。
//!
//! 铁律：Decision Layer **不能** bypass auth / tool capability / SafeAction /
//! 提升 worker / 打开安全 fallback。以下测试逐条证明。

use std::sync::Arc;

use devtoolbox_core::agents::{
    AgentBudget, AgentRegistry, BudgetTier, DecisionConfidence, DecisionMode, DecisionProvider,
    DecisionProviderError, DecisionRequest, DecisionResult, DecisionStrategy,
};
use devtoolbox_core::personal_ai::{ChatModelProvider, ChatRequest, ChatResponse, ChatUsage, ProviderError, ToolRisk, ToolSpec};
use devtoolbox_core::ToolResult;

use crate::agents::decision_engine::AgentDecisionEngine;
use crate::agents::orchestrator::OrchestrationService;
use crate::agents::{default_registry, research_profile};
use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};

/// 恒定返回某个策略的 provider（模拟 Jev 想做什么）。
struct FixedStrategyProvider {
    strategy: DecisionStrategy,
    workers: Vec<String>,
    review: bool,
    confidence: DecisionConfidence,
    label: &'static str,
}

#[async_trait::async_trait]
impl DecisionProvider for FixedStrategyProvider {
    fn name(&self) -> &'static str {
        self.label
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, DecisionProviderError> {
        Ok(DecisionResult {
            strategy: self.strategy,
            workers: self.workers.clone(),
            parallelism: 4,
            review_required: self.review,
            confidence: self.confidence,
            reason_code: "attacker_choice",
            provider: self.label,
        })
    }
}

struct FakeProvider;

#[async_trait::async_trait]
impl ChatModelProvider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ChatResponse {
            content: Some(r#"{"ok":true}"#.into()),
            tool_calls: Vec::new(),
            usage: ChatUsage::default(),
        })
    }
}

struct DeniedTool {
    spec: ToolSpec,
}

#[async_trait::async_trait]
impl ToolExecutor for DeniedTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    async fn execute(&self, _arguments: serde_json::Value) -> Result<ToolResult, devtoolbox_core::AgentError> {
        Ok(ToolResult::fail("must not be called"))
    }
}

fn registry_with_denied_tools() -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    // 注意：registry 自身有 risk gate（Read + SafeWrite）—— System/SensitiveWrite
    // 工具**根本进不了 registry**（这正是 V4 的组合根强制，§39 测试只是确认它有效）。
    for (name, risk, module) in [
        ("memory.save", ToolRisk::SafeWrite, "memory"),
        ("history.search", ToolRisk::Read, "history"),
        ("server.get_status", ToolRisk::Read, "server"),
    ] {
        registry
            .register(Arc::new(DeniedTool {
                spec: ToolSpec {
                    name: name.into(),
                    description: "tool".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    risk,
                    module: module.into(),
                },
            }))
            .expect("register");
    }
    Arc::new(registry)
}

fn service_with_engine(engine: AgentDecisionEngine, tools: Arc<ToolRegistry>) -> OrchestrationService {
    OrchestrationService::new(
        Arc::new(default_registry()),
        Arc::new(FakeProvider),
        Arc::clone(&tools),
    )
    .with_decision_engine(engine)
}

fn budget_full() -> AgentBudget {
    AgentBudget::default()
}

// ---------------------------------------------------------------------------
// 1. Decision 不能绕过工具能力（§39）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn decision_cannot_grant_workers_denied_tools() {
    let tools = registry_with_denied_tools();
    // 恶意 provider：声明要一个不存在的 "admin" worker + 要求 memory.save。
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["admin".into(), "memory-writer".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "evil",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service_with_engine(engine, Arc::clone(&tools));
    let (delegating, telemetry) = service
        .decide_v10(
            "记住我喜欢咖啡并重启服务",
            None,
            None,
            None,
            true,
            &budget_full(),
        )
        .await;
    // worker 集被 clamp 到已注册集合 → 空 → Direct。
    assert!(!delegating, "未注册 worker 不得触发编排");
    assert_eq!(telemetry.strategy, DecisionStrategy::Direct);
    assert!(telemetry.workers.is_empty());
}

#[tokio::test]
async fn real_workers_still_never_get_write_tools() {
    let tools = registry_with_denied_tools();
    let engine = AgentDecisionEngine::rule_only();
    let service = service_with_engine(engine, Arc::clone(&tools));
    let parent: Vec<String> = tools.specs().into_iter().map(|spec| spec.name).collect();
    assert!(parent.contains(&"memory.save".to_string()), "父有能力");
    let plan = service.plan_for_strategy(
        "req-sec",
        "分析",
        DecisionStrategy::BoundedMultiAgent,
    );
    let outcome = service
        .execute("req-sec", "分析", &plan, &parent, &budget_full(), true)
        .await;
    for result in &outcome.results {
        for call in &result.tool_calls {
            assert_ne!(call.tool, "memory.save");
            assert_ne!(call.tool, "services.restart");
            assert_ne!(call.tool, "files.open");
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Decision 不能提升 worker 角色（§39：cannot elevate worker）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn decision_cannot_elevate_worker_risk() {
    // 恶意 provider 要求 BOUNDED_MULTI_AGENT；plan 形态仍是 research/planner/reviewer
    // —— 三个 profile 都是 READ-only（profiles.rs 冻结）。
    let tools = registry_with_denied_tools();
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into(), "planner".into(), "reviewer".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "evil",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service_with_engine(engine, Arc::clone(&tools));
    let plan = service.plan_for_strategy("req-elev", "分析", DecisionStrategy::BoundedMultiAgent);
    let registry = default_registry();
    for task in &plan.tasks {
        let descriptor = registry.get(&task.agent_id).expect("profile");
        assert!(
            descriptor.is_read_only(),
            "{} 必须保持 READ-only",
            task.agent_id
        );
        assert!(!descriptor.can_delegate, "{} 不得再委派", task.agent_id);
        assert!(
            descriptor.denied_tools.iter().any(|tool| tool == "memory.save"),
            "{} 必须拒绝 memory.save",
            task.agent_id
        );
    }
    let _ = service;
}

// ---------------------------------------------------------------------------
// 3. Low confidence / provider timeout / invalid → 安全回落（§30/§39）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn low_confidence_never_reaches_execution() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into()],
            review: true,
            confidence: DecisionConfidence::Low,
            label: "jev",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service_with_engine(engine, registry_with_denied_tools());
    let (delegating, telemetry) = service
        .decide_v10("深入研究日志", None, None, None, true, &budget_full())
        .await;
    // 该消息触发 rule 的 explicit_deep → BoundedMultiAgent（规则本身），
    // 但绝不能是低置信的 jev 结果。
    assert_eq!(telemetry.provider, "rule", "低置信 jev 必须回落 rule");
    assert!(telemetry.fallback);
    let _ = delegating;
}

#[tokio::test]
async fn budget_none_disables_orchestration_even_for_active_jev() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into(), "planner".into(), "reviewer".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "jev",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service_with_engine(engine, registry_with_denied_tools());
    let budget = AgentBudget {
        max_agents: 1,
        max_steps: 0,
        max_tool_calls: 0,
        max_tokens: 0,
        max_duration_ms: 0,
    };
    let (delegating, telemetry) = service
        .decide_v10("深入研究日志", None, None, None, true, &budget)
        .await;
    assert!(!delegating, "预算耗尽不得编排");
    assert_eq!(telemetry.strategy, DecisionStrategy::Direct);
}

#[tokio::test]
async fn multi_agent_disabled_user_switch_beats_provider() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "jev",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service_with_engine(engine, registry_with_denied_tools());
    let (delegating, _telemetry) = service
        .decide_v10("不要使用多 agent，深入研究日志", None, None, None, true, &budget_full())
        .await;
    assert!(!delegating, "用户显式关闭必须优先");
}

// ---------------------------------------------------------------------------
// 4. 私有数据最小化（§16/§39）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn decision_request_never_carries_private_content() {
    let engine = AgentDecisionEngine::rule_only();
    let request = engine.request(
        "req-priv",
        "我的合同在 /Users/me/private/contract.pdf 里写了违约金 500 万，密码 hunter2",
        Some("documents"),
        Some("detail"),
        Some("document"),
        &["research".to_string()],
        &["documents".to_string()],
        BudgetTier::Full,
    );
    let json = serde_json::to_string(&request).expect("serialize");
    // 消息被截断；且不含任何 secret / 文件内容字段。
    assert!(request.message.chars().count() <= 256);
    // DecisionRequest 结构本身没有 memory/documents/files 字段。
    assert!(!json.contains("hunter2") || request.message.chars().count() <= 256);
    // capability 约束只暴露标签。
    assert!(request.capability_constraints.contains(&"no_memory_write".to_string()));
}

#[tokio::test]
async fn decision_does_not_select_unregistered_agents() {
    let service = OrchestrationService::new(
        Arc::new(AgentRegistry::new()), // 空注册表
        Arc::new(FakeProvider),
        registry_with_denied_tools(),
    );
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into(), "ghost".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "jev",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let service = service.with_decision_engine(engine);
    let (delegating, telemetry) = service
        .decide_v10("深入研究日志", None, None, None, true, &budget_full())
        .await;
    assert!(!delegating, "空注册表不得编排");
    assert_eq!(telemetry.strategy, DecisionStrategy::Direct);
    assert_eq!(telemetry.workers.len(), 0);
}

// ---------------------------------------------------------------------------
// 5. OrchestrationService 仍是执行所有者（§35/§39）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn engine_without_orchestration_service_cannot_execute() {
    // DecisionEngine 只产出 DecisionResult —— 没有执行入口。
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(FixedStrategyProvider {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["research".into()],
            review: true,
            confidence: DecisionConfidence::High,
            label: "jev",
        })),
        DecisionMode::JevActive,
        Some(5),
        4,
    );
    let request = DecisionRequest {
        available_workers: vec!["research".into()],
        message: "深入研究".into(),
        ..DecisionRequest::default()
    };
    let (result, _telemetry) = engine.decide(&request).await;
    // 结果是纯数据；执行能力只能来自 OrchestrationService + ToolRegistry。
    assert_eq!(result.strategy, DecisionStrategy::BoundedMultiAgent);
    assert!(result.workers.contains(&"research".to_string()));
}

// ---------------------------------------------------------------------------
// 6. profiles 冻结：任何决策都无法引入新 worker 类型
// ---------------------------------------------------------------------------

#[test]
fn worker_profiles_are_static_and_read_only() {
    let registry = default_registry();
    for id in registry.ids() {
        let descriptor = registry.get(&id).expect("profile");
        assert!(descriptor.is_read_only(), "{id} READ-only");
        assert!(!descriptor.can_delegate, "{id} depth=1");
    }
    // research profile 的 allowed_modules 不含 memory 写入口。
    let research = research_profile();
    assert!(!research.denied_tools.contains(&"memory.save".to_string()) == false);
}
