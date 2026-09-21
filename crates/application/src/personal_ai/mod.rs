//! Personal AI Hub — application 层（V4）。
//!
//! 依赖方向：`devtoolbox-application::personal_ai` 只依赖 `devtoolbox-core::personal_ai`
//! 与 application 内既有 use case（如 `history::HistoryService`），零 infra 引用。
//! 组合根（apps/desktop）负责装配 Provider / Session / History 端口。

pub mod agent;
pub mod context;
pub mod history;
pub mod json_schema;
pub mod prompt;
pub mod registry;
pub mod session;
pub mod geography;
pub mod language;
pub mod travel;

pub use agent::{AgentConfig, PersonalAgent, PersonalHub};
pub use context::{ContextBudget, ContextBundle, ModuleContextProvider, bundle_to_text};
pub use geography::{
    GeographyProviderOwned, GeographyTools, geography_tool_names, register_geography,
};
pub use history::{HistoryProviderOwned, HistoryTools, history_tool_names, register_history};
pub use language::{
    LanguageProviderOwned, LanguageTools, language_tool_names, register_language,
};
pub use registry::{ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry, allowed_risk};
pub use session::{InMemorySessionStore, SessionStore};
pub use travel::{TravelContextProvider, TravelTools, register_travel, travel_tool_names};

/// agent 循环测试（Fake provider 五种场景，V4 §79）。
#[cfg(test)]
mod tests {}
