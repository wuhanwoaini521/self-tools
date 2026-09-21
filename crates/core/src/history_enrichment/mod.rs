//! History V3.1 On-demand Enrichment — 纯数据契约（V5 Gate 2）。
//!
//! 与 `history_records` 同理：Enrichment 记录被 application（服务/校验）与
//! infrastructure（SQLite 行映射）共用，因此归 core 持有，避免 infra → application
//! 反向依赖。原则：Canonical = Truth Layer；富化是 derived 用户数据（独立 SQLite）。

pub mod model;

pub use model::{
    ENRICHMENT_SCHEMA_VERSION, EnrichmentClaim, EnrichmentKey, EnrichmentMetadata,
    EnrichmentPayload, EnrichmentRecord, EnrichmentSection, EnrichmentState, EnrichmentView,
};
