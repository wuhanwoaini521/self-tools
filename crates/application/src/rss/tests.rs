//! RSS 用例测试（Gate 7.6：Fake 仓储 + Stub 抓取器，无 SQLite / 无网络）。
//!
//! 覆盖：URL 校验、添加去重、空 Feed、刷新成功/失败逐 Feed 传播、
//! 抓取失败不落库、级联删除。断言与迁移前的真实仓储行为保持一致。

use std::collections::HashMap;
use std::sync::Mutex;

use devtoolbox_core::rss::{ArticleRow, FeedRow, FetchedEntry, FetchedFeed};

use super::ports::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort, RssRepositoryPort};
use super::{commit_new_feed, commit_refresh, delete_feed, feed_snapshots, list_articles, list_feeds, validate_feed_url};
use crate::ApplicationError;

// ---------- Fakes ----------

/// 内存版 `RssRepositoryPort`：行为对齐 SQLite 实现
/// （`(feed_id, guid)` 去重、级联删除、unread 计数、last_error 记录）。
struct FakeRepositoryState {
    feeds: Vec<FeedRow>,
    articles: Vec<ArticleRow>,
    next_feed_id: i64,
    next_article_id: i64,
}

impl Default for FakeRepositoryState {
    fn default() -> Self {
        Self {
            feeds: Vec::new(),
            articles: Vec::new(),
            next_feed_id: 1,
            next_article_id: 1,
        }
    }
}

struct FakeRepository(Mutex<FakeRepositoryState>);

impl FakeRepository {
    fn new() -> Self {
        Self(Mutex::new(FakeRepositoryState::default()))
    }
}

impl RssRepositoryPort for FakeRepository {
    fn list_feeds(&self) -> Result<Vec<FeedRow>, String> {
        let state = self.0.lock().expect("fake repository poisoned");
        Ok(state.feeds.clone())
    }

    fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        let state = self.0.lock().expect("fake repository poisoned");
        Ok(state.feeds.iter().find(|feed| feed.url == url).map(|feed| feed.id))
    }

    fn insert_feed(
        &self,
        title: &str,
        url: &str,
        site_url: Option<&str>,
    ) -> Result<i64, String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        let id = state.next_feed_id;
        state.next_feed_id += 1;
        state.feeds.push(FeedRow {
            id,
            title: title.to_string(),
            url: url.to_string(),
            site_url: site_url.map(str::to_string),
            last_updated: None,
            last_error: None,
            unread_count: 0,
        });
        Ok(id)
    }

    fn insert_articles(&self, feed_id: i64, entries: &[FetchedEntry]) -> Result<usize, String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        let mut inserted = 0;
        for entry in entries {
            if state
                .articles
                .iter()
                .any(|article| article.feed_id == feed_id && article.guid == entry.guid)
            {
                continue;
            }
            let id = state.next_article_id;
            state.next_article_id += 1;
            let feed_title = state
                .feeds
                .iter()
                .find(|feed| feed.id == feed_id)
                .map(|feed| feed.title.clone())
                .unwrap_or_default();
            state.articles.push(ArticleRow {
                id,
                feed_id,
                feed_title,
                guid: entry.guid.clone(),
                url: entry.url.clone(),
                title: entry.title.clone(),
                published_at: entry.published_at,
                summary: entry.summary.clone(),
                is_read: false,
            });
            inserted += 1;
        }
        if let Some(feed) = state.feeds.iter_mut().find(|feed| feed.id == feed_id) {
            feed.unread_count += inserted as i64;
        }
        Ok(inserted)
    }

    fn set_feed_success(&self, feed_id: i64) -> Result<(), String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        if let Some(feed) = state.feeds.iter_mut().find(|feed| feed.id == feed_id) {
            feed.last_error = None;
        }
        Ok(())
    }

    fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        if let Some(feed) = state.feeds.iter_mut().find(|feed| feed.id == feed_id) {
            feed.last_error = Some(message.to_string());
        }
        Ok(())
    }

    fn feed_title(&self, feed_id: i64) -> Result<Option<String>, String> {
        let state = self.0.lock().expect("fake repository poisoned");
        Ok(state
            .feeds
            .iter()
            .find(|feed| feed.id == feed_id)
            .map(|feed| feed.title.clone()))
    }

    fn list_articles(&self, feed_id: i64, limit: i64) -> Result<Vec<ArticleRow>, String> {
        let state = self.0.lock().expect("fake repository poisoned");
        Ok(state
            .articles
            .iter()
            .filter(|article| article.feed_id == feed_id)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String> {
        let state = self.0.lock().expect("fake repository poisoned");
        Ok(state
            .articles
            .iter()
            .rev()
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn mark_article_read(&self, article_id: i64) -> Result<(), String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        if let Some(article) = state
            .articles
            .iter_mut()
            .find(|article| article.id == article_id)
        {
            article.is_read = true;
        }
        Ok(())
    }

    fn delete_feed(&self, feed_id: i64) -> Result<(), String> {
        let mut state = self.0.lock().expect("fake repository poisoned");
        state.feeds.retain(|feed| feed.id != feed_id);
        state.articles.retain(|article| article.feed_id != feed_id);
        Ok(())
    }
}

/// 脚本化抓取 Stub：按 URL 返回预设结果（成功 / 失败 / 空内容）。
struct ScriptedFetcher {
    results: Mutex<HashMap<String, Result<FetchedFeed, FeedFetchError>>>,
}

impl ScriptedFetcher {
    fn new(results: Vec<(String, Result<FetchedFeed, FeedFetchError>)>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
        }
    }
}

impl FeedFetcherPort for ScriptedFetcher {
    async fn fetch_feed(&self, url: &str) -> Result<FetchedFeed, FeedFetchError> {
        let results = self.results.lock().expect("scripted fetcher poisoned");
        match results.get(url).cloned() {
            Some(result) => result,
            None => Err(FeedFetchError {
                kind: FeedFetchErrorKind::Fetch,
                message: format!("no scripted result for {url}"),
            }),
        }
    }
}

// ---------- Fixtures ----------

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

// ---------- Tests ----------

#[test]
fn rejects_non_http_url() {
    assert!(validate_feed_url("ftp://example.com/rss").is_err());
    assert!(validate_feed_url("example.com/rss").is_err());
    assert_eq!(
        validate_feed_url("  https://example.com/rss ").expect("valid"),
        "https://example.com/rss"
    );
}

/// 添加：重复 URL 报 `DuplicateFeed`，不产生第二行。
#[test]
fn commit_new_feed_is_deduplicated() {
    let repository = FakeRepository::new();
    let first =
        commit_new_feed(&repository, "https://example.com/rss", fetched("Tech", &["a", "b"]))
            .expect("commit");
    assert_eq!(first.unread_count, 2);
    assert_eq!(list_feeds(&repository).expect("feeds").len(), 1);
    assert!(matches!(
        commit_new_feed(&repository, "https://example.com/rss", fetched("Tech", &["a"])),
        Err(ApplicationError::DuplicateFeed(_))
    ));
    assert_eq!(list_feeds(&repository).expect("feeds").len(), 1);
}

/// 刷新：成功 Feed 新增文章、失败 Feed 单独记录错误，互不影响（传播语义）。
#[test]
fn commit_refresh_propagates_per_feed_results() {
    let repository = FakeRepository::new();
    let good = commit_new_feed(&repository, "https://example.com/good", fetched("Good", &["a"]))
        .expect("commit good");
    let bad = commit_new_feed(&repository, "https://example.com/bad", fetched("Bad", &["b"]))
        .expect("commit bad");
    let snapshots = feed_snapshots(&repository).expect("snapshots");
    assert_eq!(snapshots.len(), 2);

    let results = vec![
        (snapshots[0].clone(), Ok(fetched("Good", &["a", "c"]))),
        (
            snapshots[1].clone(),
            Err(FeedFetchError {
                kind: FeedFetchErrorKind::Fetch,
                message: "server returned 500".to_string(),
            }),
        ),
    ];
    let report = commit_refresh(&repository, results).expect("report");
    assert_eq!(report.new_articles, 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].feed_title, "Bad");

    let feeds = list_feeds(&repository).expect("feeds");
    let good_dto = feeds.iter().find(|feed| feed.id == good.id).expect("good");
    let bad_dto = feeds.iter().find(|feed| feed.id == bad.id).expect("bad");
    assert_eq!(good_dto.unread_count, 2);
    assert!(good_dto.last_error.is_none());
    assert!(bad_dto.last_error.is_some());
    assert_eq!(
        list_articles(&repository, good.id, 100)
            .expect("articles")
            .len(),
        2
    );
}

/// 空 Feed（无条目）：添加与刷新均不产文章、不报错。
#[test]
fn empty_feed_commits_zero_articles() {
    let repository = FakeRepository::new();
    let feed =
        commit_new_feed(&repository, "https://example.com/empty", fetched("Empty", &[]))
            .expect("commit");
    assert_eq!(
        list_articles(&repository, feed.id, 10)
            .expect("articles")
            .len(),
        0
    );
    let report = commit_refresh(
        &repository,
        vec![(
            feed_snapshots(&repository).expect("snapshots")[0].clone(),
            Ok(fetched("Empty", &[])),
        )],
    )
    .expect("report");
    assert_eq!(report.new_articles, 0);
    assert!(report.failures.is_empty());
}

/// 抓取失败（超时/网络）：不落库、刷新报告中按 Feed 分离传播。
#[tokio::test]
async fn fetch_failure_is_reported_without_commit() {
    let repository = FakeRepository::new();
    let feed = commit_new_feed(&repository, "https://example.com/down", fetched("Down", &["a"]))
        .expect("commit");
    let snapshots = feed_snapshots(&repository).expect("snapshots");
    let fetcher = ScriptedFetcher::new(vec![(
        "https://example.com/down".to_string(),
        Err(FeedFetchError {
            kind: FeedFetchErrorKind::Fetch,
            message: "timed out".to_string(),
        }),
    )]);
    let results = super::fetch_all_feeds(&snapshots, &fetcher).await;
    let report = commit_refresh(&repository, results).expect("report");
    assert_eq!(report.new_articles, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].message.contains("timed out"));
    let listed = list_feeds(&repository).expect("feeds");
    assert!(
        listed
            .iter()
            .find(|row| row.id == feed.id)
            .expect("feed")
            .last_error
            .is_some()
    );
}

/// 删除订阅：级联删掉该 Feed 的文章，其他 Feed 不受影响。
#[test]
fn delete_feed_cascades_articles() {
    let repository = FakeRepository::new();
    let first = commit_new_feed(&repository, "https://example.com/one", fetched("One", &["a"]))
        .expect("commit one");
    let second = commit_new_feed(&repository, "https://example.com/two", fetched("Two", &["b"]))
        .expect("commit two");
    delete_feed(&repository, first.id).expect("delete");
    assert_eq!(list_feeds(&repository).expect("feeds").len(), 1);
    // 已删除订阅在读取层面表现为 FeedNotFound（级联删除后的可观察语义）。
    assert!(matches!(
        list_articles(&repository, first.id, 10),
        Err(ApplicationError::FeedNotFound(_))
    ));
    assert_eq!(
        list_articles(&repository, second.id, 10)
            .expect("articles")
            .len(),
        1
    );
}