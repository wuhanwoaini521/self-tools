//! Personal Memory 域（V6 Track A）。
//!
//! 依赖方向：只依赖 `devtoolbox-core`（模型 + gate）与本 crate 的 `error`/`time`；
//! 持久化经 `MemoryStorePort`，实现在 infrastructure，组合根在 `apps/desktop`。

pub mod ports;
pub mod service;

pub use ports::{MemoryStoreError, MemoryStorePort};
pub use service::{MemoryConfig, MemoryService, MemoryStats, relevance};

#[cfg(test)]
mod tests;
