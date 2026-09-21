//! Personal AI Hub — 纯领域契约（V4 Foundation）。
//!
//! 本模块是 `self-tools` V4「Personal Digital Hub」的**共享协议层**：只包含
//! 数据契约（AgentRequest / AgentResponse / Tool / ModuleDescriptor / AppContext /
//! Action / UiBlock）、错误模型与 `ChatModelProvider` 端口。不包含任何业务逻辑、
//! HTTP 或持久化实现 —— 与 `core::travel::provider` 同级别的端口所在层。
//!
//! 依赖方向：`core :: personal_ai` 无内部依赖；
//! application 在此之上实现 PersonalAgent / 注册表 / Context Provider；
//! infrastructure 实现 `ChatModelProvider`；组合根（apps/desktop）完成装配。

pub mod error;
pub mod provider;
pub mod types;

pub use error::{AgentError, AgentErrorKind};
pub use provider::{
    ChatMessage, ChatModelProvider, ChatRequest, ChatResponse, ChatRole, ChatToolCall,
    ChatToolSpec, ChatUsage, ProviderError, ProviderErrorKind,
};
pub use types::{
    Action, ActionKind, AgentMessage, AgentRequest, AgentResponse, AgentUsage, AppContext,
    EntityRef, ModuleDescriptor, SelectionRef, ToolCallRequest, ToolResult, ToolRisk, ToolSpec,
    ToolTraceEntry, UiBlock, UiBlockKind,
};
