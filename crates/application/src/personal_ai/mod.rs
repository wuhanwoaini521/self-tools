//! Personal AI Hub — application 层（V4）。
//!
//! 依赖方向：`devtoolbox-application::personal_ai` 只依赖 `devtoolbox-core::personal_ai`
//! 与 application 内既有 use case（如 `history::HistoryService`），零 infra 引用。
//! 组合根（apps/desktop）负责装配 Provider / Session / History 端口。

pub mod agent;
pub mod args;
pub mod context;
pub mod documents;
pub mod files;
pub mod geography;
pub mod history;
pub mod json_schema;
pub mod knowledge;
pub mod language;
pub mod memory;
pub mod prompt;
pub mod registry;
pub mod retrieval;
pub mod server;
pub mod session;
pub mod travel;

pub use agent::{AgentConfig, PersonalAgent, PersonalHub};
pub use context::{ContextBudget, ContextBundle, ModuleContextProvider, bundle_to_text};
pub use documents::{
    DocumentsProviderOwned, DocumentsTools, documents_tool_names, register_documents,
};
pub use files::{FilesProviderOwned, FilesTools, files_tool_names, register_files};
pub use geography::{
    GeographyProviderOwned, GeographyTools, geography_tool_names, register_geography,
};
pub use history::{HistoryProviderOwned, HistoryTools, history_tool_names, register_history};
pub use knowledge::{
    KnowledgeProviderOwned, KnowledgeTools, knowledge_tool_names, register_knowledge,
};
pub use language::{LanguageProviderOwned, LanguageTools, language_tool_names, register_language};
pub use memory::{MemoryProviderOwned, MemoryTools, memory_tool_names, register_memory};
pub use server::{ServerProviderOwned, ServerTools, register_server, server_tool_names};
pub use registry::{ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry, allowed_risk};
pub use retrieval::RetrievalAugmenter;
pub use session::{InMemorySessionStore, SessionStore};
pub use travel::{TravelContextProvider, TravelTools, register_travel, travel_tool_names};

/// agent 循环测试（Fake provider 五种场景，V4 §79）。
#[cfg(test)]
mod tests {}
