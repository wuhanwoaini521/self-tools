//! News 领域用例层（V12 / ADR-010：**News 是独立 bounded context**）。
//!
//! ```text
//! NewsService（同步读写）── NewsRepositoryPort ──→ NewsRepository(config/news.db)
//!        │
//! NewsIngestService（联网）─ crate::feed::fetch_many（共享抓取骨架）
//! ```
//!
//! - **不依赖 `crate::rss`**：不读 `RssRepositoryPort`、不复用
//!   `rss::workflows` 的 DTO/报告；
//! - **共享基础设施**：抓取骨架 `crate::feed::fetch_many` +
//!   `core::feed::{FetchedFeed, FetchedEntry}` 归一契约 + `feed_fetcher`；
//! - 消费面：桌面 `news_*` 命令、Personal AI `news.*` 工具、MCP。

pub mod ports;
pub mod service;

pub use devtoolbox_core::news::{
    NewsArticle, NewsCategory, NewsSource, NewsSourceType, RecommendedSource, recommended_sources,
};
pub use ports::{
    NewsError, NewsErrorKind, NewsIngestPort, NewsPort, NewsRefreshFailure, NewsRefreshReport,
    NewsRepositoryPort, NewsSourceHealth, NewsSourcesView,
};
pub use service::{NewsIngestService, NewsService};

#[cfg(test)]
mod tests;
