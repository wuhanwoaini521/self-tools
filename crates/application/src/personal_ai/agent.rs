//! PersonalAgent（V4 §10/§30/§31）：统一 Agent 循环。
//!
//! ```text
//! User → AgentRequest → Prompt 组装 → ChatModelProvider
//!   └─ tool_calls? → ToolRegistry.validate+execute → ToolResult 回喂 → 再调（≤ max_tool_rounds）
//!   └─ 无 tool_calls → 解析最终 envelope → AgentResponse{message, actions, ui_blocks, tool_trace, usage}
//! ```
//!
//! 不负责：业务 DB 实现、前端路由、API key 存储（key 只进 Provider 配置）。

use std::sync::Arc;
use std::time::Instant;

use devtoolbox_core::{
    AgentError, AgentRequest, AgentResponse, ChatMessage, ChatModelProvider, ChatRole,
};

use crate::personal_ai::context::ContextBudget;
use crate::personal_ai::prompt::{
    assemble_messages, assemble_system, parse_agent_envelope, ui_snapshot,
};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};
use crate::personal_ai::runtime::{ToolLoopConfig, run_tool_loop};
use crate::personal_ai::session::SessionStore;

/// Agent 配置。
#[derive(Clone, Debug)]
pub struct AgentConfig {
    /// 工具循环安全上限（V4 §31）。
    pub max_tool_rounds: usize,
    /// 是否允许委派多 Agent（V9 §80：用户设置 / 显式关闭优先）。
    pub multi_agent_enabled: bool,
    /// 上下文预算（V4 §26）。
    pub context_budget: ContextBudget,
    /// 会话快照返回上限（UI 重建用）。
    pub snapshot_cap: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: 4,
            multi_agent_enabled: true,
            context_budget: ContextBudget::default(),
            snapshot_cap: 40,
        }
    }
}

/// 注册中心集合（组合根装配）。
#[derive(Default)]
pub struct PersonalHub {
    pub modules: ModuleRegistry,
    pub tools: ToolRegistry,
    /// 可选的通用检索增强（V6）：未装配 = 不做任何自动知识注入。
    pub retrieval: Option<Arc<dyn crate::personal_ai::retrieval::RetrievalAugmenter>>,
    /// 可选的多 Agent 编排（V9）：未装配 = 单 Agent 直接回答（§43）。
    pub orchestration: Option<Arc<crate::agents::orchestrator::OrchestrationService>>,

}

/// PersonalAgent：一个核心服务，服务所有模块（V4 Principle 2）。
pub struct PersonalAgent {
    provider: Arc<dyn ChatModelProvider>,
    hub: Arc<PersonalHub>,
    session: Arc<dyn SessionStore>,
    config: AgentConfig,
}

impl PersonalAgent {
    #[must_use]
    pub fn new(
        provider: Arc<dyn ChatModelProvider>,
        hub: Arc<PersonalHub>,
        session: Arc<dyn SessionStore>,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            hub,
            session,
            config,
        }
    }

    #[must_use]
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }
    #[must_use]
    pub fn hub(&self) -> &PersonalHub {
        &self.hub
    }
    #[must_use]
    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// 运行一轮 agent 循环。
    pub async fn run(&self, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| format!("once-{}", uid16()));

        // V11 §104：provider 不支持的多模态输入 → 受控拒绝，不假装分析。
        let parts = request.effective_parts();
        if let Some(reason) = self
            .provider
            .capabilities()
            .unsupported_reason(&parts)
        {
            return Err(AgentError::unsupported_input(reason));
        }

        // 有效模块集合（capabilities 过滤；空 = 全部）。
        let enabled_tools = self.enabled_tools(&request.capabilities);
        let mut system = assemble_system(
            &self.hub.modules.descriptors(),
            &request.app_context,
            self.hub
                .modules
                .context_provider(request.app_context.module.as_deref().unwrap_or_default())
                .as_deref(),
            &self.config.context_budget,
        );

        // 通用检索增强 stage（V6 §22/§55）：平台能力，无任何业务语义 ——
        // 是否注入、检索哪里、注入多少由注册的 `RetrievalAugmenter` 决定；
        // 未注册 / 无命中 / 检索失败 → 什么都不做（Principle 7）。
        if let Some(augmenter) = self.hub.retrieval.as_deref()
            && let Some(block) = augmenter.augment(&request.message, &request.app_context).await
        {
            system.push_str("\n\n");
            system.push_str(&block);
        }

        // 可选的多 Agent 编排（V9 §42 / V10 §35）：由 `DecisionEngine` 判定
        // 是否需要委派以及选哪种策略；简单请求不进这里（§43/§105）。
        // 编排结果作为 untrusted worker 结果注入 system（§97），最终回答仍由
        // 本 Agent 合成（§1 单用户入口）。
        let mut orchestration_trace: Option<devtoolbox_core::OrchestrationTraceView> = None;
        if let Some(orchestration) = self.hub.orchestration.as_deref() {
            let (delegating, decision_telemetry) = orchestration
                .decide_v10(
                    &request.message,
                    request.app_context.module.as_deref(),
                    request.app_context.page.as_deref(),
                    request
                        .app_context
                        .entity
                        .as_ref()
                        .map(|entity| entity.kind.as_str()),
                    self.config.multi_agent_enabled,
                    &devtoolbox_core::agents::AgentBudget::default(),
                )
                .await;
            if delegating {
                // V9-F1：session_id 前端可控 → 过 task id 校验，不合法则用 uid16。
                let request_id = if devtoolbox_core::agents::is_valid_task_id(&session_id) {
                    session_id.clone()
                } else {
                    format!("req-{}", uid16())
                };
                let plan = orchestration.plan_for_strategy(
                    &request_id,
                    &request.message,
                    decision_telemetry.strategy,
                );
                let parent_tools: Vec<String> = enabled_tools
                    .iter()
                    .map(|spec| spec.name.clone())
                    .collect();
                let budget = devtoolbox_core::agents::AgentBudget::default();
                let mut outcome = orchestration
                    .execute(
                        &request_id,
                        &request.message,
                        &plan,
                        &parent_tools,
                        &budget,
                        self.config.multi_agent_enabled,
                    )
                    .await;
                // V10 §20：把决策遥测挂到 trace（视图只暴露结构性字段）。
                outcome.trace.decision_telemetry = Some(decision_telemetry);
                orchestration_trace = Some(trace_view(&outcome.trace));
                // §97：worker 结果经围栏投影后注入（不可信数据，不是指令）。
                let fenced = crate::agents::orchestrator::untrusted_projection(
                    &serde_json::json!({
                        "decision": outcome.trace.decision.clone(),
                        "partial": outcome.partial,
                        "merged": outcome.merged,
                        "runs": outcome.trace.runs.iter().map(|run| serde_json::json!({
                            "task_id": run.task_id,
                            "agent_id": run.agent_id,
                            "status": run.status.as_str(),
                            "duration_ms": run.duration_ms,
                            "tool_calls": run.tool_calls,
                        })).collect::<Vec<_>>(),
                    }),
                    8_000,
                );
                let block = fenced;
                system.push_str("\n\n[多 Agent 编排结果]\n");
                system.push_str(&block);
            }
        }

        // 会话历史 + 用户消息
        let messages = self.session.load(&session_id);
        let chat_messages =
            assemble_messages(&messages, &request.message, &enabled_tools, &system);

        let started = Instant::now();
        // V9 Gate 3：工具循环抽取为共享 runtime（`personal_ai::runtime`），
        // PersonalAgent 与子 Agent 复用同一执行语义（§40）。
        let loop_outcome = run_tool_loop(
            self.provider.as_ref(),
            &self.hub.tools,
            chat_messages,
            &enabled_tools,
            ToolLoopConfig {
                max_rounds: self.config.max_tool_rounds,
                temperature: 0.2,
                max_tokens: None,
                max_tool_calls: 0,
            },
        )
        .await?;
        let mut total_usage = loop_outcome.usage;
        let trace = loop_outcome.tool_trace;
        let rounds = loop_outcome.rounds;

        // 最终轮：解析 envelope
        let content = loop_outcome.content;
        let (message, actions, ui_blocks) = parse_agent_envelope(&content);

        // 写入会话并返回
        let mut session_messages = messages;
        session_messages.push(ChatMessage::user(request.message.as_str()));
        if !content.is_empty() {
            session_messages.push(devtoolbox_core::ChatMessage {
                role: ChatRole::Assistant,
                content: Some(message.clone()),
                // 隐私：隐藏推理绝不写入会话历史（V11 §98）。
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        self.session.append(&session_id, &session_messages);

        total_usage.duration_ms = started.elapsed().as_millis() as u64;
        total_usage.tool_rounds = rounds;
        Ok(AgentResponse {
            session_id,
            message,
            actions,
            ui_blocks,
            tool_trace: trace,
            usage: Some(total_usage),
            messages: ui_snapshot(&session_messages, self.config.snapshot_cap),
            provider: Some(self.provider.name().to_string()),
            model: None,
            orchestration: orchestration_trace,
        })
    }

    fn enabled_tools(&self, capabilities: &[String]) -> Vec<devtoolbox_core::ToolSpec> {
        let all = self.hub.tools.specs();
        if capabilities.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|spec| capabilities.contains(&spec.module))
            .collect()
    }
}


/// `OrchestrationTrace` → 可序列化视图（§72：无 secret / 无正文 / 无隐藏推理）。
fn trace_view(trace: &crate::agents::orchestrator::OrchestrationTrace) -> devtoolbox_core::OrchestrationTraceView {
    devtoolbox_core::OrchestrationTraceView {
        trace_id: trace.trace_id.clone(),
        decision: trace.decision.clone(),
        plan_rationale: trace.plan_rationale.clone(),
        runs: trace
            .runs
            .iter()
            .map(|run| devtoolbox_core::OrchestrationRunView {
                task_id: run.task_id.clone(),
                agent_id: run.agent_id.clone(),
                state: run.state.as_str().to_string(),
                status: run.status.as_str().to_string(),
                duration_ms: run.duration_ms,
                tool_calls: run.tool_calls,
                tokens: run.tokens,
                error_code: run.error_code.clone(),
            })
            .collect(),
        review: trace.review.as_ref().map(|finding| finding.verdict.as_str().to_string()),
        merged: trace.merged,
        stopped_early: trace.stopped_early.map(str::to_string),
        decision_provider: trace
            .decision_telemetry
            .as_ref()
            .map(|telemetry| telemetry.provider.to_string()),
        decision_strategy: trace
            .decision_telemetry
            .as_ref()
            .map(|telemetry| telemetry.strategy.as_str().to_string()),
        decision_confidence: trace
            .decision_telemetry
            .as_ref()
            .map(|telemetry| telemetry.confidence.as_str().to_string()),
        decision_reason_code: trace
            .decision_telemetry
            .as_ref()
            .map(|telemetry| telemetry.reason_code.to_string()),
        decision_latency_ms: trace
            .decision_telemetry
            .as_ref()
            .map(|telemetry| telemetry.decision_latency_ms),
        decision_fallback: trace
            .decision_telemetry
            .as_ref()
            .is_some_and(|telemetry| telemetry.fallback),
        shadow_decision: trace.decision_telemetry.as_ref().and_then(|telemetry| {
            telemetry
                .shadow
                .as_ref()
                .map(|shadow| shadow.shadow_strategy.as_str().to_string())
        }),
        worker_count: trace.runs.len(),
    }
}

/// 8-hex 后缀的会话 id（无 crypto 依赖；仅用于一次性会话标识）。
fn uid16() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}", nanos % u128::from(u64::MAX))
}

#[cfg(test)]
mod tests {
    use devtoolbox_core::{ChatRequest, ToolResult};
    use super::*;
    use crate::personal_ai::registry::ToolExecutor;
    use devtoolbox_core::{
        ChatToolCall, ModuleDescriptor, ToolRisk, ToolSpec, personal_ai::AppContext,
    };
    use serde_json::json;
    use std::sync::RwLock;

    // ---- FakeChatModelProvider（V4 §61）----

    /// 可编程 Fake：按脚本依次返回（content / tool_calls / error）。
    pub struct FakeChatModelProvider {
        pub script: RwLock<VecDequeScript>,
        pub name_label: &'static str,
    }

    #[derive(Default)]
    pub struct VecDequeScript {
        pub steps: std::collections::VecDeque<
            Result<devtoolbox_core::ChatResponse, devtoolbox_core::ProviderError>,
        >,
    }

    impl FakeChatModelProvider {
        pub fn new() -> Self {
            Self {
                script: RwLock::new(VecDequeScript::default()),
                name_label: "fake",
            }
        }
        pub fn push_text(&self, text: &str) {
            self.script
                .write()
                .unwrap()
                .steps
                .push_back(Ok(devtoolbox_core::ChatResponse {
                    content: Some(text.to_string()),
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                    usage: devtoolbox_core::ChatUsage {
                        input_tokens: 1,
                        output_tokens: 2,
                        total_tokens: 3,
                        duration_ms: 1,
                    },
                }));
        }
        pub fn push_tool_call(&self, id: &str, name: &str, arguments: serde_json::Value) {
            self.script
                .write()
                .unwrap()
                .steps
                .push_back(Ok(devtoolbox_core::ChatResponse {
                    content: None,
                    reasoning_content: None,
                    tool_calls: vec![ChatToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments,
                    }],
                    usage: devtoolbox_core::ChatUsage::default(),
                }));
        }
        pub fn push_error(&self, error: devtoolbox_core::ProviderError) {
            self.script.write().unwrap().steps.push_back(Err(error));
        }
    }

    #[async_trait::async_trait]
    impl ChatModelProvider for FakeChatModelProvider {
        fn name(&self) -> &'static str {
            self.name_label
        }
        async fn chat(
            &self,
            _request: ChatRequest,
        ) -> Result<devtoolbox_core::ChatResponse, devtoolbox_core::ProviderError> {
            self.script
                .write()
                .unwrap()
                .steps
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(devtoolbox_core::ChatResponse {
                        content: Some("script exhausted".to_string()),
                        reasoning_content: None,
                        tool_calls: Vec::new(),
                        usage: devtoolbox_core::ChatUsage::default(),
                    })
                })
        }
    }

    // ---- FakeTool ----
    struct EchoTool {
        spec: ToolSpec,
        fail: bool,
        block: bool,
    }
    #[async_trait::async_trait]
    impl ToolExecutor for EchoTool {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
            if self.fail {
                return Err(AgentError::tool_execution_failed("tool blew up"));
            }
            if self.block {
                return Err(AgentError::tool_execution_failed("blocked by risk gate"));
            }
            Ok(ToolResult::ok(json!({"ok": true, "echo": arguments})))
        }
    }

    fn make_hub() -> Arc<PersonalHub> {
        let mut hub = PersonalHub::default();
        hub.modules
            .register(crate::personal_ai::registry::ModuleRegistration {
                descriptor: ModuleDescriptor {
                    id: "history".into(),
                    display_name: "History".into(),
                    description: "d".into(),
                    capabilities: vec!["search".into()],
                    tools: vec!["history.search".into()],
                },
                context_provider: None,
            })
            .unwrap();
        hub.tools
            .register(Arc::new(EchoTool {
                spec: ToolSpec {
                    name: "history.search".into(),
                    description: "search".into(),
                    input_schema: json!({"type": "object", "required": ["query"], "properties": {"query": {"type": "string"}}}),
                    risk: ToolRisk::Read,
                    module: "history".into(),
                },
                fail: false,
                block: false,
            }))
            .unwrap();
        hub.tools
            .register(Arc::new(EchoTool {
                spec: ToolSpec {
                    name: "history.fail".into(),
                    description: "failing".into(),
                    input_schema: json!({"type": "object"}),
                    risk: ToolRisk::Read,
                    module: "history".into(),
                },
                fail: true,
                block: false,
            }))
            .unwrap();
        Arc::new(hub)
    }

    fn agent(fake: FakeChatModelProvider) -> PersonalAgent {
        PersonalAgent::new(
            Arc::new(fake),
            make_hub(),
            Arc::new(crate::personal_ai::session::InMemorySessionStore::new()),
            AgentConfig {
                max_tool_rounds: 4,
                ..AgentConfig::default()
            },
        )
    }

    fn request(message: &str) -> AgentRequest {
        AgentRequest {
            message: message.to_string(),
            session_id: Some("test-session".into()),
            app_context: AppContext {
                module: Some("history".into()),
                ..AppContext::default()
            },
            capabilities: vec!["history".into()],
            locale: Some("zh-CN".into()),
            parts: Vec::new(),
        }
    }

    // Case A：model → text（无工具）
    #[tokio::test]
    async fn case_a_text_only() {
        let fake = FakeChatModelProvider::new();
        fake.push_text("直接回答");
        let agent = agent(fake);
        let response = agent.run(request("你好")).await.unwrap();
        assert_eq!(response.message, "直接回答");
        assert!(response.tool_trace.is_empty());
        assert_eq!(response.session_id, "test-session");
        assert_eq!(response.usage.unwrap().tool_rounds, 1);
        // 快照含 user + assistant
        assert_eq!(response.messages.len(), 2);
    }

    // Case B：model → tool call → result → final
    #[tokio::test]
    async fn case_b_tool_call_then_final() {
        let fake = FakeChatModelProvider::new();
        fake.push_tool_call("call_1", "history.search", json!({"query": "遵义会议"}));
        fake.push_text("遵义会议是一次重要会议。");
        let agent = agent(fake);
        let response = agent.run(request("查一下遵义会议")).await.unwrap();
        assert_eq!(response.message, "遵义会议是一次重要会议。");
        assert_eq!(response.tool_trace.len(), 1);
        assert_eq!(response.tool_trace[0].tool, "history.search");
        assert!(response.tool_trace[0].ok);
        assert_eq!(response.usage.unwrap().tool_rounds, 2);
    }

    // Case C：模型调用不存在的工具 → 受控失败（不 panic），并把失败
    // 以 ToolResult::fail 回喂，最终轮正常返回。
    #[tokio::test]
    async fn case_c_unknown_tool_controlled_failure() {
        let fake = FakeChatModelProvider::new();
        fake.push_tool_call("call_1", "history.ghost", json!({}));
        fake.push_text("没有找到这个工具。");
        let agent = agent(fake);
        let response = agent.run(request("调用 ghost")).await.unwrap();
        assert_eq!(response.tool_trace.len(), 1);
        assert!(!response.tool_trace[0].ok);
        assert!(response.tool_trace[0].note.is_some());
        assert_eq!(response.message, "没有找到这个工具。");
    }

    // Case D：工具执行异常 → 受控错误（ToolExecutionFailed → fail ToolResult）
    #[tokio::test]
    async fn case_d_tool_exception_controlled() {
        let fake = FakeChatModelProvider::new();
        fake.push_tool_call("call_1", "history.fail", json!({}));
        fake.push_text("工具出错了，但系统没有崩溃。");
        let agent = agent(fake);
        let response = agent.run(request("触发失败工具")).await.unwrap();
        assert!(!response.tool_trace[0].ok);
        assert_eq!(response.message, "工具出错了，但系统没有崩溃。");
    }

    // Case E：工具轮超过上限 → 安全停止（AgentError::MaxToolRounds）
    #[tokio::test]
    async fn case_e_max_tool_rounds_stops_safely() {
        let fake = FakeChatModelProvider::new();
        for _ in 0..10 {
            fake.push_tool_call("call_looped", "history.search", json!({"query": "x"}));
        }
        let mut config = AgentConfig {
            max_tool_rounds: 3,
            ..AgentConfig::default()
        };
        config.max_tool_rounds = 3;
        let hub = std::sync::Arc::new(PersonalHub::default());
        let agent = PersonalAgent::new(
            std::sync::Arc::new(fake),
            hub,
            std::sync::Arc::new(crate::personal_ai::session::InMemorySessionStore::new()),
            config,
        );
        let error = agent.run(request("循环")).await.unwrap_err();
        assert_eq!(error.code(), "personal_ai_max_tool_rounds");
    }

    // Provider 错误映射
    #[tokio::test]
    async fn provider_error_maps_to_agent_error() {
        let fake = FakeChatModelProvider::new();
        fake.push_error(devtoolbox_core::ProviderError::unavailable("no key"));
        let agent = agent(fake);
        let error = agent.run(request("hi")).await.unwrap_err();
        assert_eq!(error.code(), "personal_ai_model_unavailable");
    }

    // capabilities 过滤：非启用模块工具不注入
    #[tokio::test]
    async fn capabilities_filter_tools() {
        let fake = FakeChatModelProvider::new();
        fake.push_text("ok");
        let agent = agent(fake);
        let request = AgentRequest {
            message: "x".into(),
            session_id: None,
            app_context: AppContext::default(),
            capabilities: vec!["travel".into()], // history 不在启用列表
            locale: None,
            parts: Vec::new(),
        };
        let _ = agent.run(request).await.unwrap();
        // 若工具被注入，模型可能会调用；此场景仅确认不 panic 且正常返回。
    }
}
