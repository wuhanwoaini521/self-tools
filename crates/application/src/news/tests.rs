//! News 用例测试（ADR-010：Fake repository + 脚本化抓取器，无 SQLite / 无网络）。
//!
//! 覆盖：空库降级 / latest 排序与限量 / 搜索命中与空关键词 / 分类与源过滤 /
//! 未知源受控失败 / 收藏与已读 / 未知源删除与改分类 / refresh 单源失败隔离 /
//! add_source 判重 / 推荐源目录质量 + **不自动订阅**。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use devtoolbox_core::feed::{FetchedEntry, FetchedFeed};
use devtoolbox_core::news::{NewsArticle, NewsCategory, NewsSource, NewsSourceType};

use crate::feed::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort};
use crate::news::ports::{
    NewsError, NewsIngestPort, NewsPort, NewsRepositoryPort, NewsSourceHealth,
};
use crate::news::{NewsIngestService, NewsService};

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 内存版 News 持久化端口（行为对齐 `NewsRepository`：URL 唯一、guid 去重）。
#[derive(Default)]
struct FakeNewsStore {
    sources: Vec<NewsSource>,
    articles: Vec<NewsArticle>,
    next_source_id: i64,
    next_article_id: i64,
}

impl FakeNewsStore {
    fn new() -> Self {
        Self {
            next_source_id: 1,
            next_article_id: 1,
            ..Self::default()
        }
    }
}

impl NewsRepositoryPort for FakeNewsStore {
    fn list_sources(&self) -> Result<Vec<NewsSource>, String> {
        let mut sources = self.sources.clone();
        for source in sources.iter_mut() {
            source.unread_count = self
                .articles
                .iter()
                .filter(|article| article.source_id == source.id && !article.is_read)
                .count() as i64;
        }
        Ok(sources)
    }

    fn source_by_id(&self, source_id: i64) -> Result<Option<NewsSource>, String> {
        Ok(self
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .cloned())
    }

    fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        Ok(self
            .sources
            .iter()
            .find(|source| source.url == url)
            .map(|source| source.id))
    }

    fn insert_source(
        &self,
        _name: &str,
        _url: &str,
        _category: NewsCategory,
        _site_url: Option<&str>,
    ) -> Result<i64, String> {
        unreachable!("insert_source 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn set_source_category(&self, _source_id: i64, _category: NewsCategory) -> Result<(), String> {
        unreachable!("set_source_category 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn set_source_health(&self, _source_id: i64, _error: Option<&str>) -> Result<(), String> {
        unreachable!("set_source_health 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn delete_source(&self, _source_id: i64) -> Result<(), String> {
        unreachable!("delete_source 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn has_failed_source(&self) -> Result<bool, String> {
        Ok(self
            .sources
            .iter()
            .any(|source| source.last_error.is_some()))
    }

    fn insert_articles(&self, _source_id: i64, _entries: &[FetchedEntry]) -> Result<usize, String> {
        unreachable!("insert_articles 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        let mut rows = self.articles.clone();
        rows.sort_by(|a, b| {
            b.published_at
                .unwrap_or(i64::MIN)
                .cmp(&a.published_at.unwrap_or(i64::MIN))
                .then(b.id.cmp(&a.id))
        });
        Ok(rows.into_iter().take(limit as usize).collect())
    }

    fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        let ids: Vec<i64> = self
            .sources
            .iter()
            .filter(|source| source.category == category)
            .map(|source| source.id)
            .collect();
        let mut rows: Vec<NewsArticle> = self
            .latest(i64::MAX)?
            .into_iter()
            .filter(|article| ids.contains(&article.source_id))
            .collect();
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }

    fn latest_by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, String> {
        let mut rows: Vec<NewsArticle> = self
            .latest(i64::MAX)?
            .into_iter()
            .filter(|article| article.source_id == source_id)
            .collect();
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }

    fn article_by_id(&self, article_id: i64) -> Result<Option<NewsArticle>, String> {
        Ok(self
            .articles
            .iter()
            .find(|article| article.id == article_id)
            .cloned())
    }

    fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        let needle = keyword.map(|value| value.trim().to_lowercase());
        let mut rows: Vec<NewsArticle> = self
            .latest(i64::MAX)?
            .into_iter()
            .filter(|article| source_id.is_none_or(|id| article.source_id == id))
            .filter(|article| {
                let Some(needle) = needle.as_ref() else {
                    return true;
                };
                if needle.is_empty() {
                    return true;
                }
                let haystack = format!(
                    "{} {} {}",
                    article.title,
                    article.author.as_deref().unwrap_or_default(),
                    article.summary.as_deref().unwrap_or_default()
                )
                .to_lowercase();
                haystack.contains(needle)
            })
            .collect();
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }

    fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        let mut rows: Vec<NewsArticle> = self
            .latest(i64::MAX)?
            .into_iter()
            .filter(|article| article.starred)
            .collect();
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }

    fn toggle_star(&self, _article_id: i64) -> Result<bool, String> {
        unreachable!("toggle_star 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn mark_read(&self, _article_id: i64) -> Result<(), String> {
        unreachable!("mark_read 只经 &mut 路径；见 FakeNewsStoreMut")
    }

    fn unread_total(&self) -> Result<i64, String> {
        Ok(self
            .articles
            .iter()
            .filter(|article| !article.is_read)
            .count() as i64)
    }
}

// `RssRepositoryPort` 的 Fake 模式（`Arc<Mutex<State>>`）：写方法也走同一状态，
// 这里统一为 Mutex 包装的结构体，读写共用。
struct FakeNewsRepository(std::sync::Mutex<FakeNewsStore>);

impl FakeNewsRepository {
    fn new() -> Self {
        Self(std::sync::Mutex::new(FakeNewsStore::new()))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeNewsStore> {
        self.0.lock().expect("fake news store poisoned")
    }

    /// 种一个新闻源（测试用）。
    fn seed_source(&self, name: &str, url: &str, category: NewsCategory) -> i64 {
        let mut state = self.lock();
        let id = state.next_source_id;
        state.next_source_id += 1;
        state.sources.push(NewsSource {
            id,
            name: name.to_string(),
            url: url.to_string(),
            source_type: NewsSourceType::Rss,
            category,
            site_url: None,
            last_updated: None,
            last_error: None,
            unread_count: 0,
        });
        id
    }

    /// 种几篇文章（测试用）。
    fn seed_articles(&self, source_id: i64, entries: &[FetchedEntry]) -> usize {
        let mut state = self.lock();
        let source_name = state
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(|source| source.name.clone())
            .unwrap_or_default();
        let mut inserted = 0;
        for entry in entries {
            if state
                .articles
                .iter()
                .any(|article| article.source_id == source_id && article.guid == entry.guid)
            {
                continue;
            }
            let id = state.next_article_id;
            state.next_article_id += 1;
            state.articles.push(NewsArticle {
                id,
                source_id,
                source_name: source_name.clone(),
                guid: entry.guid.clone(),
                url: entry.url.clone(),
                title: entry.title.clone(),
                author: entry.author.clone(),
                image_url: entry.image_url.clone(),
                published_at: entry.published_at,
                summary: entry.summary.clone(),
                is_read: false,
                starred: false,
            });
            inserted += 1;
        }
        inserted
    }
}

impl NewsRepositoryPort for FakeNewsRepository {
    fn list_sources(&self) -> Result<Vec<NewsSource>, String> {
        self.lock().list_sources()
    }
    fn source_by_id(&self, source_id: i64) -> Result<Option<NewsSource>, String> {
        self.lock().source_by_id(source_id)
    }
    fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        self.lock().find_source_id_by_url(url)
    }
    fn insert_source(
        &self,
        name: &str,
        url: &str,
        category: NewsCategory,
        site_url: Option<&str>,
    ) -> Result<i64, String> {
        let mut state = self.lock();
        if state.sources.iter().any(|source| source.url == url) {
            return Err(format!("news source already exists: {url}"));
        }
        let id = state.next_source_id;
        state.next_source_id += 1;
        state.sources.push(NewsSource {
            id,
            name: name.to_string(),
            url: url.to_string(),
            source_type: NewsSourceType::Rss,
            category,
            site_url: site_url.map(str::to_string),
            last_updated: None,
            last_error: None,
            unread_count: 0,
        });
        Ok(id)
    }
    fn set_source_category(&self, source_id: i64, category: NewsCategory) -> Result<(), String> {
        let mut state = self.lock();
        let source = state
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .ok_or_else(|| format!("news source {source_id} not found"))?;
        source.category = category;
        Ok(())
    }
    fn set_source_health(&self, source_id: i64, error: Option<&str>) -> Result<(), String> {
        let mut state = self.lock();
        let source = state
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .ok_or_else(|| format!("news source {source_id} not found"))?;
        source.last_error = error.map(str::to_string);
        source.last_updated = Some(1_800_000_000);
        Ok(())
    }
    fn delete_source(&self, source_id: i64) -> Result<(), String> {
        let mut state = self.lock();
        if !state.sources.iter().any(|source| source.id == source_id) {
            return Err(format!("news source {source_id} not found"));
        }
        state.sources.retain(|source| source.id != source_id);
        state
            .articles
            .retain(|article| article.source_id != source_id);
        Ok(())
    }
    fn has_failed_source(&self) -> Result<bool, String> {
        self.lock().has_failed_source()
    }
    fn insert_articles(&self, source_id: i64, entries: &[FetchedEntry]) -> Result<usize, String> {
        Ok(self.seed_articles(source_id, entries))
    }
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock().latest(limit)
    }
    fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.lock().latest_by_category(category, limit)
    }
    fn latest_by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock().latest_by_source(source_id, limit)
    }
    fn article_by_id(&self, article_id: i64) -> Result<Option<NewsArticle>, String> {
        self.lock().article_by_id(article_id)
    }
    fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.lock().query_articles(keyword, source_id, limit)
    }
    fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.lock().starred_articles(limit)
    }
    fn toggle_star(&self, article_id: i64) -> Result<bool, String> {
        let mut state = self.lock();
        let article = state
            .articles
            .iter_mut()
            .find(|article| article.id == article_id)
            .ok_or_else(|| format!("news article {article_id} not found"))?;
        article.starred = !article.starred;
        Ok(article.starred)
    }
    fn mark_read(&self, article_id: i64) -> Result<(), String> {
        let mut state = self.lock();
        let article = state
            .articles
            .iter_mut()
            .find(|article| article.id == article_id)
            .ok_or_else(|| format!("news article {article_id} not found"))?;
        article.is_read = true;
        Ok(())
    }
    fn unread_total(&self) -> Result<i64, String> {
        self.lock().unread_total()
    }
}

/// 脚本化抓取器：按 URL 返回预设结果；未配置的 URL 默认失败。
struct ScriptedFetcher {
    results: Mutex<HashMap<String, Result<FetchedFeed, FeedFetchError>>>,
}

impl ScriptedFetcher {
    fn new(results: Vec<(&str, Result<FetchedFeed, FeedFetchError>)>) -> Self {
        Self {
            results: Mutex::new(
                results
                    .into_iter()
                    .map(|(url, result)| (url.to_string(), result))
                    .collect(),
            ),
        }
    }
}

impl FeedFetcherPort for ScriptedFetcher {
    #[allow(clippy::manual_async_fn)]
    fn fetch_feed(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<FetchedFeed, FeedFetchError>> + Send {
        async move {
            let results = self.results.lock().expect("scripted fetcher poisoned");
            results.get(url).cloned().unwrap_or_else(|| {
                Err(FeedFetchError {
                    kind: FeedFetchErrorKind::Fetch,
                    message: format!("no scripted result for {url}"),
                })
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn feed(title: &str, guids: &[(&str, &str, i64)]) -> FetchedFeed {
    FetchedFeed {
        title: title.to_string(),
        site_url: Some("https://example.com".to_string()),
        entries: guids
            .iter()
            .map(|(guid, title, published_at)| FetchedEntry {
                guid: (*guid).to_string(),
                url: format!("https://example.com/{guid}"),
                title: (*title).to_string(),
                author: Some("Desk".to_string()),
                image_url: None,
                published_at: Some(*published_at),
                summary: Some("summary".to_string()),
            })
            .collect(),
    }
}

fn entry(guid: &str, title: &str, published_at: i64) -> FetchedEntry {
    FetchedEntry {
        guid: guid.to_string(),
        url: format!("https://example.com/{guid}"),
        title: title.to_string(),
        author: Some("Desk".to_string()),
        image_url: None,
        published_at: Some(published_at),
        summary: Some("summary".to_string()),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

fn service(store: &Arc<FakeNewsRepository>) -> Arc<NewsService> {
    Arc::new(NewsService::new(Arc::clone(
        &(Arc::clone(store) as Arc<dyn NewsRepositoryPort>),
    )))
}

fn seeded() -> (Arc<FakeNewsRepository>, Arc<NewsService>, i64) {
    let store = Arc::new(FakeNewsRepository::new());
    let source = store.seed_source("Wire", "https://wire.example/rss", NewsCategory::General);
    store.seed_articles(
        source,
        &[
            entry("a", "Old story", 1_600_000_000),
            entry("b", "New story", 1_800_000_000),
            entry("c", "Mid story", 1_700_000_000),
        ],
    );
    let service = service(&store);
    (store, service, source)
}

// ---------------------------------------------------------------------------
// NewsService tests
// ---------------------------------------------------------------------------

#[test]
fn empty_store_degrades_to_empty_healthy_view() {
    let store = Arc::new(FakeNewsRepository::new());
    let service = service(&store);

    let view = service.sources().expect("sources");
    assert!(view.sources.is_empty());
    assert_eq!(
        view.health,
        NewsSourceHealth::Healthy,
        "空库不是降级，是未配置"
    );
    assert!(service.latest(10).expect("latest").is_empty());
    assert!(service.starred(10).expect("starred").is_empty());
}

#[test]
fn latest_is_newest_first_and_bounded() {
    let (_store, service, _source) = seeded();
    let rows = service.latest(2).expect("latest");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "New story");
    assert_eq!(rows[1].title, "Mid story");
    assert_eq!(rows[0].source_name, "Wire");
    assert_eq!(rows[0].author.as_deref(), Some("Desk"));
}

#[test]
fn search_is_case_insensitive_and_rejects_blank() {
    let (_store, service, _source) = seeded();
    assert_eq!(service.search("new story", 10).expect("lower").len(), 1);
    assert_eq!(service.search("NEW STORY", 10).expect("upper").len(), 1);
    assert!(
        service.search("earthquake", 10).expect("no hit").is_empty(),
        "无命中是空结果，不是错误"
    );

    let error = service.search("   ", 10).expect_err("blank keyword");
    assert!(
        matches!(error, NewsError::Store(_)) && error.to_string().contains("empty"),
        "空关键词必须是参数错误"
    );
}

#[test]
fn by_category_and_by_source_filter_correctly() {
    let store = Arc::new(FakeNewsRepository::new());
    let tech = store.seed_source("HN", "https://hnrss.org/frontpage", NewsCategory::Tech);
    store.seed_source("Xinhua", "https://news.cn/rss.xml", NewsCategory::China);
    store.seed_articles(tech, &[entry("t", "Rust 2.0 released", 1_800_000_000)]);
    let service = service(&store);

    let tech_rows = service.by_category(NewsCategory::Tech, 10).expect("tech");
    assert_eq!(tech_rows.len(), 1);
    assert_eq!(tech_rows[0].title, "Rust 2.0 released");

    let china_rows = service.by_category(NewsCategory::China, 10).expect("china");
    assert!(china_rows.is_empty(), "没文章的分类返回空，不是错误");

    let by_source = service.by_source(tech, 10).expect("by source");
    assert_eq!(by_source.len(), 1);
}

#[test]
fn unknown_source_is_a_controlled_failure() {
    let (_store, service, _source) = seeded();
    let error = service.by_source(9999, 10).expect_err("unknown source");
    assert_eq!(error, NewsError::SourceNotFound(9999));
    assert_eq!(
        service.remove_source(9999).expect_err("unknown source"),
        NewsError::SourceNotFound(9999),
        "删除未知源同样受控失败"
    );
}

#[test]
fn star_and_read_round_trip() {
    let (_store, service, _source) = seeded();
    let target = service.latest(1).expect("latest")[0].id;

    assert!(service.toggle_star(target).expect("star"));
    assert_eq!(service.starred(10).expect("starred").len(), 1);
    assert!(!service.toggle_star(target).expect("unstar"));
    assert!(service.starred(10).expect("starred").is_empty());

    service.mark_read(target).expect("read");
    assert!(service.get_article(target).expect("article").is_read);
}

#[test]
fn get_article_reports_missing_as_controlled_failure() {
    let (_store, service, _source) = seeded();
    assert_eq!(
        service.get_article(9999).expect_err("missing"),
        NewsError::ArticleNotFound(9999)
    );
}

#[test]
fn remove_source_deletes_its_articles() {
    let (store, service, source) = seeded();
    assert!(!store.lock().articles.is_empty());
    service.remove_source(source).expect("remove");
    assert!(store.lock().articles.is_empty(), "删除源必须级联删文章");
    assert!(service.latest(10).expect("latest").is_empty());
}

#[test]
fn set_category_only_touches_news_source() {
    let (store, service, source) = seeded();
    service
        .set_category(source, NewsCategory::World)
        .expect("set category");
    assert_eq!(
        store.lock().sources[0].category,
        NewsCategory::World,
        "改分类只动 news_sources"
    );
    assert_eq!(store.lock().articles.len(), 3, "改分类不得改动任何新闻条目");
}

// ---------------------------------------------------------------------------
// NewsIngestService tests
// ---------------------------------------------------------------------------

#[test]
fn refresh_ingests_new_articles_and_isolates_failures() {
    let store = Arc::new(FakeNewsRepository::new());
    let good = store.seed_source("Wire", "https://wire.example/rss", NewsCategory::General);
    let bad = store.seed_source("Down", "https://down.example/rss", NewsCategory::General);

    let fetcher = ScriptedFetcher::new(vec![
        (
            "https://wire.example/rss",
            Ok(feed("Wire", &[("n1", "Fresh one", 1_900_000_000)])),
        ),
        (
            "https://down.example/rss",
            Err(FeedFetchError {
                kind: FeedFetchErrorKind::Fetch,
                message: "server returned 500".into(),
            }),
        ),
    ]);
    let ingest = NewsIngestService::new(service(&store), fetcher);

    let report = block_on(ingest.refresh()).expect("refresh");
    assert_eq!(report.new_articles, 1, "只统计实际新增");
    assert_eq!(report.failures.len(), 1, "单源失败被隔离记录");
    assert_eq!(report.failures[0].source, "Down");

    // 成功源补写健康态，失败源记录 last_error。
    assert_eq!(store.lock().sources[0].last_error, None);
    assert_eq!(
        store.lock().sources[1].last_error.as_deref(),
        Some("server returned 500")
    );

    // 读面看到新文章，且失败源不污染读结果。
    let rows = service(&store).latest(10).expect("latest");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Fresh one");
    assert_eq!(rows[0].source_id, good);
    assert_ne!(rows[0].source_id, bad);
}

#[test]
fn refresh_on_empty_store_is_a_no_op() {
    let store = Arc::new(FakeNewsRepository::new());
    let fetcher = ScriptedFetcher::new(vec![]);
    let ingest = NewsIngestService::new(service(&store), fetcher);
    let report = block_on(ingest.refresh()).expect("refresh");
    assert_eq!(report.new_articles, 0);
    assert!(report.failures.is_empty());
}

#[test]
fn add_source_fetches_then_persists_and_rejects_duplicates() {
    let store = Arc::new(FakeNewsRepository::new());
    let fetcher = ScriptedFetcher::new(vec![(
        "https://thepaper.cn/rss.xml",
        Ok(feed("澎湃", &[("p1", "Summit opens", 1_800_000_000)])),
    )]);
    let ingest = NewsIngestService::new(service(&store), fetcher);

    let source =
        block_on(ingest.add_source("  https://thepaper.cn/rss.xml  ", NewsCategory::General))
            .expect("add source");
    assert_eq!(source.name, "澎湃", "标题来自抓取结果");
    assert_eq!(source.category, NewsCategory::General);
    assert_eq!(store.lock().articles.len(), 1, "添加时顺带落地首批文章");

    // 同 URL 再加 → 受控失败（不是静默重复）。
    let error = block_on(ingest.add_source("https://thepaper.cn/rss.xml", NewsCategory::General))
        .expect_err("duplicate");
    assert!(error.to_string().contains("already added"));
    assert_eq!(store.lock().sources.len(), 1);

    // 非 http(s) 被拒。
    let error = block_on(ingest.add_source("ftp://example.com/rss", NewsCategory::General))
        .expect_err("scheme");
    assert!(error.to_string().contains("invalid"));

    // 抓取失败 → Fetch 错误，且不落库。
    let error = block_on(ingest.add_source("https://unconfigured.example/rss", NewsCategory::Tech))
        .expect_err("fetch failure");
    assert!(matches!(error, NewsError::Fetch(_)));
    assert_eq!(store.lock().sources.len(), 1, "抓取失败不得留半条记录");
}

// ---------------------------------------------------------------------------
// 推荐源目录
// ---------------------------------------------------------------------------

#[test]
fn recommended_sources_are_well_formed_and_cover_every_category() {
    let sources = devtoolbox_core::news::recommended_sources();
    assert!(!sources.is_empty());
    for category in NewsCategory::ALL {
        assert!(
            sources.iter().any(|source| source.category == category),
            "推荐目录必须覆盖分类 {category:?}"
        );
    }
    for source in &sources {
        assert!(
            source.url.starts_with("https://") || source.url.starts_with("http://"),
            "非法 feed url: {}",
            source.url
        );
        assert!(!source.name.trim().is_empty(), "name 不能为空");
        assert!(!source.note.trim().is_empty(), "note 不能为空");
    }
    // URL 唯一。
    let mut urls: Vec<&str> = sources.iter().map(|source| source.url.as_str()).collect();
    urls.sort_unstable();
    let before = urls.len();
    urls.dedup();
    assert_eq!(before, urls.len(), "推荐目录里有重复 URL");
}

#[test]
fn recommendations_never_subscribe_automatically() {
    // 推荐是候选：NewsService 不认识目录 —— 空库必须保持空。
    let store = Arc::new(FakeNewsRepository::new());
    let service = service(&store);
    let view = service.sources().expect("sources");
    assert!(view.sources.is_empty(), "推荐目录不得自动变成新闻源");
    assert!(service.latest(10).expect("latest").is_empty());
}
