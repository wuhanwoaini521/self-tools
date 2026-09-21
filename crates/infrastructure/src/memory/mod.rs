//! Personal Memory 持久化（V6 Track A）。
//!
//! `infrastructure` 只做实现：SQLite 读写，不持有端口类型、不含业务规则。
//! 端口（`MemoryStorePort`）与用例在 application，装配在 `apps/desktop`。

pub mod store;

pub use store::{MEMORY_SCHEMA_VERSION, MemorySqliteStore};
