//! Personal AI Hub — infrastructure 实现（V4）。
//!
//! `OpenAiCompatibleChatModelProvider`：OpenAI-Compatible `/chat/completions`
//! 实现（messages + tools + tool_calls + usage）。V5：travel 的旧
//! `OpenAiCompatibleLlmProvider` 已删除，本实现是唯一的模型 Provider。

pub mod conversation_store;
pub mod llm;

pub use conversation_store::ConversationSqliteStore;

pub use llm::{
    AiModelConfig, OpenAiCompatibleChatModelProvider, decode_tool_name, encode_tool_name,
    parse_chat_response,
};
