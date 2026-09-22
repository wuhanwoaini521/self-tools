//! 备份/恢复域（V11 §64-§70、§160）。
//!
//! 依赖方向：契约（`BackupManifest` / `BackupEntry` / `BackupSource` /
//! `RestoreReport`）在 `devtoolbox_core::backup`，实现（引擎内安全快照）在
//! `devtoolbox_infrastructure::backup`，本域只做编排，不碰具体 SQL。
//!
//! **恢复演练**见本 crate 的 `tests.rs`；涉及真实 SQLite / DuckDB 的活动库
//! 快照与恢复（fixture 建库 → 备份 → 破坏 → 恢复到第二个隔离目录）放在
//! `devtoolbox_infrastructure::backup::drill`（那里有 rusqlite / duckdb）。

pub mod service;

pub use service::{BackupService, JsonSource};

#[cfg(test)]
mod tests;
