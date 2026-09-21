//! History Enrichment 基础设施：SQLite 派生富化缓存（V5 §17/§18）。
//!
//! 数据是 **derived 用户数据**（gitignored config/history_enrichment.db），
//! 与只读 Canonical（history.duckdb）完全分离。

pub mod store;

pub use store::EnrichmentSqliteStore;