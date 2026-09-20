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
            self.config.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1),
        );

        let body = OpenAiChatRequest {
            model,
            messages: request.messages.iter().map(to_openai_message).collect(),
            tools: request
                .tools
                .iter()
                .map(|tool| OpenAiTool {
                    kind: "function",
                    function: OpenAiFunction {
                        name: &tool.name,
                        description: &tool.description,
                        parameters: &tool.parameters,
                    },
                })
                .collect(),
            temperature: request.temperature.unwrap_or(0.2),
        };

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
        parse_chat_response(&body).map_err(ProviderError::invalid_response)
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
        tool_calls: message.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|call| OpenAiToolCall {
                    id: &call.id,
                    kind: "function",
                    function: OpenAiFunctionCall {
                        name: &call.name,
                        arguments: serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect()
        }),
        tool_call_id: message.tool_call_id.as_deref(),
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
pub fn parse_chat_response(body: &[u8]) -> Result<ChatResponse, String> {
    let parsed: OpenAiChatResponse =
        serde_json::from_slice(body).map_err(|error| format!("invalid model response json: {error}"))?;
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
                    let id = call.id.unwrap_or_else(|| gen_call_id());
                    let arguments = call
                        .function
                        .arguments
                        .map(|raw| serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw)))
                        .unwrap_or_else(|| serde_json::Value::Null);
                    Some(ChatToolCall { id, name, arguments })
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
    fn malformed_arguments_fall_back_to_string() {
        let body = json!({
            "choices": [{"index": 0, "message": {
                "role": "assistant",
                "tool_calls": [{"id": "c1", "function": {"name": "t.x", "arguments": "not json"}}]
            }}]
        });
        let response = parse_chat_response(serde_json::to_vec(&body).unwrap().as_slice()).unwrap();
        assert_eq!(response.tool_calls[0].arguments, serde_json::Value::String("not json".into()));
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