//! News 端口（ADR-010：**News 是独立 bounded context**）。
//!
//! 三件事：
//! 1. [`NewsRepositoryPort`] —— News 自己的持久化端口（`config/news.db`），
//!    **与 `RssRepositoryPort` 无任何关系**；
//! 2. [`NewsPort`] —— 同步读写面（News 页命令 + Personal AI `news.*` 工具共用）；
//! 3. [`NewsIngestPort`] —— 联网摄取（抓取骨架共享 `crate::feed::fetch_many`，
//!    落库语义是 News 自己的）。
//!
//! 类型契约在 `devtoolbox_core::news`；这里只做 re-export 便于组合根引用。

use devtoolbox_core::feed::FetchedEntry;
use devtoolbox_core::news::{NewsArticle, NewsCategory, NewsSource};

pub use devtoolbox_core::news::{
    NewsArticle as Article, NewsCategory as Category, NewsSource as Source, NewsSourceType,
    RecommendedSource, recommended_sources,
};

/// News 源健康态总览（未配置任何源时的降级依据）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewsSourceHealth {
    /// 至少一个新闻源最近一次抓取失败。
    Degraded,
    /// 全部新闻源正常（或无新闻源）。
    Healthy,
}

/// News 用例错误。
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NewsError {
    /// 新闻源不存在。
    #[error("news source not found: {0}")]
    SourceNotFound(i64),
    /// 新闻文章不存在。
    #[error("news article not found: {0}")]
    ArticleNotFound(i64),
    /// 存储失败（消息来自适配层，已是可读文本）。
    #[error("news store failed: {0}")]
    Store(String),
    /// 抓取失败（网络 / 解析）。
    #[error("news fetch failed: {0}")]
    Fetch(String),
}

impl NewsError {
    /// 稳定 reason 码（前端 / 工具层分支用，不做字符串匹配）。
    #[must_use]
    pub fn kind(&self) -> NewsErrorKind {
        match self {
            Self::SourceNotFound(_) => NewsErrorKind::SourceNotFound,
            Self::ArticleNotFound(_) => NewsErrorKind::ArticleNotFound,
            Self::Store(_) => NewsErrorKind::Store,
            Self::Fetch(_) => NewsErrorKind::Fetch,
        }
    }
}

/// News 错误分类（稳定码）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewsErrorKind {
    SourceNotFound,
    ArticleNotFound,
    Store,
    Fetch,
}

/// 新闻源列表 + 健康态（`news_sources` 命令与 ContextProvider 共用形状）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsSourcesView {
    pub sources: Vec<NewsSource>,
    pub health: NewsSourceHealth,
}

/// 一次刷新的结果（**News 自己的报告**，不复用 `rss::workflows::RefreshReport`）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NewsRefreshReport {
    /// 实际新增的新闻条数。
    pub new_articles: usize,
    /// 逐源失败（单源失败隔离，不影响其它源）。
    pub failures: Vec<NewsRefreshFailure>,
}

/// 单个新闻源的抓取失败。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsRefreshFailure {
    pub source: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// NewsRepositoryPort：News 自己的持久化端口
// ---------------------------------------------------------------------------

/// News 持久化端口（对应 `NewsRepository` 的真实使用面，非完整 API 复制）。
///
/// 实现在 infrastructure（`NewsRepository` → `config/news.db`）；
/// **不与 `RssRepositoryPort` 共享任何方法或类型**（ADR-010）。
pub trait NewsRepositoryPort: Send + Sync {
    fn list_sources(&self) -> Result<Vec<NewsSource>, String>;
    fn source_by_id(&self, source_id: i64) -> Result<Option<NewsSource>, String>;
    fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, String>;
    fn insert_source(
        &self,
        name: &str,
        url: &str,
        category: NewsCategory,
        site_url: Option<&str>,
    ) -> Result<i64, String>;
    fn set_source_category(&self, source_id: i64, category: NewsCategory) -> Result<(), String>;
    fn set_source_health(&self, source_id: i64, error: Option<&str>) -> Result<(), String>;
    fn delete_source(&self, source_id: i64) -> Result<(), String>;
    fn has_failed_source(&self) -> Result<bool, String>;

    fn insert_articles(&self, source_id: i64, entries: &[FetchedEntry]) -> Result<usize, String>;
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, String>;
    fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String>;
    fn latest_by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, String>;
    fn article_by_id(&self, article_id: i64) -> Result<Option<NewsArticle>, String>;
    fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String>;
    fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, String>;
    fn toggle_star(&self, article_id: i64) -> Result<bool, String>;
    fn mark_read(&self, article_id: i64) -> Result<(), String>;
    fn unread_total(&self) -> Result<i64, String>;
}

// ---------------------------------------------------------------------------
// NewsPort：同步读写面（News 页命令 + Personal AI news.* 工具）
// ---------------------------------------------------------------------------

/// News 能力端口：读 + 本地用户状态往返。**全部同步**（存储短锁语义）。
///
/// 与 `NewsIngestPort` 分离 —— 联网摄取是另一条 async 边界。
pub trait NewsPort: Send + Sync {
    fn sources(&self) -> Result<NewsSourcesView, NewsError>;
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError>;
    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<NewsArticle>, NewsError>;
    fn by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, NewsError>;
    fn by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, NewsError>;
    fn get_article(&self, article_id: i64) -> Result<NewsArticle, NewsError>;
    fn starred(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError>;
    fn toggle_star(&self, article_id: i64) -> Result<bool, NewsError>;
    fn mark_read(&self, article_id: i64) -> Result<(), NewsError>;
    fn remove_source(&self, source_id: i64) -> Result<(), NewsError>;
    fn set_category(&self, source_id: i64, category: NewsCategory) -> Result<(), NewsError>;
}

// ---------------------------------------------------------------------------
// NewsIngestPort：联网摄取（抓取共享，落库是 News 的）
// ---------------------------------------------------------------------------

/// News 摄取端口（async）。
///
/// `Send`：底层 `FeedFetcherPort::fetch_feed` 显式 `+ Send`，
/// 因此可被 Tauri 命令与 `ToolExecutor::execute` 消费。
#[async_trait::async_trait]
pub trait NewsIngestPort: Send + Sync {
    /// 立即刷新全部新闻源（并发抓取 + 逐源落库 + 单源失败隔离）。
    async fn refresh(&self) -> Result<NewsRefreshReport, NewsError>;

    /// 添加新闻源：抓取取标题 → 落 `news_sources`（URL 冲突报错）。
    ///
    /// 这是 **News 侧的添加**，与 `rss::commit_new_feed` 各走各的存储。
    async fn add_source(&self, url: &str, category: NewsCategory) -> Result<NewsSource, NewsError>;
}
