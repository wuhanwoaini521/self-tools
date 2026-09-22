//! MCP 应用层（V8 Track A/C/D）。
//!
//! 职责边界（Principle 1/2）：
//! - **不实现任何工具语义**——全部委派 `personal_ai::registry::ToolRegistry`；
//! - MCP tool definition 从 `ToolSpec` 派生（§12，无第二份手写 schema）；
//! - discovery 与 execution 都过授权（§18）；
//! - SYSTEM 工具不直接执行：进 `SafeActionService`（§54/§55）。
//!
//! 身份/审计经端口注入（`RemoteIdentityProvider` / `McpAuditPort`），
//! infrastructure 只提供 Fake（CI 用，§106）。

pub mod adapter;
pub mod auth;
pub mod policy;
pub mod service;

pub use adapter::{McpCallOutcome, McpToolAdapter, McpToolDefinition};
pub use auth::{AuthProviderUnavailable, RemoteIdentityProvider, StaticTokenIdentityProvider};
pub use policy::{AuthorizationDecision, McpAuthorizationPolicy};
pub use service::{McpService, McpServiceConfig};
