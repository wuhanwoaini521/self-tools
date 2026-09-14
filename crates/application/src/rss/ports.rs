//! RSS 端口（Gate 7.6：依赖方向反转 + 异步边界）。
//!
//! 应用层用例只依赖两个能力端口，不再直接接触 `reqwest` / `FeedRepository`：
//! - `RssRepositoryPort`：订阅/文章的持久化（SQLite 由 runtime 适配器实现）；
//! - `FeedFetcherPort`：远端 Feed 抓取+解析（HTTP 由 runtime 适配器实现）。
//!
//! 两个 capability 分开定义——repository 是同步存储，fetcher 是异步传输，
//! 各自的失败语义不同，因此不合成一个巨型对象。

use devtoolbox_core::rss::{ArticleRow, FeedRow, FetchedEntry, FetchedFeed};

/// 抓取失败分类：传输失败（网络/DNS/超时）与解析失败（非 Feed 内容）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedFetchErrorKind {
    Fetch,
    Parse,
}

/// 抓取端口错误：应用层可见的传输失败描述。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedFetchError {
    pub kind: FeedFetchErrorKind,
    pub message: String,
}

impl std::fmt::Display for FeedFetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// RSS 持久化端口（对应 `FeedRepository` 的真实使用面，非完整 API 复制）。
pub trait RssRepositoryPort: Send + Sync {
    fn list_feeds(&self) -> Result<Vec<FeedRow>, String>;
    fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, String>;
    fn insert_feed(
        &self,
        title: &str,
        url: &str,
        site_url: Option<&str>,
    ) -> Result<i64, String>;
    fn insert_articles(&self, feed_id: i64, entries: &[FetchedEntry]) -> Result<usize, String>;
    fn set_feed_success(&self, feed_id: i64) -> Result<(), String>;
    fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), String>;
    fn feed_title(&self, feed_id: i64) -> Result<Option<String>, String>;
    fn list_articles(&self, feed_id: i64, limit: i64) -> Result<Vec<ArticleRow>, String>;
    fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String>;
    fn mark_article_read(&self, article_id: i64) -> Result<(), String>;
    fn delete_feed(&self, feed_id: i64) -> Result<(), String>;
}

/// 抓取端口（异步；具体传输在 runtime 适配器）。
#[allow(async_fn_in_trait)] // 调用方只通过具体类型使用本端口（非 dyn），无需自动 trait 边界
pub trait FeedFetcherPort: Send + Sync {
    async fn fetch_feed(&self, url: &str) -> Result<FetchedFeed, FeedFetchError>;
}