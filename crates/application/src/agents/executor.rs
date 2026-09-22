//! Agent 执行器（V9 Gate 3，§39）。
//!
//! 职责（§39）：load profile → build prompt/context → enforce budget →
//! call ChatModelProvider → handle tool loop → return structured result。
//!
//! **复用共享 tool loop**（§40）；不复制 `agent.rs`。
//! **不持有任何业务能力**：工具执行全部经 `ToolRegistry`（§2）。

use std::sync::Arc;
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use devtoolbox_core::agents::{
    AgentBudget, AgentRunState, BudgetUsage, DelegationResult, DelegationStatus, TaskEnvelope,
    TokenUsage, ToolCallRecord,
};
use devtoolbox_core::personal_ai::{ChatMessage, ChatModelProvider, ChatRole, ToolSpec};

use crate::personal_ai::registry::ToolRegistry;
use crate::personal_ai::runtime::{ToolLoopConfig, run_tool_loop};

use super::prompt::build_worker_system;

/// 执行器依赖（组合根注入）。
pub struct AgentExecutorDeps {
    pub provider: Arc<dyn ChatModelProvider>,
    pub registry: Arc<ToolRegistry>,
}

/// 单次 run 的产出（orchestrator 收集用）。
#[derive(Clone, Debug)]
pub struct RunOutcome {
    pub result: DelegationResult,
    pub state: AgentRunState,
    /// 该 run 消耗的预算（回填给全局，§54）。
    pub usage: BudgetUsage,
}

/// Agent 执行器（无状态；per-run 状态在 `TaskEnvelope` / `RunOutcome` 里，§32）。
pub struct AgentExecutor {
    deps: AgentExecutorDeps,
}

impl AgentExecutor {
    #[must_use]
    pub fn new(deps: AgentExecutorDeps) -> Self {
        Self { deps }
    }

    /// 该 agent 在此任务下可用的工具 specs（能力过滤，§34-§36）。
    ///
    /// 顺序：registry 全部 → descriptor 静态规则 → task capability → 排序稳定。
    pub fn authorized_tools(
        &self,
        descriptor: &devtoolbox_core::agents::AgentDescriptor,
        task: &TaskEnvelope,
    ) -> Vec<ToolSpec> {
        self.deps
            .registry
            .specs()
            .into_iter()
            .filter(|spec| descriptor.allows_tool(spec))
            .filter(|spec| task.capabilities.allows(&spec.name))
            .collect()
    }

    /// 执行一个任务（§39）。**不 panic**：任何失败都变成 `DelegationResult`。
    pub async fn run(
        &self,
        descriptor: &devtoolbox_core::agents::AgentDescriptor,
        task: &TaskEnvelope,
        budget: &AgentBudget,
    ) -> RunOutcome {
        self.run_cancellable(descriptor, task, budget, None).await
    }

    /// 带取消令牌的执行（§58：Stop → 取消 pending/running child）。
    pub async fn run_cancellable(
        &self,
        descriptor: &devtoolbox_core::agents::AgentDescriptor,
        task: &TaskEnvelope,
        budget: &AgentBudget,
        cancel: Option<&CancellationToken>,
    ) -> RunOutcome {
        let started = Instant::now();
        let agent_id = descriptor.id.clone();

        // §58：开跑前检查取消（不留 zombie）。
        if cancel.is_some_and(|token| token.is_cancelled()) {
            let mut result = DelegationResult::failed(&task.task_id, &agent_id, "cancelled");
            result.status = DelegationStatus::Cancelled;
            result.duration_ms = started.elapsed().as_millis() as u64;
            return RunOutcome {
                result,
                state: AgentRunState::Cancelled,
                usage: BudgetUsage::default(),
            };
        }
        // §57：绝对截止与 per-agent 超时共同决定上限。
        if task.is_past_deadline(now_unix()) {
            return self.failed(task, &agent_id, "deadline_exceeded", started);
        }
        let tools = self.authorized_tools(descriptor, task);
        let tool_names: Vec<String> = tools.iter().map(|spec| spec.name.clone()).collect();

        let system = build_worker_system(
            descriptor,
            &task.objective,
            &task.instructions,
            &task.context_refs,
            &tool_names,
        );
        let messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: Some(system),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: Some(task.objective.clone()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        // 预算收敛到单 run（§54：child ≤ parent 剩余）。
        let max_rounds = task.max_steps.min(budget.max_steps).max(1);
        let outcome = run_tool_loop(
            self.deps.provider.as_ref(),
            &self.deps.registry,
            messages,
            &tools,
            ToolLoopConfig {
                max_rounds,
                // worker 低温度：结构化、可复现。
                temperature: 0.1,
            },
        )
        .await;

        let duration_ms = started.elapsed().as_millis() as u64;
        match outcome {
            Ok(loop_outcome) => {
                let usage = TokenUsage {
                    input_tokens: loop_outcome.usage.input_tokens as u32,
                    output_tokens: loop_outcome.usage.output_tokens as u32,
                    total_tokens: loop_outcome.usage.total_tokens as u32,
                };
                let tool_calls: Vec<ToolCallRecord> = loop_outcome
                    .tool_trace
                    .iter()
                    .map(|entry| ToolCallRecord {
                        tool: entry.tool.clone(),
                        ok: entry.ok,
                        duration_ms: entry.duration_ms,
                        error_code: entry.note.clone(),
                    })
                    .collect();
                // §98：结构化输出。解析失败 → status=Failed（fail-closed，不放行自然语言）。
                let structured = parse_worker_json(&loop_outcome.content);
                let (status, summary, errors) = match &structured {
                    Some(value) => (
                        DelegationStatus::Completed,
                        value
                            .get("summary")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        None,
                    ),
                    None => (
                        DelegationStatus::Failed,
                        String::new(),
                        Some("invalid_structured_output".to_string()),
                    ),
                };
                let sources = structured
                    .as_ref()
                    .map(collect_sources)
                    .unwrap_or_default();
                let result = DelegationResult {
                    task_id: task.task_id.clone(),
                    agent_id,
                    status,
                    structured_output: structured.unwrap_or(serde_json::json!({"ok": false})),
                    summary,
                    sources,
                    tool_calls: tool_calls.clone(),
                    usage,
                    duration_ms,
                    errors,
                };
                let budget_usage = BudgetUsage {
                    agents: 1,
                    steps: loop_outcome.rounds as usize,
                    tool_calls: tool_calls.len(),
                    tokens: usage.total_tokens,
                    elapsed_ms: duration_ms,
                };
                let state = if result.status.is_usable() {
                    AgentRunState::Completed
                } else {
                    AgentRunState::Failed
                };
                RunOutcome {
                    result,
                    state,
                    usage: budget_usage,
                }
            }
            Err(error) => {
                // §57/§62：超时与瞬时错误可重试；这里只标记，重试策略在 orchestrator。
                let state = if error.to_string().contains("tool rounds") {
                    AgentRunState::Failed
                } else {
                    AgentRunState::Failed
                };
                let mut result = DelegationResult::failed(
                    &task.task_id,
                    &agent_id,
                    &stable_error_code(&error),
                );
                result.duration_ms = duration_ms;
                let budget_usage = BudgetUsage {
                    agents: 1,
                    ..BudgetUsage::default()
                };
                RunOutcome {
                    result,
                    state,
                    usage: budget_usage,
                }
            }
        }
    }

    fn failed(
        &self,
        task: &TaskEnvelope,
        agent_id: &str,
        reason: &str,
        started: Instant,
    ) -> RunOutcome {
        let mut result = DelegationResult::failed(&task.task_id, agent_id, reason);
        result.duration_ms = started.elapsed().as_millis() as u64;
        RunOutcome {
            result,
            state: AgentRunState::Failed,
            usage: BudgetUsage {
                agents: 1,
                ..BudgetUsage::default()
            },
        }
    }
}

/// 解析 worker 的 JSON 输出（§98：只接受单个 JSON 对象）。
fn parse_worker_json(content: &str) -> Option<serde_json::Value> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 允许 ```json 围栏（模型常见形态），但只取第一段。
    let candidate = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|rest| rest.trim_end_matches("```").trim())
        .unwrap_or(trimmed);
    serde_json::from_str::<serde_json::Value>(candidate)
        .ok()
        .filter(|value| value.is_object())
}

/// 从结构化输出收集来源引用（§69）。
fn collect_sources(value: &serde_json::Value) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    match key.as_str() {
                        "source" | "source_id" => {
                            if let Some(text) = item.as_str()
                                && !text.is_empty()
                                && !out.iter().any(|known| known == text)
                            {
                                out.push(text.to_string());
                            }
                        }
                        // `sources` 约定为字符串数组。
                        "sources" => {
                            if let Some(items) = item.as_array() {
                                for entry in items.iter().filter_map(serde_json::Value::as_str) {
                                    if !entry.is_empty() && !out.iter().any(|known| known == entry) {
                                        out.push(entry.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    walk(item, out);
                }
            }
            _ => {}
        }
    }
    walk(value, &mut sources);
    sources
}


/// 稳定错误码（不含内容，§73）。
fn stable_error_code(error: &devtoolbox_core::AgentError) -> String {
    error.code().to_string()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::registry::ToolExecutor;
    use devtoolbox_core::agents::AgentDescriptor;

    struct EchoTool {
        spec: ToolSpec,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for EchoTool {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        async fn execute(
            &self,
            _arguments: serde_json::Value,
        ) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
            Ok(devtoolbox_core::ToolResult::ok(serde_json::json!({"ok": true})))
        }
    }

    /// 假 provider：返回固定 JSON（无工具调用）。
    struct JsonProvider {
        payload: String,
    }

    #[async_trait::async_trait]
    impl ChatModelProvider for JsonProvider {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn chat(
            &self,
            _request: devtoolbox_core::ChatRequest,
        ) -> Result<devtoolbox_core::ChatResponse, devtoolbox_core::ProviderError> {
            Ok(devtoolbox_core::ChatResponse {
                content: Some(self.payload.clone()),
                tool_calls: Vec::new(),
                usage: devtoolbox_core::ChatUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                    duration_ms: 1,
                },
            })
        }
    }

    fn executor_with(payload: &str, tools: Vec<ToolSpec>) -> AgentExecutor {
        let mut registry = ToolRegistry::new();
        for spec in tools {
            registry
                .register(Arc::new(EchoTool { spec }))
                .expect("register");
        }
        AgentExecutor::new(AgentExecutorDeps {
            provider: Arc::new(JsonProvider {
                payload: payload.to_string(),
            }),
            registry: Arc::new(registry),
        })
    }

    fn profile() -> AgentDescriptor {
        AgentDescriptor {
            id: "research".into(),
            role: devtoolbox_core::agents::AgentRole::Research,
            description: "d".into(),
            model_profile: "fast".into(),
            max_risk: devtoolbox_core::personal_ai::ToolRisk::Read,
            allowed_modules: Vec::new(),
            denied_tools: Vec::new(),
            max_steps: 4,
            max_tokens: 8_000,
            timeout_ms: 60_000,
            can_delegate: false,
        }
    }

    fn task(capabilities: Vec<String>) -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".into(),
            parent_task_id: "req-1".into(),
            objective: "收集证据".into(),
            instructions: String::new(),
            context_refs: Vec::new(),
            required_tools: Vec::new(),
            capabilities: devtoolbox_core::agents::DelegatedCapabilitySet {
                allowed_tools: capabilities,
                denied_tools: Vec::new(),
            },
            max_steps: 4,
            max_tokens: 8_000,
            timeout_ms: 60_000,
            deadline: 0,
            output_schema: None,
            priority: Default::default(),
            trace_id: "trace-1".into(),
        }
    }

    #[tokio::test]
    async fn structured_output_becomes_completed_result() {
        let executor = executor_with(
            r#"{"findings":[{"claim":"磁盘 85%","source":"service:self-tools"}]}"#,
            Vec::new(),
        );
        let outcome = executor
            .run(&profile(), &task(Vec::new()), &AgentBudget::default())
            .await;
        assert_eq!(outcome.state, AgentRunState::Completed);
        assert_eq!(outcome.result.status, DelegationStatus::Completed);
        assert_eq!(
            outcome.result.structured_output["findings"][0]["claim"],
            "磁盘 85%"
        );
        assert_eq!(outcome.result.sources, vec!["service:self-tools".to_string()]);
        assert_eq!(outcome.usage.tokens, 15);
        assert_eq!(outcome.usage.agents, 1);
    }

    #[tokio::test]
    async fn non_json_output_fails_closed() {
        // §98：结构化校验失败 → Failed（不让自然语言直接进 merge）。
        let executor = executor_with("我觉得可能是磁盘问题", Vec::new());
        let outcome = executor
            .run(&profile(), &task(Vec::new()), &AgentBudget::default())
            .await;
        assert_eq!(outcome.state, AgentRunState::Failed);
        assert_eq!(outcome.result.errors.as_deref(), Some("invalid_structured_output"));
    }

    #[tokio::test]
    async fn capability_set_filters_tools() {
        let spec = ToolSpec {
            name: "services.get_logs".into(),
            description: "logs".into(),
            input_schema: serde_json::json!({"type": "object"}),
            risk: devtoolbox_core::personal_ai::ToolRisk::Read,
            module: "server".into(),
        };
        let executor = executor_with("{}", vec![spec]);
        let tools = executor.authorized_tools(&profile(), &task(vec!["services.get_logs".into()]));
        assert_eq!(tools.len(), 1);
        // 不在能力集内 → 过滤掉（§34-§36）。
        assert!(executor.authorized_tools(&profile(), &task(vec!["other.tool".into()])).is_empty());
    }

    #[tokio::test]
    async fn past_deadline_is_rejected_before_running() {
        let executor = executor_with("{}", Vec::new());
        let mut task = task(Vec::new());
        task.deadline = 1;
        let outcome = executor.run(&profile(), &task, &AgentBudget::default()).await;
        assert_eq!(outcome.state, AgentRunState::Failed);
        assert_eq!(outcome.result.errors.as_deref(), Some("deadline_exceeded"));
    }

    #[test]
    fn fenced_json_is_accepted() {
        let value = parse_worker_json("```json\n{\"a\":1}\n```");
        assert_eq!(value, Some(serde_json::json!({"a": 1})));
        assert!(parse_worker_json("[1,2]").is_none(), "只接受对象");
        assert!(parse_worker_json("").is_none());
    }

    #[test]
    fn sources_are_collected_recursively_and_deduped() {
        let value = serde_json::json!({
            "findings": [
                {"claim": "a", "source": "s1"},
                {"claim": "b", "sources": ["s2", "s1"]}
            ]
        });
        assert_eq!(collect_sources(&value), vec!["s1".to_string(), "s2".to_string()]);
    }
}
