//! Language Learning Hub 的用例编排（DTO 直接给 React）。
//!
//! Gate 7.5：`LanguageService` 只依赖 `LanguageStorePort`（本模块 `ports.rs`），
//! 不再直接 import 基础设施的 `LanguageStore` / `now_unix` / 导入工具。
//! Starter 与原始文件导入工作流已迁入 infrastructure 的 `language` 模块
//! （`starter` / `importing`），应用层不再持有导入功能。

pub mod ports;
pub mod service;

pub use ports::{
    LanguageCount, LanguageDetailRows, LanguageExample, LanguageStorePort, SearchHitModel,
};
pub use service::{
    LanguageInfo, LanguageSearchHit, LanguageService, ProgressView, ReviewCard, SourceInfo,
    TodayView, WordDetail,
};

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;
