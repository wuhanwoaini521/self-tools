//! Decision Intelligence Layer（V10 §15-§35）核心契约。
//!
//! ```text
//! DecisionRequest ──▶ DecisionProvider (rule / jev / llm)
//!                          │  Result<DecisionResult>
//!                          ▼
//!                    DecisionEngine（mode + fallback + shadow）
//!                          │
//!                     DecisionResult
//!                          ▼
//!      OrchestrationService（唯一执行方）── 不变的 capability / budget / SafeAction
//! ```
//!
//! **硬边界（§12/§39）**：本层只决定 *orchestration strategy*（direct / workers /
//! parallelism / review）。它**不能**决定 authorization、tool permission、MCP scope、
//! 文件安全、SafeAction、confirmation、SYSTEM 权限、Memory 写入权限 —— 这些继续由
//! 确定性代码（capability intersect + `TaskEnvelope::validate` + child budget +
//! SafeActionService）控制。因此 `DecisionResult` 只含「选哪些已注册 worker」这类
//! 无害信息，且 engine 会在返回前把 worker 集裁剪到 `available_workers`。

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::personal_ai::ProviderError;

/// 编排策略（§25：至少 5 种）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStrategy {
    /// 直接回答：不启动任何 worker。
    #[default]
    Direct,
    /// 只做检索取证（并行 research）。
    ResearchOnly,
    /// 先规划再检索（planner + research）。
    PlanAndResearch,
    /// 检索后再审查（research + reviewer）。
    ResearchAndReview,
    /// 有界多 Agent（research + planner + reviewer）。
    BoundedMultiAgent,
}

impl DecisionStrategy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionStrategy::Direct => "direct",
            DecisionStrategy::ResearchOnly => "research_only",
            DecisionStrategy::PlanAndResearch => "plan_and_research",
            DecisionStrategy::ResearchAndReview => "research_and_review",
            DecisionStrategy::BoundedMultiAgent => "bounded_multi_agent",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(DecisionStrategy::Direct),
            "research_only" => Some(DecisionStrategy::ResearchOnly),
            "plan_and_research" => Some(DecisionStrategy::PlanAndResearch),
            "research_and_review" => Some(DecisionStrategy::ResearchAndReview),
            "bounded_multi_agent" => Some(DecisionStrategy::BoundedMultiAgent),
            _ => None,
        }
    }

    /// 策略是否需要启动 worker（空 plan = Direct）。
    #[must_use]
    pub fn is_orchestrating(self) -> bool {
        !matches!(self, DecisionStrategy::Direct)
    }

    /// 策略是否包含最终 review。
    #[must_use]
    pub fn includes_review(self) -> bool {
        matches!(
            self,
            DecisionStrategy::ResearchAndReview | DecisionStrategy::BoundedMultiAgent
        )
    }

    /// 策略是否包含 planner。
    #[must_use]
    pub fn includes_planner(self) -> bool {
        matches!(
            self,
            DecisionStrategy::PlanAndResearch | DecisionStrategy::BoundedMultiAgent
        )
    }
}

/// 决策模式（§26：RULE / JEV_SHADOW / JEV_ACTIVE）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    /// 只用规则基线（默认；未配置 Jev 时强制）。
    #[default]
    Rule,
    /// 实际路由走 Rule，同时调 Jev 记录差异（不影响执行）。
    JevShadow,
    /// 实际路由走 Jev；任何失败自动回落 Rule。
    JevActive,
}

impl DecisionMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionMode::Rule => "rule",
            DecisionMode::JevShadow => "jev_shadow",
            DecisionMode::JevActive => "jev_active",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rule" => Some(DecisionMode::Rule),
            "jev_shadow" | "jev-shadow" | "shadow" => Some(DecisionMode::JevShadow),
            "jev_active" | "jev-active" | "active" => Some(DecisionMode::JevActive),
            _ => None,
        }
    }
}

/// 决策置信档位（§29：集中阈值，禁止调用方散落 `if confidence > 0.5`）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionConfidence {
    /// 明确（>= 0.75）。
    High,
    /// 不确定（0.45..0.75）：仍可执行，但标记供遥测/审查。
    #[default]
    Uncertain,
    /// 低（< 0.45）：provider 结果不被信任 → 调用方回落 Rule。
    Low,
}

impl DecisionConfidence {
    /// 唯一阈值表（§29）。
    #[must_use]
    pub fn from_probability(probability: f32) -> Self {
        if !(probability.is_finite()) {
            return DecisionConfidence::Low;
        }
        if probability >= 0.75 {
            DecisionConfidence::High
        } else if probability >= 0.45 {
            DecisionConfidence::Uncertain
        } else {
            DecisionConfidence::Low
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionConfidence::High => "high",
            DecisionConfidence::Uncertain => "uncertain",
            DecisionConfidence::Low => "low",
        }
    }

    /// 低置信是否必须触发安全回落（§30）。
    #[must_use]
    pub fn forces_fallback(self) -> bool {
        matches!(self, DecisionConfidence::Low)
    }
}

/// 决策请求（§16：**只含路由所需信息**；禁止携带 Memory/Documents/聊天正文/文件内容）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionRequest {
    /// 请求 id（trace 用；取自 session/request）。
    pub request_id: String,
    /// 用户消息**截断**后的分类用文本（≤ 256 字符）。
    pub message: String,
    /// 当前模块（AppContext.module）。
    pub module: Option<String>,
    /// 当前页面（AppContext.page）。
    pub page: Option<String>,
    /// 当前实体类型（AppContext.entity.kind；不含 id/label）。
    pub entity_kind: Option<String>,
    /// 是否跨模块（启发式，供 provider 参考）。
    pub cross_module_hint: bool,
    /// 是否需要外部证据（搜索 / 联网类提示）。
    pub external_evidence_needed: bool,
    /// 用户显式要求深度研究。
    pub explicit_deep: bool,
    /// 用户显式关闭多 Agent。
    pub multi_agent_off: bool,
    /// 可用 worker id（已注册；provider 不得选集合外的）。
    pub available_workers: Vec<String>,
    /// 可用工具组（模块名；provider 不得据此扩权）。
    pub available_tool_groups: Vec<String>,
    /// 预算摘要（不暴露具体数值语义，只给档位）。
    pub budget_tier: BudgetTier,
    /// 当前能力约束标签（如 `read_only` / `no_memory_write` / `no_system`）。
    pub capability_constraints: Vec<String>,
}

/// 预算档位（把 `AgentBudget` 压成三档，避免把私有限额结构泄漏给 provider）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetTier {
    /// 允许完整有界编排（默认）。
    #[default]
    Full,
    /// 只允许单 worker。
    Single,
    /// 不允许编排（预算已紧张）。
    None,
}

impl BudgetTier {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetTier::Full => "full",
            BudgetTier::Single => "single",
            BudgetTier::None => "none",
        }
    }
}

/// 消息截断上限（§16）。
pub const DECISION_MESSAGE_MAX_CHARS: usize = 256;

impl DecisionRequest {
    /// 构造并**立即**最小化：截断 message、丢弃 entity label/id。
    #[must_use]
    pub fn new(message: &str) -> Self {
        Self {
            message: truncate_chars(message, DECISION_MESSAGE_MAX_CHARS),
            ..Self::default()
        }
    }

    /// 是否允许编排（预算 + 用户开关 + worker 可用性）。
    #[must_use]
    pub fn allows_orchestration(&self) -> bool {
        !self.multi_agent_off
            && !matches!(self.budget_tier, BudgetTier::None)
            && !self.available_workers.is_empty()
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

/// 决策结果（§17）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecisionResult {
    pub strategy: DecisionStrategy,
    /// 选中的 worker（必须是 `available_workers` 的子集；空 = Direct）。
    pub workers: Vec<String>,
    /// 并行度（1 = 串行；上限由调用方再夹取）。
    pub parallelism: usize,
    /// 是否需要最终 review。
    pub review_required: bool,
    pub confidence: DecisionConfidence,
    /// 稳定机器码（UI/遥测；不含正文）。
    pub reason_code: &'static str,
    /// 产出该决策的 provider 标识（rule / jev / …）。
    pub provider: &'static str,
}

impl DecisionResult {
    /// 直接回答（rule 侧与 fallback 共用）。
    #[must_use]
    pub fn direct(provider: &'static str, reason_code: &'static str) -> Self {
        Self {
            strategy: DecisionStrategy::Direct,
            workers: Vec::new(),
            parallelism: 1,
            review_required: false,
            confidence: DecisionConfidence::High,
            reason_code,
            provider,
        }
    }

    /// 决策是否需要执行编排。
    #[must_use]
    pub fn is_orchestrating(&self) -> bool {
        self.strategy.is_orchestrating()
    }

    /// 把 worker/parallelism 夹取到调用方给定的硬边界内（engine 最后一道关）。
    #[must_use]
    pub fn clamp(mut self, available_workers: &[String], max_parallelism: usize) -> Self {
        self.workers
            .retain(|worker| available_workers.iter().any(|have| have == worker));
        self.parallelism = self.parallelism.clamp(1, max_parallelism.max(1));
        if self.workers.is_empty() {
            // 选了 worker 但一个都不可用 → 退回 Direct，不虚构编排。
            self.strategy = DecisionStrategy::Direct;
            self.review_required = false;
        }
        self
    }
}

/// Provider 侧错误（复用 transport 错误类型：Unavailable / Timeout / Transport / InvalidResponse）。
pub type DecisionProviderError = ProviderError;

/// 决策 provider 端口（§15）。核心/应用层只认这个 trait。
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    /// 稳定标识（`rule` / `jev`）。
    fn name(&self) -> &'static str;

    /// 产出一次决策。实现方必须自带超时（§30）。
    async fn decide(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResult, DecisionProviderError>;

    /// 该 provider 当前是否可用（未配置 / 无 key → false）。
    fn is_available(&self) -> bool {
        true
    }
}

/// Shadow 观测记录（§27：rule 与 jev 的差异，只记结构与标签）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecisionShadowRecord {
    pub rule_strategy: DecisionStrategy,
    pub shadow_strategy: DecisionStrategy,
    pub agreed: bool,
    pub shadow_confidence: DecisionConfidence,
    pub shadow_latency_ms: u64,
}

/// 一次决策的完整记录（telemetry 用；不含正文 / secret / CoT）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecisionTelemetry {
    pub mode: DecisionMode,
    /// 实际生效的 provider。
    pub provider: &'static str,
    pub strategy: DecisionStrategy,
    pub confidence: DecisionConfidence,
    pub reason_code: &'static str,
    pub decision_latency_ms: u64,
    /// 是否发生过 fallback（jev 失败 → rule）。
    pub fallback: bool,
    /// JEV_SHADOW 下的影子对比记录。
    pub shadow: Option<DecisionShadowRecord>,
    /// 实际选中的 worker（clamp 后）。
    pub workers: Vec<String>,
}

/// Decision 超时默认值（秒）：决策必须比 worker 便宜，超时立即回 rule。
pub const DEFAULT_DECISION_TIMEOUT_SECS: u64 = 5;

/// 决策超时（§30）。
#[must_use]
pub fn decision_timeout(timeout_secs: Option<u64>) -> Duration {
    Duration::from_secs(
        timeout_secs
            .unwrap_or(DEFAULT_DECISION_TIMEOUT_SECS)
            .clamp(1, 60),
    )
}

/// 决策引擎（§13）：provider 链 + mode + shadow + fallback。
///
/// **分工（§35）**：engine 只返回 `DecisionResult`；执行、授权、工具循环、
/// 预算强制全部留在 `OrchestrationService`。engine 也不知道任何安全语义 ——
/// 它唯一的安全职责是把结果 `clamp` 到调用方给的 `available_workers` /
/// `max_parallelism`（§36 不退化）。
#[derive(Clone)]
pub struct DecisionEngine {
    rule: Arc<dyn DecisionProvider>,
    /// 可选模型 provider（jev / llm reference）。None = 只有 rule。
    model: Option<Arc<dyn DecisionProvider>>,
    mode: DecisionMode,
    /// 单次决策超时（provider 内也可自带；这里再兜底）。
    timeout: Duration,
    /// 并行度硬上限（来自 `AgentBudget::max_agents`）。
    max_parallelism: usize,
}

impl DecisionEngine {
    /// 仅规则（未配置模型 provider 时的唯一合法形态）。
    #[must_use]
    pub fn rule_only(rule: Arc<dyn DecisionProvider>) -> Self {
        Self {
            rule,
            model: None,
            mode: DecisionMode::Rule,
            timeout: decision_timeout(None),
            max_parallelism: 4,
        }
    }

    /// 完整装配。
    #[must_use]
    pub fn new(
        rule: Arc<dyn DecisionProvider>,
        model: Option<Arc<dyn DecisionProvider>>,
        mode: DecisionMode,
        timeout_secs: Option<u64>,
        max_parallelism: usize,
    ) -> Self {
        let mode = if mode != DecisionMode::Rule && model.is_none() {
            // 没有模型 provider 时禁止 Shadow/Active（fail-closed 到 RULE）。
            DecisionMode::Rule
        } else {
            mode
        };
        Self {
            rule,
            model,
            mode,
            timeout: decision_timeout(timeout_secs),
            max_parallelism: max_parallelism.clamp(1, 32),
        }
    }

    #[must_use]
    pub fn mode(&self) -> DecisionMode {
        self.mode
    }

    /// Jev（模型决策）是否已配置。设置 UI 只显示布尔，不显示 secret。
    #[must_use]
    pub fn model_configured(&self) -> bool {
        self.model
            .as_ref()
            .is_some_and(|provider| provider.is_available())
    }

    /// 主入口：永远返回一个可用决策（绝不向上传播错误 —— §30）。
    pub async fn decide(&self, request: &DecisionRequest) -> (DecisionResult, DecisionTelemetry) {
        let started = Instant::now();
        let hard_bound = Bound {
            available_workers: request.available_workers.clone(),
            max_parallelism: self.max_parallelism,
            allows_orchestration: request.allows_orchestration(),
        };

        match self.mode {
            DecisionMode::Rule => {
                let (result, fallback) = self.rule_decide(request, &hard_bound).await;
                let telemetry = DecisionTelemetry {
                    mode: self.mode,
                    provider: result.provider,
                    strategy: result.strategy,
                    confidence: result.confidence,
                    reason_code: result.reason_code,
                    decision_latency_ms: elapsed_ms(started),
                    fallback,
                    shadow: None,
                    workers: result.workers.clone(),
                };
                (result, telemetry)
            }
            DecisionMode::JevActive => {
                let Some(model) = self.model.as_ref() else {
                    // 结构上不可能（构造函数已降级）；防御性回落。
                    return self
                        .rule_only_path(request, &hard_bound, started, true)
                        .await;
                };
                match tokio::time::timeout(self.timeout, model.decide(request)).await {
                    Ok(Ok(result))
                        if !result.confidence.forces_fallback()
                            && result.strategy.is_orchestrating()
                            && hard_bound.allows_orchestration
                            || result.strategy == DecisionStrategy::Direct
                                && !result.confidence.forces_fallback() =>
                    {
                        let clamped =
                            result.clamp(&hard_bound.available_workers, hard_bound.max_parallelism);
                        let telemetry = DecisionTelemetry {
                            mode: self.mode,
                            provider: clamped.provider,
                            strategy: clamped.strategy,
                            confidence: clamped.confidence,
                            reason_code: clamped.reason_code,
                            decision_latency_ms: elapsed_ms(started),
                            fallback: false,
                            shadow: None,
                            workers: clamped.workers.clone(),
                        };
                        (clamped, telemetry)
                    }
                    _ => {
                        self.rule_only_path(request, &hard_bound, started, true)
                            .await
                    }
                }
            }
            DecisionMode::JevShadow => {
                // 实际路由 = rule；shadow 并行观测，失败/超时只丢影子记录。
                let rule_started = Instant::now();
                let (rule_result, _rule_fallback) = self.rule_decide(request, &hard_bound).await;
                let shadow = match self.model.as_ref() {
                    Some(model) => {
                        let shadow_started = Instant::now();
                        match tokio::time::timeout(self.timeout, model.decide(request)).await {
                            Ok(Ok(shadow_result)) => {
                                let shadow_clamped = shadow_result.clamp(
                                    &hard_bound.available_workers,
                                    hard_bound.max_parallelism,
                                );
                                Some(DecisionShadowRecord {
                                    rule_strategy: rule_result.strategy,
                                    shadow_strategy: shadow_clamped.strategy,
                                    agreed: shadow_clamped.strategy == rule_result.strategy,
                                    shadow_confidence: shadow_clamped.confidence,
                                    shadow_latency_ms: elapsed_ms(shadow_started),
                                })
                            }
                            _ => None,
                        }
                    }
                    None => None,
                };
                let telemetry = DecisionTelemetry {
                    mode: self.mode,
                    provider: rule_result.provider,
                    strategy: rule_result.strategy,
                    confidence: rule_result.confidence,
                    reason_code: rule_result.reason_code,
                    decision_latency_ms: elapsed_ms(rule_started),
                    fallback: false,
                    shadow,
                    workers: rule_result.workers.clone(),
                };
                (rule_result, telemetry)
            }
        }
    }

    async fn rule_only_path(
        &self,
        request: &DecisionRequest,
        bound: &Bound,
        started: Instant,
        fallback: bool,
    ) -> (DecisionResult, DecisionTelemetry) {
        let (result, _) = self.rule_decide(request, bound).await;
        let telemetry = DecisionTelemetry {
            mode: self.mode,
            provider: result.provider,
            strategy: result.strategy,
            confidence: result.confidence,
            reason_code: result.reason_code,
            decision_latency_ms: elapsed_ms(started),
            fallback,
            shadow: None,
            workers: result.workers.clone(),
        };
        (result, telemetry)
    }

    /// rule provider + 硬边界夹取。rule 自身失败 → Direct（可用性兜底）。
    async fn rule_decide(
        &self,
        request: &DecisionRequest,
        bound: &Bound,
    ) -> (DecisionResult, bool) {
        match self.rule.decide(request).await {
            Ok(result) => (
                result.clamp(&bound.available_workers, bound.max_parallelism),
                false,
            ),
            Err(_) => (
                DecisionResult::direct(self.rule.name(), "rule_provider_failed"),
                true,
            ),
        }
    }
}

/// clamp 所需的硬边界（engine 内部）。
struct Bound {
    available_workers: Vec<String>,
    max_parallelism: usize,
    allows_orchestration: bool,
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::ProviderError;

    struct StubProvider {
        label: &'static str,
        available: bool,
        behavior: StubBehavior,
    }

    enum StubBehavior {
        Fixed(DecisionStrategy),
        Fail,
        Timeout,
        AlwaysDirect,
    }

    #[async_trait]
    impl DecisionProvider for StubProvider {
        fn name(&self) -> &'static str {
            self.label
        }

        fn is_available(&self) -> bool {
            self.available
        }

        async fn decide(
            &self,
            _request: &DecisionRequest,
        ) -> Result<DecisionResult, DecisionProviderError> {
            match self.behavior {
                StubBehavior::Fixed(strategy) => Ok(DecisionResult {
                    strategy,
                    workers: vec!["research".into()],
                    parallelism: 2,
                    review_required: strategy.includes_review(),
                    confidence: DecisionConfidence::High,
                    reason_code: "stub",
                    provider: self.label,
                }),
                StubBehavior::Fail => Err(ProviderError::invalid_response("boom")),
                StubBehavior::Timeout => {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    unreachable!()
                }
                StubBehavior::AlwaysDirect => Ok(DecisionResult::direct(self.label, "stub_direct")),
            }
        }
    }

    fn orchestrate_request() -> DecisionRequest {
        DecisionRequest {
            request_id: "req-1".into(),
            message: "比较文档和服务日志".into(),
            module: None,
            page: None,
            entity_kind: None,
            cross_module_hint: true,
            external_evidence_needed: false,
            explicit_deep: false,
            multi_agent_off: false,
            available_workers: vec!["research".into(), "planner".into(), "reviewer".into()],
            available_tool_groups: vec!["history".into(), "documents".into(), "server".into()],
            budget_tier: BudgetTier::Full,
            capability_constraints: vec!["read_only".into()],
        }
    }

    #[tokio::test]
    async fn rule_mode_returns_rule_result() {
        let engine = DecisionEngine::rule_only(Arc::new(StubProvider {
            label: "rule",
            available: true,
            behavior: StubBehavior::Fixed(DecisionStrategy::ResearchOnly),
        }));
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.strategy, DecisionStrategy::ResearchOnly);
        assert_eq!(telemetry.mode, DecisionMode::Rule);
        assert!(!telemetry.fallback);
        assert!(telemetry.shadow.is_none());
    }

    #[tokio::test]
    async fn active_mode_falls_back_on_provider_error() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::Direct),
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Fail,
            })),
            DecisionMode::JevActive,
            Some(1),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.provider, "rule");
        assert!(telemetry.fallback);
    }

    #[tokio::test]
    async fn active_mode_falls_back_on_timeout() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::Direct),
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Timeout,
            })),
            DecisionMode::JevActive,
            Some(1),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.provider, "rule");
        assert!(telemetry.fallback);
    }

    #[tokio::test]
    async fn active_mode_falls_back_on_low_confidence() {
        struct LowConfidence;
        #[async_trait]
        impl DecisionProvider for LowConfidence {
            fn name(&self) -> &'static str {
                "jev"
            }
            async fn decide(
                &self,
                _request: &DecisionRequest,
            ) -> Result<DecisionResult, DecisionProviderError> {
                Ok(DecisionResult {
                    strategy: DecisionStrategy::BoundedMultiAgent,
                    workers: vec!["research".into()],
                    parallelism: 2,
                    review_required: true,
                    confidence: DecisionConfidence::Low,
                    reason_code: "jev_low",
                    provider: "jev",
                })
            }
        }
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::Direct),
            }),
            Some(Arc::new(LowConfidence)),
            DecisionMode::JevActive,
            Some(2),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.provider, "rule");
        assert!(telemetry.fallback);
    }

    #[tokio::test]
    async fn active_mode_uses_model_when_healthy() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::Direct),
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::BoundedMultiAgent),
            })),
            DecisionMode::JevActive,
            Some(5),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.provider, "jev");
        assert_eq!(result.strategy, DecisionStrategy::BoundedMultiAgent);
        assert!(!telemetry.fallback);
    }

    #[tokio::test]
    async fn shadow_mode_routes_rule_but_records_difference() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::ResearchOnly),
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::Direct),
            })),
            DecisionMode::JevShadow,
            Some(5),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        // 实际路由 = rule。
        assert_eq!(result.provider, "rule");
        assert_eq!(result.strategy, DecisionStrategy::ResearchOnly);
        // shadow 记录存在且标记差异。
        let shadow = telemetry.shadow.expect("shadow record");
        assert!(!shadow.agreed);
        assert_eq!(shadow.rule_strategy, DecisionStrategy::ResearchOnly);
        assert_eq!(shadow.shadow_strategy, DecisionStrategy::Direct);
    }

    #[tokio::test]
    async fn shadow_failure_does_not_affect_execution() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::AlwaysDirect,
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Fail,
            })),
            DecisionMode::JevShadow,
            Some(5),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.strategy, DecisionStrategy::Direct);
        assert!(telemetry.shadow.is_none());
        assert!(!telemetry.fallback);
    }

    #[tokio::test]
    async fn shadow_without_model_provider_degrades_to_rule() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::AlwaysDirect,
            }),
            None,
            DecisionMode::JevActive,
            Some(5),
            4,
        );
        let (result, telemetry) = engine.decide(&orchestrate_request()).await;
        assert_eq!(result.provider, "rule");
        assert_eq!(telemetry.mode, DecisionMode::Rule);
    }

    #[tokio::test]
    async fn budget_none_forces_direct_even_when_model_wants_orchestration() {
        let engine = DecisionEngine::new(
            Arc::new(StubProvider {
                label: "rule",
                available: true,
                behavior: StubBehavior::AlwaysDirect,
            }),
            Some(Arc::new(StubProvider {
                label: "jev",
                available: true,
                behavior: StubBehavior::Fixed(DecisionStrategy::BoundedMultiAgent),
            })),
            DecisionMode::JevActive,
            Some(5),
            4,
        );
        let request = DecisionRequest {
            budget_tier: BudgetTier::None,
            ..orchestrate_request()
        };
        let (result, _telemetry) = engine.decide(&request).await;
        assert_eq!(result.strategy, DecisionStrategy::Direct);
        assert!(result.workers.is_empty());
    }

    #[test]
    fn strategy_round_trips_and_classifies() {
        for strategy in [
            DecisionStrategy::Direct,
            DecisionStrategy::ResearchOnly,
            DecisionStrategy::PlanAndResearch,
            DecisionStrategy::ResearchAndReview,
            DecisionStrategy::BoundedMultiAgent,
        ] {
            assert_eq!(DecisionStrategy::parse(strategy.as_str()), Some(strategy));
        }
        assert!(!DecisionStrategy::Direct.is_orchestrating());
        assert!(DecisionStrategy::ResearchOnly.is_orchestrating());
        assert!(DecisionStrategy::BoundedMultiAgent.includes_review());
        assert!(DecisionStrategy::PlanAndResearch.includes_planner());
        assert!(!DecisionStrategy::ResearchOnly.includes_review());
    }

    #[test]
    fn mode_parses_all_variants() {
        assert_eq!(DecisionMode::parse("rule"), Some(DecisionMode::Rule));
        assert_eq!(
            DecisionMode::parse("jev-shadow"),
            Some(DecisionMode::JevShadow)
        );
        assert_eq!(DecisionMode::parse("active"), Some(DecisionMode::JevActive));
        assert_eq!(DecisionMode::parse("nonsense"), None);
    }

    #[test]
    fn confidence_thresholds_are_centralized() {
        assert_eq!(
            DecisionConfidence::from_probability(0.9),
            DecisionConfidence::High
        );
        assert_eq!(
            DecisionConfidence::from_probability(0.75),
            DecisionConfidence::High
        );
        assert_eq!(
            DecisionConfidence::from_probability(0.6),
            DecisionConfidence::Uncertain
        );
        assert_eq!(
            DecisionConfidence::from_probability(0.45),
            DecisionConfidence::Uncertain
        );
        assert_eq!(
            DecisionConfidence::from_probability(0.44),
            DecisionConfidence::Low
        );
        assert_eq!(
            DecisionConfidence::from_probability(f32::NAN),
            DecisionConfidence::Low
        );
        assert!(DecisionConfidence::Low.forces_fallback());
        assert!(!DecisionConfidence::High.forces_fallback());
    }

    #[test]
    fn request_truncates_and_minimizes() {
        let long = "深".repeat(1000);
        let request = DecisionRequest::new(&long);
        assert_eq!(request.message.chars().count(), DECISION_MESSAGE_MAX_CHARS);
        // 不携带任何实体 label / 正文字段。
        assert!(request.entity_kind.is_none());
    }

    #[test]
    fn clamp_drops_unavailable_workers_and_forces_direct() {
        let available = vec!["research".to_string()];
        let result = DecisionResult::direct("rule", "x").clone();
        let mut result = result;
        result.strategy = DecisionStrategy::BoundedMultiAgent;
        result.workers = vec!["research".into(), "planner".into(), "unknown".into()];
        result.parallelism = 99;
        let clamped = result.clamp(&available, 2);
        assert_eq!(clamped.workers, vec!["research".to_string()]);
        assert_eq!(clamped.parallelism, 2);
        assert_eq!(clamped.strategy, DecisionStrategy::BoundedMultiAgent);

        let empty = DecisionResult {
            strategy: DecisionStrategy::ResearchOnly,
            workers: vec!["ghost".into()],
            ..DecisionResult::direct("rule", "x")
        };
        let clamped = empty.clamp(&available, 4);
        assert_eq!(clamped.strategy, DecisionStrategy::Direct);
    }

    #[test]
    fn orchestration_disallowed_when_budget_zero() {
        let request = DecisionRequest {
            budget_tier: BudgetTier::None,
            available_workers: vec!["research".into()],
            ..DecisionRequest::default()
        };
        assert!(!request.allows_orchestration());
        let off = DecisionRequest {
            multi_agent_off: true,
            available_workers: vec!["research".into()],
            ..DecisionRequest::default()
        };
        assert!(!off.allows_orchestration());
    }

    #[test]
    fn telemetry_carries_no_content() {
        let telemetry = DecisionTelemetry {
            mode: DecisionMode::JevShadow,
            provider: "rule",
            strategy: DecisionStrategy::ResearchOnly,
            confidence: DecisionConfidence::High,
            reason_code: "cross_module_request",
            decision_latency_ms: 12,
            fallback: false,
            shadow: Some(DecisionShadowRecord {
                rule_strategy: DecisionStrategy::ResearchOnly,
                shadow_strategy: DecisionStrategy::Direct,
                agreed: false,
                shadow_confidence: DecisionConfidence::Uncertain,
                shadow_latency_ms: 40,
            }),
            workers: vec!["research".into()],
        };
        let json = serde_json::to_string(&telemetry).unwrap();
        assert!(!json.contains("message"));
        assert!(json.contains("cross_module_request"));
    }
}
