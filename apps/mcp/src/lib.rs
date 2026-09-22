//! MCP transport crate：STDIO + Streamable HTTP 薄协议 adapter（V8）。
//!
//! 依赖方向：本 crate 只做协议编解码与传输，**不含任何工具语义**。
//! 工具语义来自 `devtoolbox_application::mcp`（ToolRegistry 派生）。

pub mod compose;
pub mod http;
pub mod protocol;
pub mod stdio;

pub mod testing;

#[cfg(test)]
use tempfile as _;
