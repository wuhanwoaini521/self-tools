//! RSS 读写服务（ADR-010：个人订阅阅读器的 AI / UI 读写面）。
//!
//! - [`RssService`] —— 同步读写（RSS 页命令 + Personal AI `rss.*` 工具），
//!   只依赖 [`RssRepositoryPort`]；
//! - [`RssIngestService`] —— 联网刷新：抓取骨架走共享
//!   [`crate::feed::fetch_many`]，落库走 `rss::workflows::commit_refresh`。
//!
//! 与 `news::service` 同构，但**共享的只有抓取骨架**（ADR-010）。

use std::sync::Arc;

use devtoolbox_core::rss::{ArticleRow, FeedRow};

use crate::feed::FeedFetcherPort;
use crate::rss::ports::RssRepositoryPort;
use crate::rss::workflows::{RefreshReport, commit_refresh, feed_snapshots};

/// RSS 用例错误（AI / UI 读写面的稳定失败分类）。
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RssError {
    /// 订阅源不存在。
    #[error("rss subscription not found: {0}")]
    FeedNotFound(i64),
    /// 条目不存在。
    #[error("rss entry not found: {0}")]
    EntryNotFound(i64),
    /// 存储失败（消息来自适配层，已是可读文本）。
    #[error("rss store failed: {0}")]
    Store(String),
    /// 抓取失败（网络 / 解析）。
    #[error("rss fetch failed: {0}")]
    Fetch(String),
}

fn store_failure(message: String) -> RssError {
    RssError::Store(message)
}

fn clamp(limit: i64) -> i64 {
    if limit <= 0 {
        return 1;
    }
    limit.clamp(1, 200)
}

/// RSS 能力端口（News 同款形状：同步读写，摄取另开 async 边界）。
///
/// 实现给 Personal AI 的 `rss.*` 工具与 RSS 页命令共用。
pub trait RssPort: Send + Sync {
    /// 订阅源清单（含未读数）。
    fn list_subscriptions(&self) -> Result<Vec<FeedRow>, RssError>;
    /// 条目列表：`feed_id = None` 跨全部订阅最新；否则该订阅内最新。
    fn list_entries(&self, feed_id: Option<i64>, limit: i64) -> Result<Vec<ArticleRow>, RssError>;
    /// 关键词检索（标题 / 署名 / 摘要）。
    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<ArticleRow>, RssError>;
    /// 单条条目详情。
    fn get_entry(&self, entry_id: i64) -> Result<ArticleRow, RssError>;
    /// 标已读（幂等）。
    fn mark_read(&self, entry_id: i64) -> Result<(), RssError>;
}

/// RSS 摄取端口（async；联网刷新全部订阅）。
#[async_trait::async_trait]
pub trait RssIngestPort: Send + Sync {
    async fn refresh(&self) -> Result<RefreshReport, RssError>;
}

// ---------------------------------------------------------------------------
// RssService
// ---------------------------------------------------------------------------

/// RSS 读写服务（RSS 页 + `rss.*` 工具共用）。
pub struct RssService {
    repository: Arc<dyn RssRepositoryPort>,
}

impl RssService {
    #[must_use]
    pub fn new(repository: Arc<dyn RssRepositoryPort>) -> Self {
        Self { repository }
    }

    fn ensure_feed(&self, feed_id: i64) -> Result<(), RssError> {
        self.repository
            .feed_title(feed_id)
            .map_err(store_failure)?
            .map(|_| ())
            .ok_or(RssError::FeedNotFound(feed_id))
    }
}

impl RssPort for RssService {
    fn list_subscriptions(&self) -> Result<Vec<FeedRow>, RssError> {
        self.repository.list_feeds().map_err(store_failure)
    }

    fn list_entries(&self, feed_id: Option<i64>, limit: i64) -> Result<Vec<ArticleRow>, RssError> {
        if let Some(feed_id) = feed_id {
            self.ensure_feed(feed_id)?;
            return self
                .repository
                .list_articles(feed_id, clamp(limit))
                .map_err(store_failure);
        }
        self.repository
            .latest_articles(clamp(limit))
            .map_err(store_failure)
    }

    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<ArticleRow>, RssError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(RssError::Store("rss search keyword is empty".into()));
        }
        self.repository
            .query_articles(Some(keyword), None, clamp(limit))
            .map_err(store_failure)
    }

    fn get_entry(&self, entry_id: i64) -> Result<ArticleRow, RssError> {
        // 无「按 id 取」端口方法：先由 list_articles 之外的路径查。
        // 这里用 search 不合适 —— 直接经由 repository 的按 id 查询能力。
        self.repository
            .entry_by_id(entry_id)
            .map_err(store_failure)?
            .ok_or(RssError::EntryNotFound(entry_id))
    }

    fn mark_read(&self, entry_id: i64) -> Result<(), RssError> {
        self.repository
            .mark_article_read(entry_id)
            .map_err(store_failure)
    }
}

// ---------------------------------------------------------------------------
// RssIngestService
// ---------------------------------------------------------------------------

/// RSS 摄取服务：`RssService` + 抓取能力。
///
/// 刷新走 rss 自己的两段式编排（`feed_snapshots` → `fetch_many` → `commit_refresh`），
/// 落库语义与既有 Tauri 命令 `refresh_rss_feeds` 完全一致。
pub struct RssIngestService<F: FeedFetcherPort> {
    service: Arc<RssService>,
    fetcher: F,
}

impl<F: FeedFetcherPort> RssIngestService<F> {
    #[must_use]
    pub fn new(service: Arc<RssService>, fetcher: F) -> Self {
        Self { service, fetcher }
    }
}

#[async_trait::async_trait]
impl<F: FeedFetcherPort> RssIngestPort for RssIngestService<F> {
    async fn refresh(&self) -> Result<RefreshReport, RssError> {
        let snapshots = feed_snapshots(self.service.repository.as_ref())
            .map_err(|error| RssError::Store(error.to_string()))?;
        // 抓取阶段：共享并发骨架（ADR-010）。
        let urls: Vec<String> = snapshots.iter().map(|feed| feed.url.clone()).collect();
        let results = crate::feed::fetch_many(&urls, &self.fetcher).await;
        let pairs = snapshots
            .into_iter()
            .zip(results.into_iter().map(|(_, result)| result))
            .collect();
        // 落库阶段：RSS 自己的 commit（逐源写 feeds/articles + last_error）。
        commit_refresh(self.service.repository.as_ref(), pairs)
            .map_err(|error| RssError::Store(error.to_string()))
    }
}
