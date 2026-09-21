//! Chat 模型提供方契约（V4 §27/§28/§30）。
//!
//! 统一到**一个** `ChatModelProvider`（messages + tools + usage），
//! 供 PersonalAgent 使用；OpenAI-Compatible 为默认路线
//! （OpenAI / DeepSeek / OpenRouter / LiteLLM / 自建 gateway 均可接）。
//! travel 的 `LlmProvider` 本轮保持不动（V5 候选统一，见 PLAN §10）。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Provider 错误（transport 层）。kind 供上层归类到 `AgentError`。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderErrorKind {
    /// 未配置 / 不可用。
    Unavailable,
    /// 超时。
    Timeout,
    /// 网络 / HTTP 传输错误。
    Transport,
    /// 响应不可解析。
    InvalidResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", provider_kind_label(self.kind), self.message)
    }
}

impl std::error::Error for ProviderError {}

fn provider_kind_label(kind: ProviderErrorKind) -> &'static str {
    match kind {
        ProviderErrorKind::Unavailable => "model unavailable",
        ProviderErrorKind::Timeout => "model timeout",
        ProviderErrorKind::Transport => "model transport error",
        ProviderErrorKind::InvalidResponse => "model invalid response",
    }
}

impl ProviderError {
    #[must_use]
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Unavailable, message)
    }
    #[must_use]
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Timeout, message)
    }
    #[must_use]
    pub fn transport(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Transport, message)
    }
    #[must_use]
    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::InvalidResponse, message)
    }
}

/// 聊天角色。`Tool` 角色消息携带 `tool_call_id` 指向 assistant 的调用。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 模型发起的工具调用（assistant 消息内）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub name: String,
    /// 模型给出的参数（JSON 值；可能是对象或字符串，由执行端解析）。
    pub arguments: serde_json::Value,
}

/// 单条聊天消息（Provider 输入端）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    /// 文本内容；tool 结果消息也可放文本。
    pub content: Option<String>,
    /// assistant 消息可携带 0..n 个工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    /// tool 结果消息回填对应调用 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    #[must_use]
    pub fn assistant_tool_calls(calls: Vec<ChatToolCall>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: None,
            tool_calls: Some(calls),
            tool_call_id: None,
        }
    }
    #[must_use]
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// 暴露给模型的工具（OpenAI function schema 形状）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema（对象）。
    pub parameters: serde_json::Value,
}

/// 单轮请求。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ChatToolSpec>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// token 用量与耗时（Provider 尽可能回填；未知项为 0）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub duration_ms: u64,
}

impl ChatUsage {
    #[must_use]
    pub fn combine(self, other: ChatUsage) -> ChatUsage {
        ChatUsage {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            total_tokens: self.total_tokens + other.total_tokens,
            duration_ms: self.duration_ms + other.duration_ms,
        }
    }
}

/// 单轮响应：文本与/或工具调用。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ChatToolCall>,
    #[serde(default)]
    pub usage: ChatUsage,
}

/// 统一聊天模型端口。实现方必须自带超时（如 reqwest timeout）；
/// 不要无限挂起调用方（V4 §67）。
#[async_trait]
pub trait ChatModelProvider: Send + Sync {
    /// 提供方标识（如 "openai-compatible" / "fake"）。
    fn name(&self) -> &'static str;

    /// 发起一次 chat 调用（messages + tools）。
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_messages_serialize_as_openai_shape() {
        let messages = vec![
            ChatMessage::system("you are helpful"),
            ChatMessage::user("hello"),
            ChatMessage::assistant_tool_calls(vec![ChatToolCall {
                id: "call_1".into(),
                name: "history.search".into(),
                arguments: serde_json::json!({"query": "遵义会议"}),
            }]),
            ChatMessage::tool_result("call_1", r#"{"ok":true}"#),
        ];
        let json = serde_json::to_value(&messages).unwrap();
        // 仅校验形状结构，不锁定字段顺序。
        let arr = json.as_array().unwrap();
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[2]["role"], "assistant");
        assert_eq!(arr[2]["tool_calls"][0]["name"], "history.search");
        assert_eq!(arr[3]["role"], "tool");
        assert_eq!(arr[3]["tool_call_id"], "call_1");
        // round-trip
        let back: Vec<ChatMessage> = serde_json::from_value(json).unwrap();
        assert_eq!(messages, back);
    }
}
