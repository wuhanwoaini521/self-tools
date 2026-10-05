//! 端到端冒烟测试（一次性验收）：真实 SQLite → 服务 → AI 工具 → MCP 暴露表。
//!
//! 验收标准（ADR-010）：
//! 1. **RSS 与 News 是两份独立存储**：写 news.db 不动 dashboard.db，反之亦然；
//! 2. News 服务读新闻源、抓取落地、分类/按源/检索/详情/收藏/已读全链路可通；
//! 3. RSS 服务读订阅、条目、检索、详情、已读全链路可通；
//! 4. Personal AI 分别注册 `news.*`(6) 与 `rss.*`(6)，命名空间不交叉；
//! 5. MCP 只读暴露 9 项（news 5 + rss 4），三个联网/本地写工具 fail-closed。

use std::sync::{Arc, Mutex};

use devtoolbox_core::ToolCallRequest;
use devtoolbox_core::feed::FetchedEntry;
use devtoolbox_core::mcp::{ExposureGroup, default_exposure};
use devtoolbox_core::news::NewsCategory;
use devtoolbox_core::rss::{ArticleRow, FeedRow};

use devtoolbox_application::feed::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort};
use devtoolbox_application::news::ports::{NewsIngestPort, NewsPort, NewsRepositoryPort};
use devtoolbox_application::news::{NewsArticle, NewsService, NewsSource, NewsSourceHealth};
use devtoolbox_application::personal_ai::registry::{ModuleRegistry, ToolRegistry};
use devtoolbox_application::personal_ai::{RssTools, register_news, register_rss};
use devtoolbox_application::rss::RssRepositoryPort;
use devtoolbox_application::rss::service::{RssPort, RssService};

use crate::news_store::NewsRepository;
use crate::rss_store::FeedRepository;

// ---------------------------------------------------------------------------
// 适配器（与组合根同形态：Mutex<Store> → Port）
// ---------------------------------------------------------------------------

struct NewsAdapter(Arc<Mutex<NewsRepository>>);

impl NewsRepositoryPort for NewsAdapter {
    fn list_sources(&self) -> Result<Vec<NewsSource>, String> {
        self.0
            .lock()
            .expect("news lock")
            .list_sources()
            .map_err(err)
    }
    fn source_by_id(&self, id: i64) -> Result<Option<NewsSource>, String> {
        self.0
            .lock()
            .expect("news lock")
            .source_by_id(id)
            .map_err(err)
    }
    fn find_source_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        self.0
            .lock()
            .expect("news lock")
            .find_source_id_by_url(url)
            .map_err(err)
    }
    fn insert_source(
        &self,
        name: &str,
        url: &str,
        category: NewsCategory,
        site_url: Option<&str>,
    ) -> Result<i64, String> {
        self.0
            .lock()
            .expect("news lock")
            .insert_source(name, url, category, site_url)
            .map_err(err)
    }
    fn set_source_category(&self, id: i64, category: NewsCategory) -> Result<(), String> {
        self.0
            .lock()
            .expect("news lock")
            .set_source_category(id, category)
            .map_err(err)
    }
    fn set_source_health(&self, id: i64, error: Option<&str>) -> Result<(), String> {
        self.0
            .lock()
            .expect("news lock")
            .set_source_health(id, error)
            .map_err(err)
    }
    fn delete_source(&self, id: i64) -> Result<(), String> {
        self.0
            .lock()
            .expect("news lock")
            .delete_source(id)
            .map_err(err)
    }
    fn has_failed_source(&self) -> Result<bool, String> {
        self.0
            .lock()
            .expect("news lock")
            .has_failed_source()
            .map_err(err)
    }
    fn insert_articles(&self, source_id: i64, entries: &[FetchedEntry]) -> Result<usize, String> {
        self.0
            .lock()
            .expect("news lock")
            .insert_articles(source_id, entries)
            .map_err(err)
    }
    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.0.lock().expect("news lock").latest(limit).map_err(err)
    }
    fn latest_by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.0
            .lock()
            .expect("news lock")
            .latest_by_category(category, limit)
            .map_err(err)
    }
    fn latest_by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.0
            .lock()
            .expect("news lock")
            .latest_by_source(source_id, limit)
            .map_err(err)
    }
    fn article_by_id(&self, article_id: i64) -> Result<Option<NewsArticle>, String> {
        self.0
            .lock()
            .expect("news lock")
            .article_by_id(article_id)
            .map_err(err)
    }
    fn query_articles(
        &self,
        keyword: Option<&str>,
        source_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, String> {
        self.0
            .lock()
            .expect("news lock")
            .query_articles(keyword, source_id, limit)
            .map_err(err)
    }
    fn starred_articles(&self, limit: i64) -> Result<Vec<NewsArticle>, String> {
        self.0
            .lock()
            .expect("news lock")
            .starred_articles(limit)
            .map_err(err)
    }
    fn toggle_star(&self, article_id: i64) -> Result<bool, String> {
        self.0
            .lock()
            .expect("news lock")
            .toggle_star(article_id)
            .map_err(err)
    }
    fn mark_read(&self, article_id: i64) -> Result<(), String> {
        self.0
            .lock()
            .expect("news lock")
            .mark_read(article_id)
            .map_err(err)
    }
    fn unread_total(&self) -> Result<i64, String> {
        self.0
            .lock()
            .expect("news lock")
            .unread_total()
            .map_err(err)
    }
}

struct RssAdapter(Arc<Mutex<FeedRepository>>);

impl RssRepositoryPort for RssAdapter {
    fn list_feeds(&self) -> Result<Vec<FeedRow>, String> {
        self.0.lock().expect("rss lock").list_feeds().map_err(err)
    }
    fn find_feed_id_by_url(&self, url: &str) -> Result<Option<i64>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .find_feed_id_by_url(url)
            .map_err(err)
    }
    fn insert_feed(&self, title: &str, url: &str, site_url: Option<&str>) -> Result<i64, String> {
        self.0
            .lock()
            .expect("rss lock")
            .insert_feed(title, url, site_url)
            .map_err(err)
    }
    fn insert_articles(&self, feed_id: i64, entries: &[FetchedEntry]) -> Result<usize, String> {
        self.0
            .lock()
            .expect("rss lock")
            .insert_articles(feed_id, entries)
            .map_err(err)
    }
    fn set_feed_success(&self, feed_id: i64) -> Result<(), String> {
        self.0
            .lock()
            .expect("rss lock")
            .set_feed_success(feed_id)
            .map_err(err)
    }
    fn set_feed_error(&self, feed_id: i64, message: &str) -> Result<(), String> {
        self.0
            .lock()
            .expect("rss lock")
            .set_feed_error(feed_id, message)
            .map_err(err)
    }
    fn feed_title(&self, feed_id: i64) -> Result<Option<String>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .feed_title(feed_id)
            .map_err(err)
    }
    fn list_articles(&self, feed_id: i64, limit: i64) -> Result<Vec<ArticleRow>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .list_articles(feed_id, limit)
            .map_err(err)
    }
    fn latest_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .latest_articles(limit)
            .map_err(err)
    }
    fn entry_by_id(&self, entry_id: i64) -> Result<Option<ArticleRow>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .entry_by_id(entry_id)
            .map_err(err)
    }
    fn mark_article_read(&self, article_id: i64) -> Result<(), String> {
        self.0
            .lock()
            .expect("rss lock")
            .mark_article_read(article_id)
            .map_err(err)
    }
    fn delete_feed(&self, feed_id: i64) -> Result<(), String> {
        self.0
            .lock()
            .expect("rss lock")
            .delete_feed(feed_id)
            .map_err(err)
    }
    fn query_articles(
        &self,
        keyword: Option<&str>,
        feed_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ArticleRow>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .query_articles(keyword, feed_id, limit)
            .map_err(err)
    }
    fn starred_articles(&self, limit: i64) -> Result<Vec<ArticleRow>, String> {
        self.0
            .lock()
            .expect("rss lock")
            .starred_articles(limit)
            .map_err(err)
    }
    fn toggle_article_star(&self, article_id: i64) -> Result<bool, String> {
        self.0
            .lock()
            .expect("rss lock")
            .toggle_article_star(article_id)
            .map_err(err)
    }
}

fn err(error: crate::error::InfrastructureError) -> String {
    error.to_string()
}

// ---------------------------------------------------------------------------
// 摄取 stub（不触网）
// ---------------------------------------------------------------------------

/// 只给 news.db 的源返回新文；给 RSS 的订阅返回一篇 + 一篇失败（隔离验证）。
struct StubFetcher;

impl FeedFetcherPort for StubFetcher {
    #[allow(clippy::manual_async_fn)]
    fn fetch_feed(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<devtoolbox_core::feed::FetchedFeed, FeedFetchError>> + Send
    {
        async move {
            if url.contains("down.example") {
                return Err(FeedFetchError {
                    kind: FeedFetchErrorKind::Fetch,
                    message: "server returned 500".into(),
                });
            }
            Ok(devtoolbox_core::feed::FetchedFeed {
                title: "Stub".into(),
                site_url: None,
                entries: vec![FetchedEntry {
                    guid: format!("id:{url}"),
                    url: url.to_string(),
                    title: format!("Fetched from {url}"),
                    author: Some("Desk".into()),
                    image_url: None,
                    published_at: Some(1_900_000_000),
                    summary: Some("<p>new</p>".into()),
                }],
            })
        }
    }
}

fn entry(guid: &str, title: &str, published_at: i64) -> FetchedEntry {
    FetchedEntry {
        guid: guid.to_string(),
        url: format!("https://example.com/{guid}"),
        title: title.to_string(),
        author: Some("Desk".to_string()),
        image_url: Some(format!("https://cdn.example.com/{guid}.jpg")),
        published_at: Some(published_at),
        summary: Some(format!("<p>summary of {guid}</p>")),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

// ---------------------------------------------------------------------------
// 冒烟
// ---------------------------------------------------------------------------

#[test]
fn news_end_to_end_real_sqlite_to_ai_tools_and_mcp() {
    let directory = tempfile::tempdir().expect("temp dir");
    // news.db 首次打开即 seed 系统维护的新闻源目录。
    let news_store = Arc::new(Mutex::new(
        NewsRepository::open(directory.path().join("news.db")).expect("open news db"),
    ));
    let seed_count = news_store
        .lock()
        .expect("news lock")
        .list_sources()
        .expect("list")
        .len();
    assert_eq!(
        seed_count,
        devtoolbox_core::news::recommended_sources().len(),
        "News 首次打开必须 seed 系统维护的新闻源"
    );

    // 落地：给一个 **China 分类**的 seed 源写两条新闻（真实 SQLite）。
    // 源地址取自当前目录（`recommended_sources()`）而不是写死 —— 上一版写死的
    // 新华网地址在 2026-10-05 已 404 并被迁移替换，这里再写死就会假失败。
    let seed_url = devtoolbox_core::news::recommended_sources()
        .into_iter()
        .find(|source| source.category == NewsCategory::China)
        .expect("catalog has a China source")
        .url;
    let wire = news_store
        .lock()
        .expect("news lock")
        .find_source_id_by_url(&seed_url)
        .expect("find seed")
        .expect("seed must exist");
    news_store
        .lock()
        .expect("news lock")
        .insert_articles(
            wire,
            &[
                entry("a", "Summit opens in Washington", 1_800_000_000),
                entry("b", "Typhoon makes landfall", 1_800_000_100),
            ],
        )
        .expect("insert articles");

    let repository: Arc<dyn NewsRepositoryPort> = Arc::new(NewsAdapter(Arc::clone(&news_store)));
    let service = Arc::new(NewsService::new(repository));

    // 读面：健康态 / 最新 / 按分类 / 按源 / 检索 / 详情 / 收藏 / 已读。
    let view = service.sources().expect("sources");
    assert_eq!(view.health, NewsSourceHealth::Healthy);
    let latest = service.latest(10).expect("latest");
    assert_eq!(latest.len(), 2);
    assert_eq!(latest[0].title, "Typhoon makes landfall", "最新在前");
    assert_eq!(latest[0].author.as_deref(), Some("Desk"));
    assert_eq!(latest[0].source_id, wire);

    let china = service
        .by_category(NewsCategory::China, 10)
        .expect("by category");
    assert_eq!(china.len(), 2, "该 seed 源归 China 分类");
    assert!(
        service
            .by_category(NewsCategory::Finance, 10)
            .expect("finance")
            .is_empty()
    );

    assert_eq!(service.by_source(wire, 10).expect("by source").len(), 2);
    assert_eq!(
        service.by_source(9999, 10).expect_err("unknown source"),
        devtoolbox_application::news::NewsError::SourceNotFound(9999)
    );

    let hits = service.search("typhoon", 10).expect("search");
    assert_eq!(hits.len(), 1, "大小写不敏感的子串检索");
    assert!(service.search("earthquake", 10).expect("no hit").is_empty());

    let target = latest[0].id;
    let article = service.get_article(target).expect("get article");
    assert_eq!(article.title, "Typhoon makes landfall");
    assert!(
        service
            .get_article(9999)
            .expect_err("missing")
            .to_string()
            .contains("9999")
    );

    assert!(service.toggle_star(target).expect("star"));
    assert_eq!(service.starred(10).expect("starred").len(), 1);
    service.mark_read(target).expect("read");
    assert!(service.get_article(target).expect("article").is_read);

    // 摄取：真实 news.db 刷新，单源失败隔离。
    let ingest =
        devtoolbox_application::news::NewsIngestService::new(Arc::clone(&service), StubFetcher);
    let report = block_on(ingest.refresh()).expect("refresh");
    assert_eq!(report.new_articles, seed_count, "每个 seed 源各抓到一篇");
    // Stub 对所有 URL 都成功（无 down.example news 源），因此失败为 0。
    assert!(report.failures.is_empty());

    // 4. Personal AI：6 个 news.* 工具可触达，命名空间不交叉。
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_news(
        &mut modules,
        &mut tools,
        Arc::clone(&service) as Arc<dyn devtoolbox_application::news::NewsPort>,
        None,
    )
    .expect("register news");
    assert_eq!(tools.len(), 6);
    for name in devtoolbox_application::personal_ai::news_tool_names() {
        assert!(tools.spec(name).is_some(), "缺少 {name}");
        assert!(name.starts_with("news."));
    }

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.latest".into(),
        arguments: serde_json::json!({ "limit": 30 }),
    }))
    .expect("news.latest");
    assert!(result.ok);
    let articles = result.data.as_array().expect("array");
    assert_eq!(articles.len(), 2 + seed_count, "含刷新后的新文章");
    assert!(
        articles
            .iter()
            .any(|article| article["title"] == "Summit opens in Washington"),
        "AI 工具必须读到此前落库的新闻"
    );
    assert!(
        articles
            .iter()
            .all(|article| { !article["source"].as_str().unwrap_or_default().is_empty() }),
        "每条新闻都必须带来源名"
    );

    // refresh 在未装配 ingest 时如实降级（MCP 只读入口语义）。
    let degraded = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("refresh");
    assert!(!degraded.ok);
    assert!(degraded.error.expect("error").contains("未装配抓取能力"));

    // 5. MCP 暴露面：news 只读 5 项，refresh fail-closed。
    for exposed in [
        "news.latest",
        "news.search",
        "news.by_category",
        "news.by_source",
        "news.get_article",
    ] {
        let exposure = default_exposure(exposed).unwrap_or_else(|| panic!("{exposed} 应暴露"));
        assert_eq!(exposure.group, ExposureGroup::ModuleRead);
    }
    assert!(
        default_exposure("news.refresh").is_none(),
        "news.refresh 不得暴露"
    );
}

#[test]
fn rss_end_to_end_real_sqlite_to_ai_tools_and_mcp() {
    let directory = tempfile::tempdir().expect("temp dir");
    let rss_store = Arc::new(Mutex::new(
        FeedRepository::open(directory.path().join("dashboard.db")).expect("open rss db"),
    ));
    let feed = rss_store
        .lock()
        .expect("rss lock")
        .insert_feed(
            "V2EX",
            "https://v2ex.com/feed.xml",
            Some("https://v2ex.com/"),
        )
        .expect("insert feed");
    rss_store
        .lock()
        .expect("rss lock")
        .insert_articles(
            feed,
            &[
                entry("t1", "[程序员] 一号员工满月了", 1_800_000_000),
                entry("t2", "[问与答] paypal 解封", 1_700_000_000),
            ],
        )
        .expect("insert entries");

    let repository: Arc<dyn RssRepositoryPort> = Arc::new(RssAdapter(Arc::clone(&rss_store)));
    let service = Arc::new(RssService::new(repository));

    let subscriptions = service.list_subscriptions().expect("list subscriptions");
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].title, "V2EX");
    assert_eq!(subscriptions[0].unread_count, 2);

    let entries = service.list_entries(None, 10).expect("list entries");
    assert_eq!(entries.len(), 2);
    let scoped = service.list_entries(Some(feed), 1).expect("scoped");
    assert_eq!(scoped.len(), 1, "limit 生效");
    assert!(
        service
            .list_entries(Some(9999), 10)
            .expect_err("unknown feed")
            .to_string()
            .contains("9999")
    );

    assert_eq!(service.search("paypal", 10).expect("search").len(), 1);
    assert!(
        service
            .search("   ", 10)
            .expect_err("blank")
            .to_string()
            .contains("empty"),
        "空关键词必须是参数错误"
    );

    let entry_id = entries[0].id;
    let detail = service.get_entry(entry_id).expect("get entry");
    assert_eq!(detail.feed_title, "V2EX");
    assert_eq!(detail.author.as_deref(), Some("Desk"));
    assert!(
        service
            .get_entry(9999)
            .expect_err("missing")
            .to_string()
            .contains("9999")
    );

    service.mark_read(entry_id).expect("mark read");
    let after = service.get_entry(entry_id).expect("entry");
    assert!(after.is_read, "已读状态真实落库");

    // Personal AI：6 个 rss.* 工具可触达，命名空间不交叉。
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_rss(
        &mut modules,
        &mut tools,
        RssTools::new(Arc::clone(&service) as Arc<dyn RssPort>),
        None,
    )
    .expect("register rss");
    assert_eq!(tools.len(), 6);
    for name in devtoolbox_application::personal_ai::rss_tool_names() {
        assert!(tools.spec(name).is_some(), "缺少 {name}");
        assert!(name.starts_with("rss."));
    }

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.list_subscriptions".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("list subscriptions");
    assert!(result.ok);
    let feeds = result.data.as_array().expect("array");
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["unread_count"], 1, "标记已读后未读数下降");

    // refresh 未装配 ingest → 如实降级。
    let degraded = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "rss.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("rss refresh");
    assert!(!degraded.ok);
    assert!(degraded.error.expect("error").contains("未装配抓取能力"));

    // MCP 暴露面：rss 只读 4 项；写与联网 fail-closed。
    for exposed in [
        "rss.list_subscriptions",
        "rss.list_entries",
        "rss.search",
        "rss.get_entry",
    ] {
        let exposure = default_exposure(exposed).unwrap_or_else(|| panic!("{exposed} 应暴露"));
        assert_eq!(exposure.group, ExposureGroup::ModuleRead);
    }
    for hidden in ["rss.mark_read", "rss.refresh"] {
        assert!(default_exposure(hidden).is_none(), "不得暴露: {hidden}");
    }
}

#[test]
fn news_and_rss_stay_in_separate_databases() {
    // ADR-010 核心断言：写 news.db 不动 dashboard.db（反之亦然）。
    let directory = tempfile::tempdir().expect("temp dir");
    let news_path = directory.path().join("news.db");
    let rss_path = directory.path().join("dashboard.db");

    let news_store = Arc::new(Mutex::new(
        NewsRepository::open(&news_path).expect("news db"),
    ));
    let rss_store = Arc::new(Mutex::new(FeedRepository::open(&rss_path).expect("rss db")));

    // News 写入。
    let news_service = Arc::new(NewsService::new(Arc::new(NewsAdapter(Arc::clone(
        &news_store,
    )))));
    let ingest = devtoolbox_application::news::NewsIngestService::new(
        Arc::clone(&news_service),
        StubFetcher,
    );
    block_on(ingest.refresh()).expect("news refresh");

    // RSS 写入。
    let feed = rss_store
        .lock()
        .expect("rss lock")
        .insert_feed("Blog", "https://blog.example.com/feed", None)
        .expect("insert feed");
    rss_store
        .lock()
        .expect("rss lock")
        .insert_articles(feed, &[entry("b1", "Hello world", 1_800_000_000)])
        .expect("insert entry");

    // 边界：两边互不可见。
    let news_service = Arc::new(NewsService::new(Arc::new(NewsAdapter(Arc::clone(
        &news_store,
    )))));
    let news_view = news_service.sources().expect("news sources");
    assert!(
        !news_view.sources.iter().any(|source| source.name == "Blog"),
        "RSS 订阅不得出现在 News 源里"
    );
    assert!(
        news_service
            .search("Hello world", 10)
            .expect("news search")
            .is_empty(),
        "RSS 条目不得被 news.search 检索到"
    );

    let rss_service = Arc::new(RssService::new(Arc::new(RssAdapter(Arc::clone(
        &rss_store,
    )))));
    let rss_entries = rss_service.list_entries(None, 10).expect("rss entries");
    assert_eq!(rss_entries.len(), 1, "RSS 只有自己的条目");
    assert!(
        rss_service
            .search("Summit", 10)
            .expect("rss search")
            .is_empty(),
        "News 文章不得被 rss.search 检索到"
    );

    // 释放 service 和 store 触发 checkpoint 到主 db 文件。
    drop(news_service);
    drop(rss_service);
    drop(news_store);
    drop(rss_store);

    // 两个文件独立存在（不是同一份存储）。
    assert!(news_path.exists());
    assert!(rss_path.exists());
    let news_bytes = std::fs::read(&news_path).expect("read news db");
    let rss_bytes = std::fs::read(&rss_path).expect("read rss db");
    assert_ne!(
        std::hint::black_box(&news_bytes),
        std::hint::black_box(&rss_bytes),
        "必须是两份独立数据库文件"
    );
}
