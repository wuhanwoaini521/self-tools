//! News 用例服务（ADR-010：独立 bounded context）。
//!
//! 两个服务：
//! - [`NewsService`] —— 同步读写（News 页命令 + Personal AI `news.*` 工具），
//!   只依赖 [`NewsRepositoryPort`]，**零 `crate::rss` 引用**；
//! - [`NewsIngestService`] —— 联网摄取：抓取骨架走共享
//!   [`crate::feed::fetch_many`]，落库语义写 `news.db`。
//!
//! `recommended_sources()` 目录在 `devtoolbox_core::news`（seed 与推荐区
//! 的唯一来源）。

use std::sync::Arc;

use devtoolbox_core::feed::FetchedFeed;
use devtoolbox_core::news::{NewsArticle, NewsCategory, NewsSource};

use crate::feed::FeedFetcherPort;
use crate::news::ports::{
    NewsError, NewsIngestPort, NewsPort, NewsRefreshFailure, NewsRefreshReport, NewsRepositoryPort,
    NewsSourceHealth, NewsSourcesView,
};

/// 关键词 / 分类查询的 limit 上限（防模型或前端传 1e9）。
const MAX_LIMIT: i64 = 200;

fn clamp(limit: i64) -> i64 {
    if limit <= 0 {
        return 1;
    }
    limit.clamp(1, MAX_LIMIT)
}

fn store_failure(message: String) -> NewsError {
    NewsError::Store(message)
}

// ---------------------------------------------------------------------------
// NewsService：同步读写
// ---------------------------------------------------------------------------

/// News 读写服务（News 页 + `news.*` 工具共用）。
pub struct NewsService {
    repository: Arc<dyn NewsRepositoryPort>,
}

impl NewsService {
    #[must_use]
    pub fn new(repository: Arc<dyn NewsRepositoryPort>) -> Self {
        Self { repository }
    }

    fn ensure_source(&self, source_id: i64) -> Result<(), NewsError> {
        self.repository
            .source_by_id(source_id)
            .map_err(store_failure)?
            .map(|_| ())
            .ok_or(NewsError::SourceNotFound(source_id))
    }
}

impl NewsPort for NewsService {
    fn sources(&self) -> Result<NewsSourcesView, NewsError> {
        let sources = self.repository.list_sources().map_err(store_failure)?;
        let health = if sources.iter().any(|source| source.last_error.is_some()) {
            NewsSourceHealth::Degraded
        } else {
            NewsSourceHealth::Healthy
        };
        Ok(NewsSourcesView { sources, health })
    }

    /// 今日最新（跨源，发布时间倒序）。
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        self.repository.latest(clamp(limit)).map_err(store_failure)
    }

    /// 关键词检索（空关键词是参数错误，不是空结果）。
    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(NewsError::Store("news search keyword is empty".into()));
        }
        self.repository
            .query_articles(Some(keyword), None, clamp(limit))
            .map_err(store_failure)
    }

    /// 按分类取最新（`news.by_category`）。
    fn by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, NewsError> {
        self.repository
            .latest_by_category(category, clamp(limit))
            .map_err(store_failure)
    }

    /// 按源取最新（`news.by_source`；未知源受控失败）。
    fn by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        self.ensure_source(source_id)?;
        self.repository
            .latest_by_source(source_id, clamp(limit))
            .map_err(store_failure)
    }

    /// 单条新闻（`news.get_article`）。
    fn get_article(&self, article_id: i64) -> Result<NewsArticle, NewsError> {
        self.repository
            .article_by_id(article_id)
            .map_err(store_failure)?
            .ok_or(NewsError::ArticleNotFound(article_id))
    }

    /// 稍后读列表。
    fn starred(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        self.repository
            .starred_articles(clamp(limit))
            .map_err(store_failure)
    }

    fn toggle_star(&self, article_id: i64) -> Result<bool, NewsError> {
        self.repository
            .toggle_star(article_id)
            .map_err(store_failure)
    }

    fn mark_read(&self, article_id: i64) -> Result<(), NewsError> {
        self.repository.mark_read(article_id).map_err(store_failure)
    }

    fn remove_source(&self, source_id: i64) -> Result<(), NewsError> {
        self.ensure_source(source_id)?;
        self.repository
            .delete_source(source_id)
            .map_err(store_failure)
    }

    fn set_category(&self, source_id: i64, category: NewsCategory) -> Result<(), NewsError> {
        self.ensure_source(source_id)?;
        self.repository
            .set_source_category(source_id, category)
            .map_err(store_failure)
    }
}

// ---------------------------------------------------------------------------
// NewsIngestService：联网摄取
// ---------------------------------------------------------------------------

/// News 摄取服务：`NewsService` + 抓取能力（抓取共享，落库是 News 的）。
///
/// `F` 为具体抓取实现（`FeedFetcherPort` 是 RPITIT，非 dyn-compatible）。
pub struct NewsIngestService<F: FeedFetcherPort> {
    service: Arc<NewsService>,
    fetcher: F,
}

impl<F: FeedFetcherPort> NewsIngestService<F> {
    #[must_use]
    pub fn new(service: Arc<NewsService>, fetcher: F) -> Self {
        Self { service, fetcher }
    }

    /// 按 URL 归一化判断（`validate` 交给调用方；这里只做 scheme 检查，
    /// 与 `rss::validate_feed_url` 同一规则但属于 News 自己的校验）。
    fn normalize_url(url: &str) -> Result<String, NewsError> {
        let trimmed = url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            Ok(trimmed.to_string())
        } else {
            Err(NewsError::Store(format!(
                "news source url is invalid: {trimmed}"
            )))
        }
    }
}

#[async_trait::async_trait]
impl<F: FeedFetcherPort> NewsIngestPort for NewsIngestService<F> {
    async fn refresh(&self) -> Result<NewsRefreshReport, NewsError> {
        let sources = self
            .service
            .repository
            .list_sources()
            .map_err(store_failure)?;
        if sources.is_empty() {
            return Ok(NewsRefreshReport::default());
        }

        // 抓取阶段：共享并发骨架（ADR-010：抓取是基础设施）。
        let urls: Vec<String> = sources.iter().map(|source| source.url.clone()).collect();
        let results = crate::feed::fetch_many(&urls, &self.fetcher).await;

        // 落库阶段：逐源写 news.db，单源失败只记 `last_error`，不中断其它源。
        let mut report = NewsRefreshReport::default();
        for (source, (_url, result)) in sources.into_iter().zip(results) {
            match result {
                Ok(fetched) => {
                    let inserted = self
                        .service
                        .repository
                        .insert_articles(source.id, &fetched.entries)
                        .map_err(store_failure)?;
                    report.new_articles += inserted;
                    self.service
                        .repository
                        .set_source_health(source.id, None)
                        .map_err(store_failure)?;
                }
                Err(error) => {
                    report.failures.push(NewsRefreshFailure {
                        source: source.name,
                        message: error.message.clone(),
                    });
                    self.service
                        .repository
                        .set_source_health(source.id, Some(&error.message))
                        .map_err(store_failure)?;
                }
            }
        }
        Ok(report)
    }

    async fn add_source(&self, url: &str, category: NewsCategory) -> Result<NewsSource, NewsError> {
        let normalized = Self::normalize_url(url)?;
        if self
            .service
            .repository
            .find_source_id_by_url(&normalized)
            .map_err(store_failure)?
            .is_some()
        {
            return Err(NewsError::Store(format!(
                "news source already added: {normalized}"
            )));
        }

        // 抓取阶段（可跨 await）→ 落库阶段（同步短锁），与 rss 用例同构。
        let fetched: FetchedFeed = self
            .fetcher
            .fetch_feed(&normalized)
            .await
            .map_err(|error| NewsError::Fetch(format!("{}: {error}", error.kind_label())))?;
        let source_id = self
            .service
            .repository
            .insert_source(
                &fetched.title,
                &normalized,
                category,
                fetched.site_url.as_deref(),
            )
            .map_err(store_failure)?;
        self.service
            .repository
            .insert_articles(source_id, &fetched.entries)
            .map_err(store_failure)?;
        self.service
            .repository
            .set_source_health(source_id, None)
            .map_err(store_failure)?;

        self.service
            .repository
            .source_by_id(source_id)
            .map_err(store_failure)?
            .ok_or_else(|| NewsError::Store("news source missing after insert".into()))
    }
}
