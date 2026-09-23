//! Jev（TypeSafe System One model）Decision Provider（V10 §22-§28）。
//!
//! **隔离铁律（§22）**：所有 Jev-specific 内容（HTTP endpoint、Bearer key、
//! `jev-latest` 模型名、`noul`/`choice`/`score` 问题类型、概率分布）只存在于
//! 本模块（infrastructure adapter）。core/application 只认识
//! `devtoolbox_core::agents::DecisionProvider`。
//!
//! API 事实核对（2026-09，https://docs.typesafe.ai/api）：
//! - `POST https://api.typesafe.ai/v1/systemone`
//! - `Authorization: Bearer <API_KEY>`；`Content-Type: application/json`
//! - body：`{ "state": …, "model": "jev-latest", "questions": { "<id>": Question } }`
//! - `choice` 问题需 `criteria: { option: rubric|null }`；返回
//!   `{ "choice": "…", "probabilities": {…}, "confidence": 0..1 }`
//! - 错误码：401 未授权 / 422 体验证失败 / 429 限流 / 529 过载
//!
//! 第一版只问两个窄问题（§24）：
//! 1. `strategy`（choice）：选编排策略；
//! 2. `review_required`（noul）：是否需要 review。
//!
//! Jev **不**执行工具、不改权限、不写 Memory、不触碰 SYSTEM/SafeAction。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use devtoolbox_core::agents::{
    DecisionConfidence, DecisionProvider, DecisionProviderError, DecisionRequest, DecisionResult,
    DecisionStrategy,
};
#[cfg(test)]
use devtoolbox_core::personal_ai::ProviderErrorKind;

/// Provider 标识（遥测 / trace UI）。
pub const PROVIDER_NAME: &str = "jev";

/// 默认 endpoint（官方文档）。
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";

/// 默认模型别名（官方文档：flagship System One model）。
pub const DEFAULT_MODEL: &str = "jev-latest";

/// 默认超时（秒）：决策必须便宜；超时即回落 rule（§30）。
pub const DEFAULT_TIMEOUT_SECS: u64 = 5;

/// Jev 配置（来自应用设置 / env；key 永不进日志与前端）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JevConfig {
    /// API base；缺省官方 `https://api.typesafe.ai`。
    pub base_url: Option<String>,
    /// API key（`Authorization: Bearer`）。空 = 未配置。
    pub api_key: Option<String>,
    /// 模型别名；缺省 `jev-latest`。
    pub model: Option<String>,
    /// 超时秒（1..60；缺省 5）。
    pub timeout_secs: Option<u64>,
}

impl JevConfig {
    /// 是否已配置（key 非空即视为已配置；base/model 有默认值）。
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
    }

    fn base(&self) -> String {
        self.base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string()
    }

    fn model(&self) -> String {
        self.model
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(DEFAULT_MODEL)
            .to_string()
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(
            self.timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .clamp(1, 60),
        )
    }
}

/// 决策问题 id（稳定；答案按同一 key 返回）。
const QUESTION_STRATEGY: &str = "strategy";
const QUESTION_REVIEW: &str = "review_required";

/// 策略候选项（choice 的 criteria keys）——与 `DecisionStrategy` 一一对应。
const OPTION_DIRECT: &str = "direct";
const OPTION_RESEARCH_ONLY: &str = "research_only";
const OPTION_PLAN_AND_RESEARCH: &str = "plan_and_research";
const OPTION_RESEARCH_AND_REVIEW: &str = "research_and_review";
const OPTION_BOUNDED_MULTI_AGENT: &str = "bounded_multi_agent";

/// 供 Jev 参考的 state（**最小化**：路由所需信号，不含正文/实体名/文件内容）。
fn decision_state(request: &DecisionRequest) -> serde_json::Value {
    serde_json::json!({
        "message": request.message,
        "module": request.module,
        "page": request.page,
        "entity_kind": request.entity_kind,
        "cross_module_hint": request.cross_module_hint,
        "external_evidence_needed": request.external_evidence_needed,
        "explicit_deep": request.explicit_deep,
        "multi_agent_off": request.multi_agent_off,
        "available_workers": request.available_workers,
        "available_tool_groups": request.available_tool_groups,
        "budget_tier": request.budget_tier.as_str(),
        "capability_constraints": request.capability_constraints,
        "instruction": "只决定编排策略；不得假设任何权限、工具执行或数据写入能力",
    })
}

fn strategy_question() -> JevQuestion {
    JevQuestion {
        kind: "choice".into(),
        instructions: serde_json::json!({
            "question": "这个用户请求应该使用哪种编排策略？",
            "guidance": "simple factual / 单模块问答选 direct；跨模块或需要取证选 research_only；需要规划框架选 plan_and_research；需要审查选 research_and_review；显式深度研究选 bounded_multi_agent。预算不足或用户关闭多 agent 时必须选 direct。",
        }),
        criteria: serde_json::json!({
            OPTION_DIRECT: "简单事实问答、单模块检索、用户关闭多 agent、预算不足",
            OPTION_RESEARCH_ONLY: "需要跨模块取证/比较，但不需要规划或审查",
            OPTION_PLAN_AND_RESEARCH: "需要先规划框架再取证",
            OPTION_RESEARCH_AND_REVIEW: "取证后需要证据充分性审查",
            OPTION_BOUNDED_MULTI_AGENT: "显式深度研究：规划 + 取证 + 审查",
        }),
    }
}

fn review_question() -> JevQuestion {
    JevQuestion {
        kind: "noul".into(),
        instructions: serde_json::json!({
            "question": "这个请求的结论是否需要独立 review 才能可信？",
            "guidance": "高风险结论、跨来源矛盾、显式深度研究为是；简单问答为否。",
        }),
        criteria: serde_json::json!({
            "true": "结论影响重要判断、需要证据核查",
            "false": "简单事实或单来源即可信",
        }),
    }
}

/// Jev 传输端口（§24：HTTP 与测试替身共用同一契约）。
#[async_trait]
pub trait JevTransport: Send + Sync {
    /// 发起一次评估；实现方必须自带超时。
    async fn evaluate(
        &self,
        body: &JevRequestBody,
    ) -> Result<JevResponseBody, DecisionProviderError>;
}

/// Jev HTTP 传输（生产实现；key 只进 header，不进日志）。
pub struct JevHttpTransport {
    client: reqwest::Client,
    config: JevConfig,
}

impl JevHttpTransport {
    #[must_use]
    pub fn new(client: reqwest::Client, config: JevConfig) -> Self {
        Self { client, config }
    }
}

#[async_trait]
impl JevTransport for JevHttpTransport {
    async fn evaluate(
        &self,
        body: &JevRequestBody,
    ) -> Result<JevResponseBody, DecisionProviderError> {
        let url = format!("{}/v1/systemone", self.config.base());
        let mut builder = self
            .client
            .post(&url)
            .timeout(self.config.timeout())
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(body);
        if let Some(key) = self.config.api_key.as_deref() {
            builder = builder.bearer_auth(key);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| DecisionProviderError::transport(error.to_string()))?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(DecisionProviderError::unavailable("jev unauthorized"));
        }
        if status.as_u16() == 429 {
            return Err(DecisionProviderError::unavailable("jev rate limited"));
        }
        if status.as_u16() == 529 || status.is_server_error() {
            return Err(DecisionProviderError::unavailable("jev overloaded"));
        }
        if !status.is_success() {
            return Err(DecisionProviderError::invalid_response(format!(
                "jev returned {}",
                status.as_u16()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| DecisionProviderError::transport(error.to_string()))?;
        serde_json::from_slice::<JevResponseBody>(&bytes)
            .map_err(|error| DecisionProviderError::invalid_response(error.to_string()))
    }
}

/// 请求体（官方形状）。
#[derive(Debug, Serialize)]
pub struct JevRequestBody {
    pub state: serde_json::Value,
    pub model: String,
    pub questions: serde_json::Map<String, serde_json::Value>,
}

/// 响应体（官方形状；只取需要的字段）。
#[derive(Debug, Deserialize)]
pub struct JevResponseBody {
    pub model: Option<String>,
    pub answers: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub usage: Option<JevUsage>,
}

/// 用量（遥测；不含计费推断）。
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct JevUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// 单个问题（choice / noul）。
#[derive(Debug, Serialize)]
pub struct JevQuestion {
    #[serde(rename = "type")]
    pub kind: String,
    pub instructions: serde_json::Value,
    pub criteria: serde_json::Value,
}

/// Jev Decision Provider（生产 + Fake 共用逻辑，只差 transport）。
pub struct JevDecisionProvider {
    transport: Arc<dyn JevTransport>,
    config: JevConfig,
    model: String,
}

impl JevDecisionProvider {
    #[must_use]
    pub fn new(transport: Arc<dyn JevTransport>, config: JevConfig) -> Self {
        let model = config.model();
        Self {
            transport,
            config,
            model,
        }
    }

    /// 从 HTTP client 构造（生产路径）。
    #[must_use]
    pub fn http(client: reqwest::Client, config: JevConfig) -> Self {
        Self::new(
            Arc::new(JevHttpTransport::new(client, config.clone())),
            config,
        )
    }

    /// 构造请求体（导出供测试断言序列化形状）。
    #[must_use]
    pub fn request_body(request: &DecisionRequest, model: &str) -> JevRequestBody {
        let mut questions = serde_json::Map::new();
        questions.insert(
            QUESTION_STRATEGY.into(),
            serde_json::to_value(strategy_question()).unwrap_or(serde_json::Value::Null),
        );
        questions.insert(
            QUESTION_REVIEW.into(),
            serde_json::to_value(review_question()).unwrap_or(serde_json::Value::Null),
        );
        JevRequestBody {
            state: decision_state(request),
            model: model.to_string(),
            questions,
        }
    }
}

#[async_trait]
impl DecisionProvider for JevDecisionProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn is_available(&self) -> bool {
        self.config.is_configured()
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResult, DecisionProviderError> {
        if !self.config.is_configured() {
            return Err(DecisionProviderError::unavailable(
                "jev api key is not configured",
            ));
        }
        let body = Self::request_body(request, &self.model);
        let response = self.transport.evaluate(&body).await?;
        let strategy = parse_strategy_answer(response.answers.get(QUESTION_STRATEGY))?;
        let review = parse_noul_answer(response.answers.get(QUESTION_REVIEW));
        let confidence = answer_confidence(response.answers.get(QUESTION_STRATEGY));
        let workers = strategy_workers(strategy, review);
        Ok(DecisionResult {
            strategy,
            workers: workers.into_iter().map(str::to_string).collect(),
            parallelism: strategy_parallelism(strategy),
            review_required: review,
            confidence: DecisionConfidence::from_probability(confidence),
            reason_code: "jev_decision",
            provider: PROVIDER_NAME,
        })
    }
}

/// 从 choice 答案解析策略；缺失 / 非法 / 未知选项 → InvalidResponse（§28）。
fn parse_strategy_answer(
    answer: Option<&serde_json::Value>,
) -> Result<DecisionStrategy, DecisionProviderError> {
    let answer =
        answer.ok_or_else(|| DecisionProviderError::invalid_response("missing strategy answer"))?;
    let raw = answer
        .get("choice")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| DecisionProviderError::invalid_response("strategy answer missing choice"))?;
    DecisionStrategy::parse(raw)
        .ok_or_else(|| DecisionProviderError::invalid_response("unknown strategy option"))
}

/// noul 答案 → 是否 review（缺失 = false；非 [0,1] = 忽略）。
fn parse_noul_answer(answer: Option<&serde_json::Value>) -> bool {
    answer
        .and_then(|value| value.get("noul"))
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|value| value >= 0.5)
}

/// choice 答案的 confidence（缺失 → 0 → Low → engine 回落 rule）。
fn answer_confidence(answer: Option<&serde_json::Value>) -> f32 {
    answer
        .and_then(|value| value.get("confidence"))
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(0.0)
}

/// 策略 → worker 集（与 Rule 基线保持同形状；engine clamp 到可用集合）。
fn strategy_workers(strategy: DecisionStrategy, review: bool) -> Vec<&'static str> {
    use DecisionStrategy::{
        BoundedMultiAgent, Direct, PlanAndResearch, ResearchAndReview, ResearchOnly,
    };
    match strategy {
        Direct => Vec::new(),
        ResearchOnly => vec!["research", "research"],
        PlanAndResearch => vec!["planner", "research", "research"],
        ResearchAndReview => vec!["research", "research", "reviewer"],
        BoundedMultiAgent => vec!["planner", "research", "research", "reviewer"],
    }
    .into_iter()
    .filter(|worker| *worker != "reviewer" || review)
    .collect()
}

fn strategy_parallelism(strategy: DecisionStrategy) -> usize {
    match strategy {
        DecisionStrategy::Direct => 1,
        DecisionStrategy::PlanAndResearch => 3,
        _ => 2,
    }
}

/// Fake transport（§28）：脚本化响应 / 错误，供无 key 环境下测试全链路。
#[derive(Default)]
pub struct FakeJevTransport {
    /// 按顺序返回的脚本（Ok / Err）。
    script: std::sync::Mutex<
        std::collections::VecDeque<Result<JevResponseBody, DecisionProviderError>>,
    >,
    /// 记录收到的请求体数量（断言 provider 只发一次）。
    received: std::sync::atomic::AtomicUsize,
}

impl FakeJevTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个 choice 响应。
    pub fn push_choice(&self, option: &str, confidence: f32, review: f64) {
        let mut answers = serde_json::Map::new();
        answers.insert(
            QUESTION_STRATEGY.into(),
            serde_json::json!({
                "type": "choice",
                "choice": option,
                "probabilities": { option: confidence },
                "confidence": confidence,
            }),
        );
        answers.insert(
            QUESTION_REVIEW.into(),
            serde_json::json!({"type": "noul", "noul": review}),
        );
        self.push(Ok(JevResponseBody {
            model: Some("jev-fake".into()),
            answers,
            usage: Some(JevUsage {
                input_tokens: 1,
                output_tokens: 1,
            }),
        }));
    }

    /// 追加一个原始响应（供构造非法答案）。
    pub fn push(&self, response: Result<JevResponseBody, DecisionProviderError>) {
        self.script
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(response);
    }

    pub fn received(&self) -> usize {
        self.received.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl JevTransport for FakeJevTransport {
    async fn evaluate(
        &self,
        _body: &JevRequestBody,
    ) -> Result<JevResponseBody, DecisionProviderError> {
        self.received
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.script
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
            .unwrap_or_else(|| {
                Err(DecisionProviderError::invalid_response(
                    "fake script exhausted",
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::agents::BudgetTier;

    fn configured() -> JevConfig {
        JevConfig {
            api_key: Some("test-key".into()),
            ..JevConfig::default()
        }
    }

    fn request() -> DecisionRequest {
        DecisionRequest {
            request_id: "req-1".into(),
            message: "比较文档和服务日志的差异".into(),
            module: None,
            page: None,
            entity_kind: None,
            cross_module_hint: true,
            external_evidence_needed: false,
            explicit_deep: false,
            multi_agent_off: false,
            available_workers: vec!["research".into(), "planner".into(), "reviewer".into()],
            available_tool_groups: vec!["documents".into(), "server".into()],
            budget_tier: BudgetTier::Full,
            capability_constraints: vec!["read_only".into()],
        }
    }

    #[tokio::test]
    async fn happy_path_returns_jev_strategy() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push_choice(OPTION_RESEARCH_ONLY, 0.9, 0.0);
        let provider = JevDecisionProvider::new(fake, configured());
        let result = provider.decide(&request()).await.expect("decision");
        assert_eq!(result.strategy, DecisionStrategy::ResearchOnly);
        assert_eq!(result.provider, "jev");
        assert!(!result.review_required);
        assert_eq!(result.confidence, DecisionConfidence::High);
    }

    #[tokio::test]
    async fn noul_answer_drives_review_flag() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push_choice(OPTION_BOUNDED_MULTI_AGENT, 0.8, 0.9);
        let provider = JevDecisionProvider::new(fake, configured());
        let result = provider.decide(&request()).await.expect("decision");
        assert_eq!(result.strategy, DecisionStrategy::BoundedMultiAgent);
        assert!(result.review_required);
        assert!(result.workers.contains(&"reviewer".to_string()));
    }

    #[tokio::test]
    async fn review_false_drops_reviewer_worker() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push_choice(OPTION_RESEARCH_AND_REVIEW, 0.8, 0.1);
        let provider = JevDecisionProvider::new(fake, configured());
        let result = provider.decide(&request()).await.expect("decision");
        assert!(!result.review_required);
        assert!(!result.workers.contains(&"reviewer".to_string()));
    }

    #[tokio::test]
    async fn missing_answer_is_invalid_response() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push(Ok(JevResponseBody {
            model: Some("jev-fake".into()),
            answers: serde_json::Map::new(),
            usage: None,
        }));
        let provider = JevDecisionProvider::new(fake, configured());
        let error = provider.decide(&request()).await.expect_err("must fail");
        assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    }

    #[tokio::test]
    async fn unknown_option_is_invalid_response() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push_choice("teleport_agents", 0.9, 0.0);
        let provider = JevDecisionProvider::new(fake, configured());
        let error = provider.decide(&request()).await.expect_err("must fail");
        assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    }

    #[tokio::test]
    async fn missing_confidence_becomes_low_and_forces_fallback() {
        let fake = Arc::new(FakeJevTransport::new());
        let mut answers = serde_json::Map::new();
        answers.insert(
            QUESTION_STRATEGY.into(),
            serde_json::json!({"type": "choice", "choice": OPTION_DIRECT}),
        );
        answers.insert(
            QUESTION_REVIEW.into(),
            serde_json::json!({"type": "noul", "noul": 0.0}),
        );
        fake.push(Ok(JevResponseBody {
            model: None,
            answers,
            usage: None,
        }));
        let provider = JevDecisionProvider::new(fake, configured());
        let result = provider.decide(&request()).await.expect("decision");
        assert_eq!(result.confidence, DecisionConfidence::Low);
        assert!(result.confidence.forces_fallback());
    }

    #[tokio::test]
    async fn transport_error_propagates_for_engine_fallback() {
        let fake = Arc::new(FakeJevTransport::new());
        fake.push(Err(DecisionProviderError::unavailable("jev rate limited")));
        let provider = JevDecisionProvider::new(fake, configured());
        let error = provider.decide(&request()).await.expect_err("must fail");
        assert_eq!(error.kind, ProviderErrorKind::Unavailable);
    }

    #[tokio::test]
    async fn unconfigured_key_is_unavailable() {
        let fake = Arc::new(FakeJevTransport::new());
        let provider = JevDecisionProvider::new(fake, JevConfig::default());
        assert!(!provider.is_available());
        let error = provider.decide(&request()).await.expect_err("must fail");
        assert_eq!(error.kind, ProviderErrorKind::Unavailable);
        assert!(error.message.contains("not configured"));
    }

    #[tokio::test]
    async fn request_body_matches_official_shape() {
        let body = JevDecisionProvider::request_body(&request(), DEFAULT_MODEL);
        assert_eq!(body.model, "jev-latest");
        let questions = body.questions;
        assert_eq!(questions["strategy"]["type"], "choice");
        assert_eq!(questions["review_required"]["type"], "noul");
        // state 只含路由信号：无实体 label、无正文长文本（message 已截断）。
        let state = body.state;
        assert!(state["available_workers"].is_array());
        assert!(state["message"].as_str().unwrap().len() <= 256);
        assert!(state.get("api_key").is_none());
        assert!(state.get("memory").is_none());
    }

    #[test]
    fn config_defaults_are_safe() {
        let config = JevConfig::default();
        assert!(!config.is_configured());
        assert_eq!(config.base(), DEFAULT_BASE_URL);
        assert_eq!(config.model(), DEFAULT_MODEL);
        assert_eq!(config.timeout(), Duration::from_secs(5));
    }
}
