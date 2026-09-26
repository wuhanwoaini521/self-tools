//! RSS 端口（Gate 7.6：依赖方向反转 + 异步边界）。
//!
//! **Bounded context：个人订阅阅读器**（ADR-010）。本端口只服务 RSS
//! 领域用例（增删订阅 / 刷新 / 阅读状态 / 收藏 / 检索），**不含任何
//! News 概念** —— 新闻归 `application::news` 的 `NewsRepositoryPort`。
//!
//! 应用层用例只依赖以下能力端口，不直接接触 `reqwest` / `FeedRepository`：
//! - `RssRepositoryPort`：订阅/文章的持久化（SQLite 由 runtime 适配器实现）；
//! - [`crate::feed::FeedFetcherPort`]：**共享**抓取端口（HTTP 由 runtime 适配器实现）。
//!
//! 抓取端口是共享基础设施，不在这里声明（ADR-010：RSS 与 News 共享 fetcher，
//! 不共享 repository）。

use devtoolbox_core::feed::FetchedEntry;
use devtoolbox_core::rss::{ArticleRow, FeedRow};

/// RSS 持久化端口（对应 `FeedRepository` 的真实使用面，非完整 API 复制）。
pub trait RssRepositoryPort: Send + Sync {
    fn list_feeds(&self) -> Result<Vec<FeedRow>, String>;
    fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, String>;
    fn insert_feed(&self, title: &str, url: &str, site_url: Option<&str>) -> Result<i64, String>;
    fn insert_articles(&self, feed_id: i64, entries: &[FetchedEntry]) -> Result<usize, String>;
    fn set_feed_success(&self, feed_id: i64) -> Result<(), String>;
    fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), String>;
    fn feed_title(&self, feed_id: i64) -> Result<Option<String>, String>;
    fn list_articles(&self, feed_id: i64, limit: i64) -> Result<Vec<ArticleRow>, String>;
    fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String>;
    /// 按 id 取单条（`rss.get_entry` / 详情面板）。
    fn entry_by_id(&self, entry_id: i64) -> Result<Option<ArticleRow>, String>;
    fn mark_article_read(&self, article_id: i64) -> Result<(), String>;
    fn delete_feed(&self, feed_id: i64) -> Result<(), String>;

    // ---- RSS 阅读面 ----

    /// 关键词检索：`None` = 不限关键词；标题 / 署名 / 摘要 LIKE 粗筛。
    fn query_articles(
        &self,
        keyword: Option<&str>,
        feed_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ArticleRow>, String>;

    /// 收藏文章（稍后读）倒序列表。
    fn starred_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String>;

    /// 切换收藏；返回切换后状态（true = 已收藏）。
    fn toggle_article_star(&self, article_id: i64) -> Result<bool, String>;
}
