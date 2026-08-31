//! Language Learning Hub 的用例编排（DTO 直接给 React）。
//!
//! `LanguageService` 持 `Arc<Mutex<LanguageStore>>`（与 History 服务同款模式），
//! 所有方法同步；导入工作流在 `starter`（内置数据包）与 `importing`（CLI 读原始文件）。

pub mod importing;
pub mod service;
pub mod starter;

pub use service::{
    LanguageInfo, LanguageSearchHit, LanguageService, ManifestInfo, ProgressView, ReviewCard,
    SourceInfo, TodayView, WordDetail,
};
pub use starter::{DatasetReport, StarterReport, install_starter};

#[cfg(test)]
mod tests;
