//! RSS 用例编排：添加 / 刷新 / 列表 / 已读 / 删除（Gate 7.6）。
//!
//! 网络在 infrastructure 的抓取适配器（经 `FeedFetcherPort`），持久化在
//! infrastructure 的仓储适配器（经 `RssRepositoryPort`）。应用层不再持有
//! `reqwest::Client`，也不直接接触 SQLite。
//!
//! 两段式约束保持不变：**抓取阶段**（仅 `fetcher`，可跨 `.await`）与
//! **落库阶段**（仅 `repository`，同步短锁），上层据此组织命令实现。
//!
//! - 添加时先抓取解析，非法 Feed 直接报错，不落库；
//! - 刷新逐 Feed 并发执行，单个 Feed 失败只记录到该 Feed 的 `last_error`；
//! - 去重由存储层 `(feed_id, guid)` 唯一约束保证，guid 三级回退由抓取层归一化。

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use devtoolbox_core::rss::{ArticleRow, FeedRow, FetchedFeed};

use super::ports::{FeedFetchError, FeedFetcherPort, RssRepositoryPort};
use crate::error::{ApplicationError, RssErrorKind};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedDto {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub site_url: Option<String>,
    pub unread_count: i64,
    pub last_updated: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArticleDto {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub title: String,
    pub url: String,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RefreshReport {
    pub new_articles: usize,
    pub failures: Vec<RefreshFailure>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RefreshFailure {
    pub feed_title: String,
    pub message: String,
}

/// 刷新任务快照：抓取阶段只需要 id / title / url。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedSnapshot {
    pub id: i64,
    pub title: String,
    pub url: String,
}

impl From<FeedRow> for FeedDto {
    fn from(row: FeedRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            url: row.url,
            site_url: row.site_url,
            unread_count: row.unread_count,
            last_updated: row.last_updated,
            last_error: row.last_error,
        }
    }
}

impl From<ArticleRow> for ArticleDto {
    fn from(row: ArticleRow) -> Self {
        Self {
            id: row.id,
            feed_id: row.feed_id,
            feed_title: row.feed_title,
            title: row.title,
            url: row.url,
            published_at: row.published_at,
            summary: row.summary,
            is_read: row.is_read,
        }
    }
}

fn rss_error(kind: RssErrorKind, message: String) -> ApplicationError {
    ApplicationError::Rss { kind, message }
}

fn repository_error(message: String) -> ApplicationError {
    rss_error(RssErrorKind::Repository, message)
}

fn fetcher_error(error: FeedFetchError) -> ApplicationError {
    rss_error(
        match error.kind {
            super::ports::FeedFetchErrorKind::Fetch => RssErrorKind::Fetch,
            super::ports::FeedFetchErrorKind::Parse => RssErrorKind::Parse,
        },
        error.message,
    )
}

/// 校验并规范化 Feed URL（仅接受 http/https）。
pub fn validate_feed_url(url: &str) -> Result<String, ApplicationError> {
    let trimmed = url.trim().to_string();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed)
    } else {
        Err(ApplicationError::InvalidFeedUrl(trimmed))
    }
}

/// 抓取阶段：解析一个新 Feed（添加订阅用）。不触碰存储。
pub async fn fetch_new_feed<F: FeedFetcherPort + ?Sized>(
    url: &str,
    fetcher: &F,
) -> Result<FetchedFeed, ApplicationError> {
    let url = validate_feed_url(url)?;
    fetcher.fetch_feed(&url).await.map_err(fetcher_error)
}

/// 落库阶段：重复 URL 直接报错；否则写入 Feed 与首批文章。
pub fn commit_new_feed(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    url: &str,
    fetched: FetchedFeed,
) -> Result<FeedDto, ApplicationError> {
    let url = validate_feed_url(url)?;
    if repository
        .find_feed_id_by_url(&url)
        .map_err(repository_error)?
        .is_some()
    {
        return Err(ApplicationError::DuplicateFeed(url));
    }
    let feed_id = repository
        .insert_feed(&fetched.title, &url, fetched.site_url.as_deref())
        .map_err(repository_error)?;
    repository
        .insert_articles(feed_id, &fetched.entries)
        .map_err(repository_error)?;
    repository
        .set_feed_success(feed_id)
        .map_err(repository_error)?;
    repository
        .list_feeds()
        .map_err(repository_error)?
        .into_iter()
        .find(|feed| feed.id == feed_id)
        .map(FeedDto::from)
        .ok_or(ApplicationError::FeedNotFound(feed_id))
}

/// 抓取阶段：刷新所有订阅。并发抓取，逐 Feed 返回结果，单个失败不影响其他。
pub async fn fetch_all_feeds<F: FeedFetcherPort + ?Sized>(
    snapshots: &[FeedSnapshot],
    fetcher: &F,
) -> Vec<(FeedSnapshot, Result<FetchedFeed, FeedFetchError>)> {
    let fetches = snapshots.iter().map(|feed| async move {
        let result = fetcher.fetch_feed(&feed.url).await;
        (feed.clone(), result)
    });
    join_all(fetches).await
}

/// 落库阶段：写入刷新结果并汇总报告。
pub fn commit_refresh(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    results: Vec<(
        FeedSnapshot,
        Result<FetchedFeed, FeedFetchError>,
    )>,
) -> Result<RefreshReport, ApplicationError> {
    let mut report = RefreshReport::default();
    for (snapshot, result) in results {
        match result {
            Ok(fetched) => {
                let inserted = repository
                    .insert_articles(snapshot.id, &fetched.entries)
                    .map_err(repository_error)?;
                report.new_articles += inserted;
                repository
                    .set_feed_success(snapshot.id)
                    .map_err(repository_error)?;
            }
            Err(error) => {
                report.failures.push(RefreshFailure {
                    feed_title: snapshot.title.clone(),
                    message: error.message.clone(),
                });
                repository
                    .set_feed_error(snapshot.id, &error.message)
                    .map_err(repository_error)?;
            }
        }
    }
    Ok(report)
}

pub fn feed_snapshots(
    repository: &(dyn RssRepositoryPort + Send + Sync),
) -> Result<Vec<FeedSnapshot>, ApplicationError> {
    Ok(repository
        .list_feeds()
        .map_err(repository_error)?
        .into_iter()
        .map(|feed| FeedSnapshot {
            id: feed.id,
            title: feed.title,
            url: feed.url,
        })
        .collect())
}

pub fn list_feeds(
    repository: &(dyn RssRepositoryPort + Send + Sync),
) -> Result<Vec<FeedDto>, ApplicationError> {
    Ok(repository
        .list_feeds()
        .map_err(repository_error)?
        .into_iter()
        .map(FeedDto::from)
        .collect())
}

pub fn list_articles(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    feed_id: i64,
    limit: i64,
) -> Result<Vec<ArticleDto>, ApplicationError> {
    if repository
        .feed_title(feed_id)
        .map_err(repository_error)?
        .is_none()
    {
        return Err(ApplicationError::FeedNotFound(feed_id));
    }
    Ok(repository
        .list_articles(feed_id, limit)
        .map_err(repository_error)?
        .into_iter()
        .map(ArticleDto::from)
        .collect())
}

pub fn latest_articles(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    limit: i64,
) -> Result<Vec<ArticleDto>, ApplicationError> {
    Ok(repository
        .latest_articles(limit)
        .map_err(repository_error)?
        .into_iter()
        .map(ArticleDto::from)
        .collect())
}

pub fn mark_article_read(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    article_id: i64,
) -> Result<(), ApplicationError> {
    repository
        .mark_article_read(article_id)
        .map_err(repository_error)
}

pub fn delete_feed(
    repository: &(dyn RssRepositoryPort + Send + Sync),
    feed_id: i64,
) -> Result<(), ApplicationError> {
    if repository
        .feed_title(feed_id)
        .map_err(repository_error)?
        .is_none()
    {
        return Err(ApplicationError::FeedNotFound(feed_id));
    }
    repository.delete_feed(feed_id).map_err(repository_error)
}