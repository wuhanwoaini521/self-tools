//! OpenAI-Compatible Chat Provider（V4 §27-§30）。
//!
//! 支持任意 OpenAI-compatible 端点（OpenAI / DeepSeek / OpenRouter / LiteLLM /
//! 自建 gateway / 本地 Ollama `/v1`）。不绑定具体模型。
//! 响应解析为纯函数 `parse_chat_response`，可离线单测。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use devtoolbox_core::personal_ai::{
    ChatMessage, ChatModelProvider, ChatRequest, ChatResponse, ChatRole, ChatToolCall, ChatToolSpec,
    ChatUsage, ProviderError,
};

/// AI 模型配置（来自应用设置 `AiSettings`；key 可选，本地 Ollama 可留空）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiModelConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    /// 秒；缺省 120。
    pub timeout_secs: Option<u64>,
}

impl AiModelConfig {
    #[must_use]
    pub fn is_configured(&self) -> bool {
        let base = self.base_url.as_deref().unwrap_or_default().trim();
        let model = self.model.as_deref().unwrap_or_default().trim();
        !base.is_empty() && !model.is_empty()
    }
}

const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// OpenAI-Compatible `/chat/completions` 实现（含 function calling）。
pub struct OpenAiCompatibleChatModelProvider {
    client: reqwest::Client,
    config: AiModelConfig,
}

impl OpenAiCompatibleChatModelProvider {
    #[must_use]
    pub fn new(client: reqwest::Client, config: AiModelConfig) -> Self {
        Self { client, config }
    }
}

#[async_trait]
impl ChatModelProvider for OpenAiCompatibleChatModelProvider {
    fn name(&self) -> &'static str {
        "openai-compatible"
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        if !self.config.is_configured() {
            return Err(ProviderError::unavailable(
                "ai model is not configured (base_url + model required)",
            ));
        }
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or_default()
            .trim_end_matches('/');
        let model = self.config.model.as_deref().unwrap_or_default();
        let url = format!("{base}/chat/completions");
        let timeout = std::time::Duration::from_secs(
            self.config
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .max(1),
        );

        // 工具名 wire 编码 + arguments 字符串化（先落 Vec，避免借用临时值）。
        let encoded_calls: Vec<Vec<EncodedToolCall>> = request
            .messages
            .iter()
            .map(|message| match &message.tool_calls {
                Some(calls) => encode_tool_calls(calls),
                None => Vec::new(),
            })
            .collect();
        let wire_tools: Vec<(String, &devtoolbox_core::personal_ai::ChatToolSpec)> = request
            .tools
            .iter()
            .map(|tool| (encode_tool_name(&tool.name), tool))
            .collect();
        let body = build_request_body(&request, model, &encoded_calls, &wire_tools);

        let mut builder = self
            .client
            .post(&url)
            .timeout(timeout)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .json(&body);
        if let Some(key) = self.config.api_key.as_deref().filter(|k| !k.is_empty()) {
            builder = builder.bearer_auth(key);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| ProviderError::transport(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::transport(format!(
                "model returned {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| ProviderError::transport(error.to_string()))?;
        let known: Vec<String> = request.tools.iter().map(|tool| tool.name.clone()).collect();
        parse_chat_response_with_tools(&body, &known).map_err(ProviderError::invalid_response)
    }
}

// ---------------------------------------------------------------------------
// 请求序列化
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    tools: Vec<OpenAiTool<'a>>,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall<'a> {
    id: &'a str,
    /// 固定 "function"（OpenAI 协议）。
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiFunctionCall<'a>,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionCall<'a> {
    name: &'a str,
    /// OpenAI 要求 arguments 是 JSON 字符串。
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiFunction<'a>,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

/// 工具名 wire 编码（§V11 兼容 OpenAI function name 规则 `^[a-zA-Z0-9_-]+$`）。
///
/// 内部契约是 `module.action`（registry 强制含 `.`），但 OpenAI 兼容 API 的
/// `tools[].function.name` 不接受 `.` — 发请求时把 `.` 换成 `_`，收到响应时换回来。
/// 这是一一映射（`.` ↔ `_`），不含歧义：内部名不允许 `_` 与 `.` 混用产生的冲突，
/// 因为反向替换只对**已知注册工具名**生效（白名单映射，见 `Router`）。
#[must_use]
pub fn encode_tool_name(internal: &str) -> String {
    internal.replace('.', "_")
}

/// wire 名 → 内部名（白名单：只解码注册表里存在的工具）。
///
/// `known` 为 registry 的内部名集合；找不到 = 原样返回 wire（让 provider 报
/// unknown tool，由执行端受控处理）。返回值可能是 `wire` 的借用，因此
/// 调用方持有的 `wire` 必须比结果活得更久（本 crate 内部调用点在 `chat()`，
/// `wire` 来源于反序列化后的局部结构，满足该约束）。
#[must_use]
pub fn decode_tool_name<'a>(
    wire: &'a str,
    known: &'a [String],
) -> std::borrow::Cow<'a, str> {
    if !wire.contains('_') {
        return std::borrow::Cow::Borrowed(wire);
    }
    for candidate in known {
        if encode_tool_name(candidate) == wire {
            return std::borrow::Cow::Owned(candidate.clone());
        }
    }
    std::borrow::Cow::Borrowed(wire)
}

/// assistant 消息里工具调用名的 wire 编码（需与 `OpenAiMessage` 同生命周期）。
#[derive(Debug, Serialize)]
struct EncodedToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: EncodedFunctionCall,
}

#[derive(Debug, Serialize)]
struct EncodedFunctionCall {
    name: String,
    /// OpenAI 要求 arguments 是 JSON 字符串。
    arguments: String,
}

/// 把消息里的工具调用转成 wire 形态（名字 `.` → `_`，arguments 序列化为字符串）。
fn encode_tool_calls(calls: &[ChatToolCall]) -> Vec<EncodedToolCall> {
    calls
        .iter()
        .map(|call| EncodedToolCall {
            id: call.id.clone(),
            kind: "function",
            function: EncodedFunctionCall {
                name: encode_tool_name(&call.name),
                arguments: serde_json::to_string(&call.arguments)
                    .unwrap_or_else(|_| "{}".to_string()),
            },
        })
        .collect()
}

fn to_openai_message(message: &ChatMessage) -> OpenAiMessage<'_> {
    let role = match message.role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    };
    OpenAiMessage {
        role,
        content: message.content.as_deref(),
        tool_calls: None,
        tool_call_id: message.tool_call_id.as_deref(),
    }
}

/// 请求体构造（wire 形态：工具名 `.` → `_`，arguments 序列化为字符串）。
///
/// `wire_tools` 由调用方预先构建（编码后的名字需与 body 同生命周期）。
fn build_request_body<'a>(
    request: &'a ChatRequest,
    model: &'a str,
    encoded_calls: &'a [Vec<EncodedToolCall>],
    wire_tools: &'a [(String, &'a ChatToolSpec)],
) -> OpenAiChatRequest<'a> {
    let messages: Vec<OpenAiMessage<'_>> = request
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let mut encoded = to_openai_message(message);
            encoded.tool_calls = encoded_calls.get(index).map(|calls| {
                calls
                    .iter()
                    .map(|call| OpenAiToolCall {
                        id: &call.id,
                        kind: call.kind,
                        function: OpenAiFunctionCall {
                            name: &call.function.name,
                            arguments: call.function.arguments.clone(),
                        },
                    })
                    .collect()
            });
            encoded
        })
        .collect();
    OpenAiChatRequest {
        model,
        messages,
        tools: wire_tools
            .iter()
            .map(|(name, tool)| OpenAiTool {
                kind: "function",
                function: OpenAiFunction {
                    name,
                    description: &tool.description,
                    parameters: &tool.parameters,
                },
            })
            .collect(),
        temperature: request.temperature.unwrap_or(0.2),
    }
}

// ---------------------------------------------------------------------------
// 响应解析（纯函数，可离线单测）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseToolCall {
    id: Option<String>,
    function: OpenAiResponseFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

/// 解析 `/chat/completions` 响应体为 `ChatResponse`。
///
/// 支持无 function calling 的老端点：`message.content` 为 null 且无 tool_calls
/// 时仍正常返回（content=None，调用方按无工具轮处理）。
///
/// `known_tools`：registry 的内部工具名（`module.action`）。模型回传的是 wire 名
/// （`.` → `_`），这里白名单解码回内部名；未知名原样保留，由执行端受控拒绝。
pub fn parse_chat_response_with_tools(
    body: &[u8],
    known_tools: &[String],
) -> Result<ChatResponse, String> {
    let parsed: OpenAiChatResponse = serde_json::from_slice(body)
        .map_err(|error| format!("invalid model response json: {error}"))?;
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| "model response has no choices".to_string())?;
    let tool_calls = choice
        .message
        .tool_calls
        .map(|calls| {
            calls
                .into_iter()
                .filter_map(|call| {
                    let name = call.function.name?;
                    let name = decode_tool_name(&name, known_tools).into_owned();
                    let id = call.id.unwrap_or_else(gen_call_id);
                    let arguments = call
                        .function
                        .arguments
                        .map(|raw| {
                            serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw))
                        })
                        .unwrap_or_else(|| serde_json::Value::Null);
                    Some(ChatToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let usage = parsed.usage.map(|usage| ChatUsage {
        input_tokens: usage.prompt_tokens.unwrap_or(0),
        output_tokens: usage.completion_tokens.unwrap_or(0),
        total_tokens: usage.total_tokens.unwrap_or(0),
        duration_ms: 0,
    });
    Ok(ChatResponse {
        content: choice.message.content,
        tool_calls,
        usage: usage.unwrap_or_default(),
    })
}

/// 无白名单的兼容入口（解码退化为「原样返回」，未知 wire 名保持 `_` 形态）。
pub fn parse_chat_response(body: &[u8]) -> Result<ChatResponse, String> {
    parse_chat_response_with_tools(body, &[])
}

fn gen_call_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("call_{:x}", nanos % u128::from(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_content_response() {
        let body = json!({
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "你好"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        let response = parse_chat_response(serde_json::to_vec(&body).unwrap().as_slice()).unwrap();
        assert_eq!(response.content.as_deref(), Some("你好"));
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[test]
    fn parses_tool_calls_response() {
        let body = json!({
            "choices": [{"index": 0, "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "history.search", "arguments": "{\"query\": \"遵義\"}"}}
                ]
            }, "finish_reason": "tool_calls"}],
            "usage": null
        });
        let response = parse_chat_response(serde_json::to_vec(&body).unwrap().as_slice()).unwrap();
        assert!(response.content.is_none());
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "history.search");
        assert_eq!(response.tool_calls[0].arguments["query"], "遵義");
    }

    #[test]
    fn tool_names_round_trip_through_wire_encoding() {
        // 内部契约 module.action；wire 上必须满足 OpenAI ^[a-zA-Z0-9_-]+$。
        for internal in [
            "history.search",
            "study-board.list",
            "study-board.save",
            "server.get_status",
            "services.get_logs",
            "memory.save",
        ] {
            let wire = encode_tool_name(internal);
            assert!(!wire.contains('.'), "{internal} → {wire} 仍含点");
            assert!(
                wire.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'),
                "{wire} 含非法字符"
            );
            let known = vec![internal.to_string()];
            assert_eq!(decode_tool_name(&wire, &known), internal);
        }
    }

    #[test]
    fn decode_unknown_wire_name_is_left_untouched() {
        let known = vec!["history.search".to_string()];
        // 未知工具：保持 wire 形态（执行端会以 unknown tool 受控拒绝）。
        assert_eq!(decode_tool_name("ghost_tool", &known), "ghost_tool");
        // 无下划线直接返回。
        assert_eq!(decode_tool_name("search", &known), "search");
    }

    #[test]
    fn parse_decodes_wire_tool_names_against_registry() {
        let body = json!({
            "choices": [{"index": 0, "message": {
                "role": "assistant",
                "tool_calls": [
                    {"id": "c1", "function": {"name": "study-board.list", "arguments": "{}"}},
                    {"id": "c2", "function": {"name": "unknown_tool", "arguments": "{}"}}
                ]
            }}]
        });
        let known = vec![
            "study-board.list".to_string(),
            "study-board.save".to_string(),
        ];
        let response =
            parse_chat_response_with_tools(serde_json::to_vec(&body).unwrap().as_slice(), &known)
                .unwrap();
        let names: Vec<&str> = response
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect();
        assert_eq!(names, vec!["study-board.list", "unknown_tool"]);
    }

    #[test]
    fn malformed_arguments_fall_back_to_string() {
        let body = json!({
            "choices": [{"index": 0, "message": {
                "role": "assistant",
                "tool_calls": [{"id": "c1", "function": {"name": "t.x", "arguments": "not json"}}]
            }}]
        });
        let response = parse_chat_response(serde_json::to_vec(&body).unwrap().as_slice()).unwrap();
        assert_eq!(
            response.tool_calls[0].arguments,
            serde_json::Value::String("not json".into())
        );
    }

    #[test]
    fn no_choices_is_error() {
        let body = json!({"choices": []});
        let error = parse_chat_response(serde_json::to_vec(&body).unwrap().as_slice()).unwrap_err();
        assert!(error.contains("no choices"));
    }

    #[test]
    fn config_requires_base_and_model() {
        let mut config = AiModelConfig::default();
        assert!(!config.is_configured());
        config.base_url = Some("http://localhost:11434/v1".into());
        assert!(!config.is_configured());
        config.model = Some("qwen2.5:7b".into());
        assert!(config.is_configured());
    }
}
