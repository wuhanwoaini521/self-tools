//! LLM Provider（需求 #十八）：OpenAI Compatible API 统一抽象。
//!
//! 支持 DeepSeek / Qwen / OpenAI / 本地 Ollama(`/v1` 即 OpenAI-Compatible)。
//! 不绑定具体模型；未配置 API key / base_url 时由上层跳过 LLM 相关步骤（需求 #十九）。
//! 响应解析为纯函数 `extract_chat_content`，可离线单测。

use async_trait::async_trait;
use serde::Serialize;

use crate::error::InfrastructureError;

/// LLM 配置（来自应用设置）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct LlmConfig {
    /// API base（如 `https://api.deepseek.com/v1`、`http://localhost:11434/v1`）。
    pub base_url: Option<String>,
    /// 可选 API key（本地 Ollama 不需要）。
    pub api_key: Option<String>,
    /// 模型名（如 `deepseek-chat`、`qwen-plus`、`gpt-4o-mini`、`qwen2.5:7b`）。
    pub model: Option<String>,
}

impl LlmConfig {
    /// 是否具备发起调用的条件（base + model 齐全）。
    #[must_use]
    pub fn is_configured(&self) -> bool {
        let base = self.base_url.as_deref().unwrap_or_default().trim();
        let model = self.model.as_deref().unwrap_or_default().trim();
        !base.is_empty() && !model.is_empty()
    }
}

/// LLM 调用接口（可 mock）。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 单轮对话，返回模型原始输出文本。
    async fn complete(&self, system: &str, user: &str) -> Result<String, InfrastructureError>;
}

/// OpenAI-Compatible `/chat/completions` 实现。
pub struct OpenAiCompatibleLlmProvider {
    client: reqwest::Client,
    config: LlmConfig,
}

impl OpenAiCompatibleLlmProvider {
    #[must_use]
    pub fn new(client: reqwest::Client, config: LlmConfig) -> Self {
        Self { client, config }
    }
}

const LLM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[async_trait]
impl LlmProvider for OpenAiCompatibleLlmProvider {
    async fn complete(&self, system: &str, user: &str) -> Result<String, InfrastructureError> {
        if !self.config.is_configured() {
            return Err(InfrastructureError::TravelLlm(
                "llm is not configured".to_string(),
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
        let request = ChatRequest {
            model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: user,
                },
            ],
            temperature: 0.2,
            response_format: None,
        };
        // 部分 OpenAI-Compatible 网关声明压缩响应但返回了不可解码的正文。
        // 明确请求未压缩 JSON，避免 reqwest 在长提示词响应阶段才解码失败。
        let mut builder = self
            .client
            .post(&url)
            .timeout(LLM_TIMEOUT)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .json(&request);
        if let Some(key) = self.config.api_key.as_deref().filter(|k| !k.is_empty()) {
            builder = builder.bearer_auth(key);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| InfrastructureError::TravelLlm(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(InfrastructureError::TravelLlm(format!(
                "llm returned {status}: {}",
                body.chars().take(200).collect::<String>()
            )));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| InfrastructureError::TravelLlm(error.to_string()))?;
        extract_chat_content(&body).map_err(InfrastructureError::TravelLlm)
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    #[allow(dead_code)]
    response_format: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// 从 `/chat/completions` 响应体提取首个 `choices[0].message.content`（纯函数）。
pub fn extract_chat_content(body: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| format!("invalid llm json: {error}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| "llm response missing choices[0].message.content".to_string())
}

#[cfg(test)]
mod tests {
    use super::{LlmConfig, extract_chat_content};

    #[test]
    fn llm_config_requires_base_and_model() {
        let mut config = LlmConfig::default();
        assert!(!config.is_configured());
        config.base_url = Some("http://localhost:11434/v1".to_string());
        assert!(!config.is_configured());
        config.model = Some("qwen2.5:7b".to_string());
        assert!(config.is_configured());
    }

    #[test]
    fn extract_standard_chat_content() {
        let body = r#"{"choices": [{"message": {"role": "assistant", "content": "{\"city\": \"杭州\"}"}}]}"#;
        let content = extract_chat_content(body.as_bytes()).expect("content");
        assert_eq!(content, "{\"city\": \"杭州\"}");
    }

    #[test]
    fn extract_fails_on_missing_content() {
        assert!(extract_chat_content(br#"{"choices": []}"#).is_err());
        assert!(extract_chat_content(b"not json").is_err());
    }

    #[test]
    fn extract_fails_on_empty_content() {
        let body = r#"{"choices": [{"message": {"content": "   "}}]}"#;
        assert!(extract_chat_content(body.as_bytes()).is_err());
    }
}
