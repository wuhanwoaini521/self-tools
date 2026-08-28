//! RSS 用例编排：添加 / 刷新 / 列表 / 已读 / 删除。
//!
//! 网络在 infrastructure(`fetch_feed`)，持久化在 infrastructure(`FeedRepository`)。
//! 关键约束：rusqlite `Connection` 非 `Sync`，因此工作流拆成两段——
//! **抓取阶段**(仅持有 client，可跨 `.await`)与**落库阶段**(短暂加锁，纯同步)，
//! 上层(Tauri 命令)据此保证不跨 await 持锁，也不会因网络阻塞其他命令。
//!
//! - 添加时先抓取解析，非法 Feed 直接报错，不落库；
//! - 刷新逐 Feed 并发执行，单个 Feed 失败只记录到该 Feed 的 `last_error`；
//! - 去重由存储层 `(feed_id, guid)` 唯一约束保证，guid 三级回退由抓取层归一化。

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use devtoolbox_infrastructure::{ArticleRow, FeedRepository, FeedRow, FetchedFeed, fetch_feed};

use crate::ApplicationError;

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

fn rss(source: devtoolbox_infrastructure::InfrastructureError) -> ApplicationError {
    source.into()
}

/// 校验并规范化 Feed URL(仅接受 http/https)。
pub fn validate_feed_url(url: &str) -> Result<String, ApplicationError> {
    let trimmed = url.trim().to_string();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed)
    } else {
        Err(ApplicationError::InvalidFeedUrl(trimmed))
    }
}

/// 抓取阶段：解析一个新 Feed(添加订阅用)。不触碰存储。
pub async fn fetch_new_feed(
    url: &str,
    client: &reqwest::Client,
) -> Result<FetchedFeed, ApplicationError> {
    let url = validate_feed_url(url)?;
    fetch_feed(&url, client).await.map_err(rss)
}

/// 落库阶段：重复 URL 直接报错；否则写入 Feed 与首批文章。
pub fn commit_new_feed(
    store: &FeedRepository,
    url: &str,
    fetched: FetchedFeed,
) -> Result<FeedDto, ApplicationError> {
    let url = validate_feed_url(url)?;
    if store.find_feed_id_by_url(&url).map_err(rss)?.is_some() {
        return Err(ApplicationError::DuplicateFeed(url));
    }
    let feed_id = store
        .insert_feed(&fetched.title, &url, fetched.site_url.as_deref())
        .map_err(rss)?;
    store
        .insert_articles(feed_id, &fetched.entries)
        .map_err(rss)?;
    store.set_feed_success(feed_id).map_err(rss)?;
    store
        .list_feeds()
        .map_err(rss)?
        .into_iter()
        .find(|feed| feed.id == feed_id)
        .map(FeedDto::from)
        .ok_or(ApplicationError::FeedNotFound(feed_id))
}

/// 抓取阶段：刷新所有订阅。并发抓取，逐 Feed 返回结果，单个失败不影响其他。
pub async fn fetch_all_feeds(
    snapshots: &[FeedSnapshot],
    client: &reqwest::Client,
) -> Vec<(
    FeedSnapshot,
    Result<FetchedFeed, devtoolbox_infrastructure::InfrastructureError>,
)> {
    let fetches = snapshots.iter().map(|feed| async move {
        let result = fetch_feed(&feed.url, client).await;
        (feed.clone(), result)
    });
    join_all(fetches).await
}

/// 落库阶段：写入刷新结果并汇总报告。
pub fn commit_refresh(
    store: &FeedRepository,
    results: Vec<(
        FeedSnapshot,
        Result<FetchedFeed, devtoolbox_infrastructure::InfrastructureError>,
    )>,
) -> Result<RefreshReport, ApplicationError> {
    let mut report = RefreshReport::default();
    for (snapshot, result) in results {
        match result {
            Ok(fetched) => {
                let inserted = store
                    .insert_articles(snapshot.id, &fetched.entries)
                    .map_err(rss)?;
                report.new_articles += inserted;
                store.set_feed_success(snapshot.id).map_err(rss)?;
            }
            Err(error) => {
                report.failures.push(RefreshFailure {
                    feed_title: snapshot.title.clone(),
                    message: error.to_string(),
                });
                store
                    .set_feed_error(snapshot.id, &error.to_string())
                    .map_err(rss)?;
            }
        }
    }
    Ok(report)
}

pub fn feed_snapshots(store: &FeedRepository) -> Result<Vec<FeedSnapshot>, ApplicationError> {
    Ok(store
        .list_feeds()
        .map_err(rss)?
        .into_iter()
        .map(|feed| FeedSnapshot {
            id: feed.id,
            title: feed.title,
            url: feed.url,
        })
        .collect())
}

pub fn list_feeds(store: &FeedRepository) -> Result<Vec<FeedDto>, ApplicationError> {
    Ok(store
        .list_feeds()
        .map_err(rss)?
        .into_iter()
        .map(FeedDto::from)
        .collect())
}

pub fn list_articles(
    store: &FeedRepository,
    feed_id: i64,
    limit: i64,
) -> Result<Vec<ArticleDto>, ApplicationError> {
    if store.feed_title(feed_id).map_err(rss)?.is_none() {
        return Err(ApplicationError::FeedNotFound(feed_id));
    }
    Ok(store
        .list_articles(feed_id, limit)
        .map_err(rss)?
        .into_iter()
        .map(ArticleDto::from)
        .collect())
}

pub fn latest_articles(
    store: &FeedRepository,
    limit: i64,
) -> Result<Vec<ArticleDto>, ApplicationError> {
    Ok(store
        .latest_articles(limit)
        .map_err(rss)?
        .into_iter()
        .map(ArticleDto::from)
        .collect())
}

pub fn mark_article_read(store: &FeedRepository, article_id: i64) -> Result<(), ApplicationError> {
    store.mark_article_read(article_id).map_err(rss)
}

pub fn delete_feed(store: &FeedRepository, feed_id: i64) -> Result<(), ApplicationError> {
    if store.feed_title(feed_id).map_err(rss)?.is_none() {
        return Err(ApplicationError::FeedNotFound(feed_id));
    }
    store.delete_feed(feed_id).map_err(rss)
}

#[cfg(test)]
mod tests {
    use devtoolbox_infrastructure::{FetchedEntry, FetchedFeed};

    use super::{commit_new_feed, commit_refresh, list_feeds, validate_feed_url};
    use crate::ApplicationError;

    fn fetched(title: &str, guids: &[&str]) -> FetchedFeed {
        FetchedFeed {
            title: title.to_string(),
            site_url: Some("https://example.com".to_string()),
            entries: guids
                .iter()
                .map(|guid| FetchedEntry {
                    guid: format!("id:{guid}"),
                    url: format!("https://example.com/{guid}"),
                    title: format!("Post {guid}"),
                    published_at: Some(1_700_000_000),
                    summary: Some("summary".to_string()),
                })
                .collect(),
        }
    }

    #[test]
    fn rejects_non_http_url() {
        assert!(validate_feed_url("ftp://example.com/rss").is_err());
        assert!(validate_feed_url("example.com/rss").is_err());
        assert_eq!(
            validate_feed_url("  https://example.com/rss ").expect("valid"),
            "https://example.com/rss"
        );
    }

    #[test]
    fn commit_new_feed_is_deduplicated() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store =
            devtoolbox_infrastructure::FeedRepository::open(directory.path().join("rss.db"))
                .expect("open");
        let first = commit_new_feed(
            &store,
            "https://example.com/rss",
            fetched("Tech", &["a", "b"]),
        )
        .expect("commit");
        assert_eq!(first.unread_count, 2);
        assert!(matches!(
            commit_new_feed(&store, "https://example.com/rss", fetched("Tech", &["a"])),
            Err(ApplicationError::DuplicateFeed(_))
        ));
    }

    #[test]
    fn commit_refresh_reports_failures_per_feed() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store =
            devtoolbox_infrastructure::FeedRepository::open(directory.path().join("rss.db"))
                .expect("open");
        let good = commit_new_feed(&store, "https://example.com/good", fetched("Good", &["a"]))
            .expect("commit good");
        let bad = commit_new_feed(&store, "https://example.com/bad", fetched("Bad", &["b"]))
            .expect("commit bad");
        let snapshots = vec![
            devtoolbox_infrastructure::FeedRow {
                id: good.id,
                title: good.title.clone(),
                url: good.url.clone(),
                site_url: None,
                last_updated: None,
                last_error: None,
                unread_count: 0,
            },
            devtoolbox_infrastructure::FeedRow {
                id: bad.id,
                title: bad.title.clone(),
                url: bad.url.clone(),
                site_url: None,
                last_updated: None,
                last_error: None,
                unread_count: 0,
            },
        ]
        .into_iter()
        .map(|row| super::FeedSnapshot {
            id: row.id,
            title: row.title,
            url: row.url,
        })
        .collect::<Vec<_>>();

        let results = vec![
            (snapshots[0].clone(), Ok(fetched("Good", &["a", "c"]))),
            (
                snapshots[1].clone(),
                Err(devtoolbox_infrastructure::InfrastructureError::FeedFetch(
                    "server returned 500".to_string(),
                )),
            ),
        ];
        let report = commit_refresh(&store, results).expect("report");
        assert_eq!(report.new_articles, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].feed_title, "Bad");

        let feeds = list_feeds(&store).expect("feeds");
        let good_dto = feeds.iter().find(|feed| feed.id == good.id).expect("good");
        let bad_dto = feeds.iter().find(|feed| feed.id == bad.id).expect("bad");
        assert_eq!(good_dto.unread_count, 2);
        assert!(good_dto.last_error.is_none());
        assert!(bad_dto.last_error.is_some());
    }
}
