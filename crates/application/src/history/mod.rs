//! History 知识库（V2）的用例编排。
//!
//! `HistoryService` 不接触 DuckDB / Tauri：它只通过 `HistoryQueryPort`
//! 读取只读查询面，并负责搜索聚合、分组、截断、来源 ID 合并、period 解析等用例决策。
//! Port 的实际实现由平台适配层（Desktop / 未来 HTTP）绑定到 `HistoryDuckDbRepository`。

pub mod ports;
pub mod service;

pub use ports::{HistoryPortError, HistoryQueryPort};
pub use service::{
    HistorySemanticEventDetail, HistorySemanticHome, HistorySemanticPeriodDetail,
    HistorySemanticPersonDetail, HistorySemanticSearchGroup, HistorySemanticSearchHit,
    HistorySemanticStoryDetail, HistorySemanticWorkDetail, HistoryService,
};

#[cfg(test)]
mod tests;