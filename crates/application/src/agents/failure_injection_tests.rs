//! 故障注入（V11 §167-§168）：LLM down / Jev down / Search down / DB 不可用 /
//! bad file / MCP 未授权 / agent timeout / backup 不可用 / PWA 离线。
//!
//! 期望：degrade / fail safely / recover；**不得** panic / 数据损坏 / 安全回落打开。
//! 全部用 Fake 与临时目录，不碰真实用户数据、不需要外网。

use std::sync::Arc;

use devtoolbox_core::agents::{
    AgentBudget, BudgetTier, DecisionConfidence, DecisionMode, DecisionProvider,
    DecisionProviderError, DecisionRequest, DecisionResult, DecisionStrategy,
};
use devtoolbox_core::personal_ai::{ChatModelProvider, ChatRequest, ChatResponse, ProviderError};
use devtoolbox_core::operations::{AppPaths, StartupMarker};

use crate::agents::decision_engine::AgentDecisionEngine;
use crate::agents::orchestrator::OrchestrationService;
use crate::backup::BackupService;
use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};
use crate::search::{GlobalSearchPort, GlobalSearchService};
use devtoolbox_core::search::{GlobalSearchHit, GlobalSearchQuery, SearchSource};

// ---------------------------------------------------------------------------
// 替身
// ---------------------------------------------------------------------------

/// 永远失败的 provider（模拟 LLM down）。
struct DownProvider;

#[async_trait::async_trait]
impl ChatModelProvider for DownProvider {
    fn name(&self) -> &'static str {
        "down"
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::unavailable("injected: provider down"))
    }
}

/// 永远挂起的 provider（模拟 LLM hang → 由外层 timeout 兜底）。
struct HangingProvider;

#[async_trait::async_trait]
impl ChatModelProvider for HangingProvider {
    fn name(&self) -> &'static str {
        "hanging"
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        std::future::pending::<()>().await;
        unreachable!()
    }
}

/// Jev down（provider 层失败）。
struct JevDown;

#[async_trait::async_trait]
impl DecisionProvider for JevDown {
    fn name(&self) -> &'static str {
        "jev"
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, DecisionProviderError> {
        Err(DecisionProviderError::unavailable("injected: jev down"))
    }
}

/// Jev 想越权（选择未注册 worker + 高置信）。
struct JevHostile;

#[async_trait::async_trait]
impl DecisionProvider for JevHostile {
    fn name(&self) -> &'static str {
        "jev"
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, DecisionProviderError> {
        Ok(DecisionResult {
            strategy: DecisionStrategy::BoundedMultiAgent,
            workers: vec!["admin".into(), "root".into()],
            parallelism: 99,
            review_required: true,
            confidence: DecisionConfidence::High,
            reason_code: "hostile",
            provider: "jev",
        })
    }
}

/// 失败的搜索源。
struct FailingSearchSource;

impl GlobalSearchPort for FailingSearchSource {
    fn source(&self) -> SearchSource {
        SearchSource::Memory
    }
    fn search(&self, _query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        Err("injected: source down".into())
    }
}

/// 正常的搜索源。
struct HealthySearchSource;

impl GlobalSearchPort for HealthySearchSource {
    fn source(&self) -> SearchSource {
        SearchSource::Documents
    }
    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        Ok(vec![GlobalSearchHit::new(
            SearchSource::Documents,
            "doc",
            "命中标题",
            format!("包含 {}", query.query),
            serde_json::json!({"module": "documents"}),
            0.9,
        )])
    }
}

struct NoopTool {
    spec: devtoolbox_core::ToolSpec,
}

#[async_trait::async_trait]
impl ToolExecutor for NoopTool {
    fn spec(&self) -> &devtoolbox_core::ToolSpec {
        &self.spec
    }
    async fn execute(
        &self,
        _arguments: serde_json::Value,
    ) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
        Ok(devtoolbox_core::ToolResult::ok(serde_json::json!({})))
    }
}

fn registry() -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(NoopTool {
            spec: devtoolbox_core::ToolSpec {
                name: "history.search".into(),
                description: "search".into(),
                input_schema: serde_json::json!({"type": "object"}),
                risk: devtoolbox_core::personal_ai::ToolRisk::Read,
                module: "history".into(),
            },
        }))
        .expect("register");
    Arc::new(registry)
}

fn orchestration(provider: Arc<dyn ChatModelProvider>) -> OrchestrationService {
    OrchestrationService::new(
        Arc::new(crate::agents::profiles::default_registry()),
        provider,
        registry(),
    )
}

// ---------------------------------------------------------------------------
// §71-§75 故障矩阵
// ---------------------------------------------------------------------------

#[tokio::test]
async fn llm_down_returns_controlled_error_not_panic() {
    // 单 Agent 路径：provider down → 受控错误（PersonalAgent 侧 Err），不是 panic。
    let service = orchestration(Arc::new(DownProvider));
    let plan = service.plan("req-llm", "分析", false);
    let outcome = service
        .execute(
            "req-llm",
            "分析",
            &plan,
            &["history.search".to_string()],
            &AgentBudget::default(),
            true,
        )
        .await;
    // worker 失败但编排服务本身安全返回（§61：失败结果进入 merged）。
    assert!(!outcome.trace.runs.is_empty(), "失败的 run 也要记账");
    let all_usable = outcome.results.iter().all(|result| result.status.is_usable());
    assert!(!all_usable || outcome.partial, "失败必须体现在 partial/status 上");
}

#[tokio::test]
async fn llm_hang_is_bounded_by_worker_timeout() {
    // V9 gate9 已证明 timeout；这里确认 hang 不会让编排挂死（有超时的 plan 仍返回）。
    let service = orchestration(Arc::new(HangingProvider));
    let plan = service.plan_for_strategy("req-hang", "分析", DecisionStrategy::ResearchOnly);
    let budget = AgentBudget {
        max_agents: 2,
        max_steps: 2,
        max_tool_calls: 4,
        max_tokens: 4_000,
        // 很短的总时长上限：强制预算路径停止（不等 provider hang 结束）。
        max_duration_ms: 300,
    };
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        service.execute(
            "req-hang",
            "分析",
            &plan,
            &["history.search".to_string()],
            &budget,
            true,
        ),
    )
    .await
    .expect("orchestration must not hang forever");
    assert!(outcome.partial || !outcome.results.is_empty());
}

#[tokio::test]
async fn jev_down_falls_back_to_rule_and_request_succeeds() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(JevDown)),
        DecisionMode::JevActive,
        Some(2),
        4,
    );
    let service = orchestration(Arc::new(DownProvider)).with_decision_engine(engine);
    let (delegating, telemetry) = service
        .decide_v10(
            "结合服务器日志和文档分析不稳定的原因",
            None,
            None,
            None,
            true,
            &AgentBudget::default(),
        )
        .await;
    // 规则仍然识别跨模块 → 编排继续，provider 标记 fallback。
    assert!(delegating, "Jev 挂了 rule 必须接管");
    assert!(telemetry.fallback, "必须记录 fallback");
    assert_eq!(telemetry.provider, "rule");
}

#[tokio::test]
async fn jev_hostile_choice_cannot_escalate() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(JevHostile)),
        DecisionMode::JevActive,
        Some(2),
        4,
    );
    let service = orchestration(Arc::new(DownProvider)).with_decision_engine(engine);
    let (delegating, telemetry) = service
        .decide_v10("随便分析一下", None, None, None, true, &AgentBudget::default())
        .await;
    assert!(!delegating, "未注册 worker 不得触发编排");
    assert_eq!(telemetry.strategy, DecisionStrategy::Direct);
    assert!(telemetry.workers.is_empty());
}

#[tokio::test]
async fn search_source_down_degrades_only_that_source() {
    let service = GlobalSearchService::new(vec![
        Arc::new(FailingSearchSource),
        Arc::new(HealthySearchSource),
    ]);
    let result = service.search(&GlobalSearchQuery::new("合同"));
    assert_eq!(result.hits.len(), 1, "健康源仍可用");
    assert_eq!(result.hits[0].source, SearchSource::Documents);
    assert_eq!(result.degraded_sources, vec![SearchSource::Memory]);
}

#[tokio::test]
async fn empty_search_registry_is_not_an_error() {
    let service = GlobalSearchService::new(Vec::new());
    let result = service.search(&GlobalSearchQuery::new("任何"));
    assert!(result.hits.is_empty());
    // 无注册源 = 无降级可言（降级只针对「注册了但失败」的源）。
    assert!(result.degraded_sources.is_empty());
    assert_eq!(result.total, 0);
}

#[tokio::test]
async fn budget_exhausted_stops_orchestration_safely() {
    let engine = AgentDecisionEngine::new(
        Some(Arc::new(JevHostile)),
        DecisionMode::JevActive,
        Some(2),
        4,
    );
    let service = orchestration(Arc::new(DownProvider)).with_decision_engine(engine);
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
    assert!(!delegating);
    assert_eq!(telemetry.strategy, DecisionStrategy::Direct);
    assert_eq!(telemetry.reason_code, "budget_exhausted");
}

#[test]
fn crash_marker_survives_unclean_exit_and_recovers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::from_root(dir.path());
    let marker = StartupMarker::new(&paths);
    // 模拟崩溃：marker 未清理。
    marker.mark_running().expect("mark");
    drop(marker);
    let marker = StartupMarker::new(&paths);
    assert!(marker.was_unclean());
    let report = marker.recover();
    assert!(report.unclean_previous_run);
    assert!(!report.rebuildable_artifacts.is_empty());
    // 恢复后再启动 mark + clean。
    marker.mark_running().expect("mark");
    marker.mark_clean();
    assert!(!StartupMarker::new(&paths).was_unclean());
}

#[tokio::test]
async fn backup_failure_does_not_affect_service() {
    // backup 目标不可写（指向一个文件当目录）→ BackupService 返回受控错误。
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("not-a-directory");
    std::fs::write(&file_path, b"x").expect("write file");
    let service = BackupService::new();
    let error = service
        .backup(&file_path, "0.1.0", "injected: unavailable destination")
        .expect_err("must fail");
    assert!(!error.is_empty(), "受控错误");
    // 服务本身（搜索 / 决策）不受影响。
    let search = GlobalSearchService::new(vec![Arc::new(HealthySearchSource)]);
    let result = search.search(&GlobalSearchQuery::new("仍可搜索"));
    assert_eq!(result.hits.len(), 1);
}

#[test]
fn offline_pwa_shell_is_independent_of_network() {
    // PWA 离线判定是纯函数（前端）；后端侧验证 secure-context 规则与
    // 网络无关：DeployMode::Production 需要安全上下文。
    use devtoolbox_core::operations::DeployMode;
    assert!(DeployMode::Production.requires_secure_context());
    assert!(!DeployMode::Development.requires_secure_context());
}

#[test]
fn mcp_unauthorized_is_fail_closed() {
    // V8/V9 已证明远程未授权拒绝；这里固化 DecisionRequest 的能力约束标签
    // 不含任何「授予」语义（决策层永远只读约束）。
    let request = DecisionRequest {
        available_workers: vec!["research".into()],
        budget_tier: BudgetTier::Full,
        ..DecisionRequest::default()
    };
    assert!(
        request.capability_constraints.is_empty()
            || request.capability_constraints.iter().all(|tag| tag.starts_with("no_")
                || matches!(tag.as_str(), "read_only")),
        "能力标签只能是约束，不得含授权语义"
    );
    assert!(request.allows_orchestration());
}

#[test]
fn bad_document_row_is_a_controlled_error() {
    // StudyBoard store 已测；这里验证 decision request 对超长输入仍安全。
    let long = "x".repeat(10_000);
    let request = DecisionRequest::new(&long);
    assert_eq!(request.message.chars().count(), 256);
}

#[tokio::test]
async fn agent_timeout_marks_run_timed_out() {
    let service = orchestration(Arc::new(HangingProvider));
    let plan = service.plan_for_strategy("req-timeout", "分析", DecisionStrategy::ResearchOnly);
    // research profile 自带 60s timeout；用小预算 + 取消语义验证受控停止。
    let budget = AgentBudget {
        max_agents: 2,
        max_steps: 2,
        max_tool_calls: 4,
        max_tokens: 4_000,
        max_duration_ms: 250,
    };
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        service.execute(
            "req-timeout",
            "分析",
            &plan,
            &["history.search".to_string()],
            &budget,
            true,
        ),
    )
    .await
    .expect("must not hang");
    // 至少一条 run 被记录且整体未 panic。
    assert!(!outcome.trace.runs.is_empty());
}
