//! History V3.1 On-demand AI Enrichment（V5 Track B）。
//!
//! 域模型（EnrichmentKey/Section/State/Payload/Record/View）位于
//! `devtoolbox_core::history_enrichment`（app 与 infra 共用纯数据载体）。
//! 本模块提供：端口（search/llm/entity/store）、来源排序、结构化生成解析、
//! 校验门、与 `HistoryEnrichmentService` 协调器（单飞 / stale-while-revalidate）。

pub mod generation;
pub mod ports;
pub mod ranking;
pub mod service;
pub mod validation;

pub use ports::{
    CanonicalEventRef, EnrichmentEntityPort, EnrichmentLlmPort, EnrichmentRunnerPort,
    EnrichmentSearchPort, EnrichmentStore, HistoryEntityPort, SourceEvidence, SourceType,
    entity_port_from_history,
};
pub use ranking::{
    authority_weight, classify_source, normalize_domain, rank_sources, sources_to_prompt_block,
};
pub use service::{EnrichmentConfig, HistoryEnrichmentService};

#[cfg(test)]
mod tests;
