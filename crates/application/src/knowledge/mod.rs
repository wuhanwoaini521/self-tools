//! Knowledge 检索域（V6 Track D，§52-§69）。
//!
//! 依赖方向：只依赖 `devtoolbox-core` 与同 crate 的三个知识域（memory /
//! documents / files）；不引入任何新的存储或模型抽象（§49/§136）。

pub mod context;
pub mod observability;
pub mod ports;
pub mod retrievers;
pub mod service;

pub use context::{KnowledgeContext, OmittedSummary};
pub use observability::{KnowledgeMetrics, KnowledgeMetricsSnapshot};
pub use ports::{KnowledgeSourceRetriever, RetrievalMode};
pub use retrievers::{DocumentRetriever, FileRetriever, MemoryRetriever};
pub use service::KnowledgeRetrievalService;

/// 预算默认值（P0：与 §64 一致；配置化留给 P1）。
#[must_use]
pub fn default_budget() -> devtoolbox_core::knowledge::KnowledgeBudget {
    devtoolbox_core::knowledge::KnowledgeBudget::default()
}

#[cfg(test)]
mod tests;
