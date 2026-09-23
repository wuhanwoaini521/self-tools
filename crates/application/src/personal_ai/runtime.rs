//! 共享 Agent 工具循环（V9 Gate 3，§40）。
//!
//! `PersonalAgent` 与子 Agent（`AgentExecutor`）**共用**这一个循环：
//! 不复制 `agent.rs` 的循环体，也不让 `agent.rs` 变成巨型 Orchestrator（§41）。
//!
//! 循环形态（与 V4 起一致）：
//! ```text
//! chat(tools) ──► tool_calls? ──► ToolRegistry.execute ──► 回feeding ──► 再调（≤ rounds）
//!              └─► 无 tool_calls ──► 返回最终 content + usage + trace
//! ```
//!
//! 本模块**不含** prompt 组装策略（调用方给 messages + tools），因此
//! PersonalAgent 与 worker 可以用不同 prompt 但同一执行语义。

use std::time::Instant;

use devtoolbox_core::{
    AgentError, AgentUsage, ChatMessage, ChatModelProvider, ChatRequest, ChatRole, ChatToolCall,
    ToolCallRequest, ToolResult, ToolTraceEntry,
};

use crate::personal_ai::registry::ToolRegistry;

/// 单轮循环的输出。
#[derive(Clone, Debug, Default)]
pub struct ToolLoopOutcome {
    /// 模型最终 content（未解析 envelope；由调用方决定语义）。
    pub content: String,
    /// 全循环累计 usage（§55）。
    pub usage: AgentUsage,
    /// 工具调用轨迹（只记名/成败/时长/错误码，不记参数全文，§73）。
    pub tool_trace: Vec<ToolTraceEntry>,
    /// 实际执行的轮数。
    pub rounds: u8,
}

/// 循环配置。
#[derive(Clone, Copy, Debug)]
pub struct ToolLoopConfig {
    /// 工具轮数上限（§53 budget 的 steps 维度）。
    pub max_rounds: usize,
    /// 采样温度（worker 用更低温度保证确定性）。
    pub temperature: f32,
    /// 单次 chat 的 token 上限（§53；None = provider 默认）。
    pub max_tokens: Option<u32>,
    /// 整个循环的工具调用硬上限（§53；0 = 不限制，由 max_rounds 兜底）。
    pub max_tool_calls: usize,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            max_rounds: 4,
            temperature: 0.2,
            max_tokens: None,
            max_tool_calls: 0,
        }
    }
}

/// provider 错误 → AgentError（与 `agent.rs` 同一映射）。
pub fn map_provider_error(error: devtoolbox_core::ProviderError) -> AgentError {
    use devtoolbox_core::ProviderErrorKind;
    match error.kind {
        ProviderErrorKind::Unavailable => AgentError::model_unavailable(error.message.clone()),
        ProviderErrorKind::Timeout => AgentError::provider_timeout(error.message.clone()),
        ProviderErrorKind::Transport => AgentError::provider(error.message.clone()),
        ProviderErrorKind::InvalidResponse => AgentError::provider(error.message.clone()),
    }
}

/// 执行共享工具循环。
///
/// `messages` 必须已包含 system + 历史 + 本轮 user 消息；`tools` 是**已授权**
/// 的 tool specs（调用方负责 capability 过滤，§34-§36）。
pub async fn run_tool_loop(
    provider: &dyn ChatModelProvider,
    registry: &ToolRegistry,
    messages: Vec<ChatMessage>,
    tools: &[devtoolbox_core::ToolSpec],
    config: ToolLoopConfig,
) -> Result<ToolLoopOutcome, AgentError> {
    let mut chat_messages = messages;
    let mut total_usage = AgentUsage::default();
    let mut trace: Vec<ToolTraceEntry> = Vec::new();
    let mut rounds = 0u8;
    let mut tool_calls = 0usize;

    loop {
        rounds += 1;
        if rounds > config.max_rounds as u8 {
            return Err(AgentError::max_tool_rounds(config.max_rounds));
        }
        let tool_specs: Vec<_> = tools
            .iter()
            .map(|spec| devtoolbox_core::ChatToolSpec {
                name: spec.name.clone(),
                description: spec.description.clone(),
                parameters: spec.input_schema.clone(),
            })
            .collect();
        let response = provider
            .chat(ChatRequest {
                messages: chat_messages.clone(),
                tools: tool_specs,
                temperature: Some(config.temperature),
                max_tokens: config.max_tokens,
            })
            .await
            .map_err(map_provider_error)?;
        total_usage = total_usage.combine(AgentUsage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            total_tokens: response.usage.total_tokens,
            duration_ms: response.usage.duration_ms,
            tool_rounds: 0,
        });
        // thinking 模式：本轮 assistant 消息必须带回 reasoning_content，
        // 否则下一轮请求被端点拒绝（The `reasoning_content` in the thinking
        // mode must be passed back to the API）。
        let reasoning = response.reasoning_content.clone();

        if response.tool_calls.is_empty() {
            return Ok(ToolLoopOutcome {
                content: response.content.unwrap_or_default(),
                usage: total_usage,
                tool_trace: trace,
                rounds,
            });
        }

        // §34-§36/§111：**执行侧**强制 allowlist —— 只执行调用方授权集合内的
        // 工具。模型（或被注入的 worker）即使返回其它已注册工具名也拒绝。
        // 这是 capability 交集唯一可信的强制点（discover 过滤只是提示）。
        for call in response.tool_calls {
            if !tools.iter().any(|spec| spec.name == call.name) {
                let failure = ToolResult::fail("tool_not_authorized");
                trace.push(ToolTraceEntry {
                    tool: call.name.clone(),
                    ok: false,
                    duration_ms: 0,
                    note: Some("tool_not_authorized".to_string()),
                });
                chat_messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: None,
                    reasoning_content: reasoning.clone(),
                    tool_calls: Some(vec![ChatToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    }]),
                    tool_call_id: None,
                });
                let result_json = serde_json::to_string(&failure)
                    .unwrap_or_else(|_| r#"{"ok":false,"error":"encode failed"}"#.to_string());
                chat_messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    content: Some(result_json),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some(call.id.clone()),
                });
                continue;
            }
            // §53：工具调用硬上限。
            if config.max_tool_calls > 0 && tool_calls >= config.max_tool_calls {
                return Err(AgentError::tool_execution_failed("tool_call_budget_exhausted"));
            }
            tool_calls += 1;
            let started = Instant::now();
            let tool_result = registry
                .execute(&ToolCallRequest {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .await
                .unwrap_or_else(|error| ToolResult::fail(error.message.clone()));
            let ok = tool_result.ok;
            let note = tool_result.error.clone();
            trace.push(ToolTraceEntry {
                tool: call.name.clone(),
                ok,
                duration_ms: started.elapsed().as_millis() as u64,
                note,
            });
            chat_messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: None,
                reasoning_content: reasoning.clone(),
                tool_calls: Some(vec![ChatToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                }]),
                tool_call_id: None,
            });
            let result_json =
                serde_json::to_string(&tool_result).unwrap_or_else(|_| {
                    r#"{"ok":false,"error":"serialization failed"}"#.to_string()
                });
            chat_messages.push(ChatMessage {
                role: ChatRole::Tool,
                content: Some(result_json),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some(call.id.clone()),
            });
        }
    }
}
