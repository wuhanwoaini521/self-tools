//! Personal AI Hub — infrastructure 实现（V4）。
//!
//! `OpenAiCompatibleChatModelProvider`：OpenAI-Compatible `/chat/completions`
//! 实现（messages + tools + tool_calls + usage）。与 travel 的
//! `OpenAiCompatibleLlmProvider` 同路线但契约独立（`core::personal_ai::ChatModelProvider`），
//! V5 候选统一；本轮互不干扰（V4 §117）。

pub mod llm;

pub use llm::{AiModelConfig, OpenAiCompatibleChatModelProvider, parse_chat_response};