//! MCP 核心契约（V8 Track A，§11-§42）。
//!
//! **MCP 只是 transport / protocol adapter**（V8 §0）：本模块只表达
//! 「谁在调用（principal）」「能看到什么（exposure）」「调用是否被允许
//! （authorization）」三类纯契约，**不含任何工具语义**。
//! 工具语义的唯一来源是 `application::personal_ai::registry::ToolRegistry`
//! （Principle 2）。
//!
//! 三条不变量：
//! 1. `McpPrincipal` 与 Personal Memory 的用户身份**无关**（§27）——它只表示
//!    「哪个 MCP client 在调用」；
//! 2. scope 不等于确认（§38）：拥有 `server.action` 仍要走 SafeAction 确认；
//! 3. 认证信息永不进入本模块的任何 `Display` / 序列化输出（§79：审计不含 token）。

pub mod audit;
pub mod exposure;
pub mod principal;

pub use audit::{McpAuditEntry, McpDecision, McpResultCode};
pub use exposure::{ExposureGroup, ToolExposure, default_exposure, required_scope_for};
pub use principal::{
    AuthFailure, McpCredential, McpPrincipal, McpScope, McpTransport, McpTrustLevel,
};
