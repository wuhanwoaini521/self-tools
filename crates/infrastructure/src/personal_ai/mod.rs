//! Personal AI Hub — infrastructure 实现（V4）。
//!
//! `OpenAiCompatibleChatModelProvider`：OpenAI-Compatible `/chat/completions`
//! 实现（messages + tools + tool_calls + usage）。V5：travel 的旧
//! `OpenAiCompatibleLlmProvider` 已删除，本实现是唯一的模型 Provider。

pub mod llm;

pub use llm::{AiModelConfig, OpenAiCompatibleChatModelProvider, parse_chat_response};
