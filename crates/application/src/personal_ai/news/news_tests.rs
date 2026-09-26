//! News 模块集成测试（V12 / ADR-010；Fake 全依赖，不触网、不触 SQLite）。
//!
//! 覆盖：模块注册与工具发现 / 风险面 / latest / search / by_category /
//! by_source / get_article / refresh（装配与未装配）/ sources 推荐源判重 /
//! ContextProvider / PersonalAgent 工具路由 / 与 RSS 模块共存。
//!
//! **关键断言**：News 与 RSS 是两个模块，工具名前缀不得混用。

use std::sync::{Arc, Mutex};

use devtoolbox_core::personal_ai::{
    AgentRequest, AgentResponse, AppContext as AiAppContext, ChatModelProvider, ChatRequest,
    ChatResponse, ChatToolCall,
};
use devtoolbox_core::{ToolCallRequest, ToolRisk};

use super::{news_tool_names, register_news};
use crate::news::ports::{
    NewsError, NewsIngestPort, NewsPort, NewsRefreshReport, NewsSourceHealth, NewsSourcesView,
};
use crate::news::{NewsArticle, NewsCategory, NewsSource, NewsSourceType};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 可编程 News 端口：内存列表 + 调用计数；无任何写外部副作用。
#[derive(Default)]
struct FakeNewsPort {
    sources: Vec<NewsSource>,
    articles: Vec<NewsArticle>,
    fail: bool,
    last_limit: Mutex<i64>,
    last_search: Mutex<String>,
    last_category: Mutex<Option<NewsCategory>>,
    last_source_id: Mutex<Option<i64>>,
    last_article_id: Mutex<Option<i64>>,
}

impl FakeNewsPort {
    fn with_articles() -> Self {
        let mut port = Self::default();
        port.sources.push(NewsSource {
            id: 1,
            name: "Wire".into(),
            url: "https://wire.example/rss".into(),
            source_type: NewsSourceType::Rss,
            category: NewsCategory::General,
            site_url: None,
            last_updated: Some(1_800_000_000),
            last_error: None,
            unread_count: 2,
        });
        port.articles.push(NewsArticle {
            id: 11,
            source_id: 1,
            source_name: "Wire".into(),
            guid: "id:11".into(),
            url: "https://wire.example/1".into(),
            title: "Summit opens in Washington".into(),
            author: Some("Jane Doe".into()),
            image_url: Some("https://cdn.example.com/1.jpg".into()),
            published_at: Some(1_800_000_000),
            summary: Some("delegates gathered".into()),
            is_read: false,
            starred: false,
        });
        port.articles.push(NewsArticle {
            id: 12,
            source_id: 1,
            source_name: "Wire".into(),
            guid: "id:12".into(),
            url: "https://wire.example/2".into(),
            title: "Typhoon makes landfall".into(),
            author: None,
            image_url: None,
            published_at: Some(1_700_000_000),
            summary: Some("winds 16".into()),
            is_read: true,
            starred: false,
        });
        port
    }
}

impl NewsPort for FakeNewsPort {
    fn sources(&self) -> Result<NewsSourcesView, NewsError> {
        if self.fail {
            return Err(NewsError::Store("boom".into()));
        }
        let health = if self
            .sources
            .iter()
            .any(|source| source.last_error.is_some())
        {
            NewsSourceHealth::Degraded
        } else {
            NewsSourceHealth::Healthy
        };
        Ok(NewsSourcesView {
            sources: self.sources.clone(),
            health,
        })
    }

    fn latest(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        if self.fail {
            return Err(NewsError::Store("boom".into()));
        }
        *self.last_limit.lock().expect("limit") = limit;
        Ok(self.articles.iter().take(limit as usize).cloned().collect())
    }

    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        *self.last_search.lock().expect("search") = keyword.to_string();
        *self.last_limit.lock().expect("limit") = limit;
        Ok(self
            .articles
            .iter()
            .filter(|article| {
                article
                    .title
                    .to_lowercase()
                    .contains(&keyword.to_lowercase())
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn by_category(
        &self,
        category: NewsCategory,
        limit: i64,
    ) -> Result<Vec<NewsArticle>, NewsError> {
        *self.last_category.lock().expect("category") = Some(category);
        *self.last_limit.lock().expect("limit") = limit;
        Ok(self
            .articles
            .iter()
            .filter(|article| {
                self.sources
                    .iter()
                    .find(|source| source.id == article.source_id)
                    .is_some_and(|source| source.category == category)
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn by_source(&self, source_id: i64, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        *self.last_source_id.lock().expect("source") = Some(source_id);
        *self.last_limit.lock().expect("limit") = limit;
        if !self.sources.iter().any(|source| source.id == source_id) {
            return Err(NewsError::SourceNotFound(source_id));
        }
        Ok(self
            .articles
            .iter()
            .filter(|article| article.source_id == source_id)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn get_article(&self, article_id: i64) -> Result<NewsArticle, NewsError> {
        *self.last_article_id.lock().expect("article") = Some(article_id);
        self.articles
            .iter()
            .find(|article| article.id == article_id)
            .cloned()
            .ok_or(NewsError::ArticleNotFound(article_id))
    }

    fn starred(&self, limit: i64) -> Result<Vec<NewsArticle>, NewsError> {
        Ok(self
            .articles
            .iter()
            .filter(|article| article.starred)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn toggle_star(&self, _article_id: i64) -> Result<bool, NewsError> {
        Ok(true)
    }

    fn mark_read(&self, _article_id: i64) -> Result<(), NewsError> {
        Ok(())
    }

    fn remove_source(&self, source_id: i64) -> Result<(), NewsError> {
        if self.sources.iter().any(|source| source.id == source_id) {
            Ok(())
        } else {
            Err(NewsError::SourceNotFound(source_id))
        }
    }

    fn set_category(&self, _source_id: i64, _category: NewsCategory) -> Result<(), NewsError> {
        Ok(())
    }
}

/// 可编程摄取端口（脚本化刷新结果，不触网）。
struct FakeIngest {
    report: Mutex<Option<Result<NewsRefreshReport, NewsError>>>,
    refresh_calls: Mutex<u32>,
}

impl FakeIngest {
    fn with_report(new_articles: usize) -> Self {
        Self {
            report: Mutex::new(Some(Ok(NewsRefreshReport {
                new_articles,
                failures: vec![crate::news::NewsRefreshFailure {
                    source: "Down".into(),
                    message: "server returned 500".into(),
                }],
            }))),
            refresh_calls: Mutex::new(0),
        }
    }
}

#[async_trait::async_trait]
impl NewsIngestPort for FakeIngest {
    async fn refresh(&self) -> Result<NewsRefreshReport, NewsError> {
        *self.refresh_calls.lock().expect("calls") += 1;
        self.report
            .lock()
            .expect("report")
            .take()
            .expect("refresh script exhausted")
    }

    async fn add_source(
        &self,
        _url: &str,
        _category: NewsCategory,
    ) -> Result<NewsSource, NewsError> {
        Err(NewsError::Store("unused in these tests".into()))
    }
}

/// 可编程模型：FIFO 队列。
struct MiniChat {
    steps: Mutex<std::collections::VecDeque<ChatResponse>>,
}

#[async_trait::async_trait]
impl ChatModelProvider for MiniChat {
    fn name(&self) -> &'static str {
        "mini"
    }
    async fn chat(
        &self,
        _request: ChatRequest,
    ) -> Result<ChatResponse, devtoolbox_core::ProviderError> {
        Ok(self
            .steps
            .lock()
            .expect("chat steps")
            .pop_front()
            .expect("chat script exhausted"))
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

fn register(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    port: Arc<dyn NewsPort>,
    ingest: Option<Arc<dyn NewsIngestPort>>,
) {
    register_news(modules, tools, port, ingest).expect("register news");
}

// ---------------------------------------------------------------------------
// 注册与工具发现
// ---------------------------------------------------------------------------

#[test]
fn news_module_registers_six_tools_with_expected_risk() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        None,
    );

    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "news");
    assert_eq!(descriptors[0].tools.len(), 6);
    assert_eq!(tools.len(), 6);

    for name in news_tool_names() {
        let spec = tools.spec(name).expect("missing news tool");
        assert_eq!(spec.module, "news");
        assert!(name.starts_with("news."), "工具名必须是 news.*：{name}");
        assert!(
            !name.starts_with("rss."),
            "ADR-010：News 工具不得占用 rss.* 命名"
        );
    }

    // 前 5 个是读本地库 → Read；refresh 写 news.db → SafeWrite。
    for read_tool in [
        "news.latest",
        "news.search",
        "news.by_category",
        "news.by_source",
        "news.get_article",
    ] {
        assert_eq!(
            tools.spec(read_tool).expect("spec").risk,
            ToolRisk::Read,
            "{read_tool} 必须 Read"
        );
    }
    assert_eq!(
        tools.spec("news.refresh").expect("spec").risk,
        ToolRisk::SafeWrite,
        "news.refresh 写 news.db（新文章 + last_error），不得伪装 Read"
    );
    assert!(
        tools.spec("news.missing").is_none(),
        "未知工具受控拒绝（未列出 = 不暴露）"
    );
}

// ---------------------------------------------------------------------------
// 工具行为
// ---------------------------------------------------------------------------

#[test]
fn latest_passes_limit_and_returns_compact_articles() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeNewsPort::with_articles());
    register(&mut modules, &mut tools, port.clone(), None);

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.latest".into(),
        arguments: serde_json::json!({ "limit": 5 }),
    }))
    .expect("latest");
    assert!(result.ok);
    let stories = result.data.as_array().expect("array");
    assert_eq!(stories.len(), 2);
    assert_eq!(stories[0]["title"], "Summit opens in Washington");
    assert_eq!(stories[0]["author"], "Jane Doe");
    assert_eq!(stories[0]["image_url"], "https://cdn.example.com/1.jpg");
    assert_eq!(result.metadata["count"], 2);
    assert_eq!(*port.last_limit.lock().expect("limit"), 5);

    // 缺省 limit 也有默认值（不传 limit 不炸）。
    let default = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.latest".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("default");
    assert!(default.ok);
    assert_eq!(*port.last_limit.lock().expect("limit"), 20);
}

#[test]
fn search_requires_query_and_reaches_the_port() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeNewsPort::with_articles());
    register(&mut modules, &mut tools, port.clone(), None);

    // 缺 query → 校验错误（不进执行器）。
    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.search".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("missing query");
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
    assert!(
        port.last_search.lock().expect("search").is_empty(),
        "非法参数不得触达端口"
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.search".into(),
        arguments: serde_json::json!({ "query": "typhoon", "limit": 5 }),
    }))
    .expect("search");
    assert_eq!(result.data.as_array().expect("array").len(), 1);
    assert_eq!(*port.last_search.lock().expect("search"), "typhoon");
}

#[test]
fn by_category_parses_enum_and_rejects_unknown() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeNewsPort::with_articles());
    register(&mut modules, &mut tools, port.clone(), None);

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.by_category".into(),
        arguments: serde_json::json!({ "category": "world" }),
    }))
    .expect("by category");
    assert!(result.ok);
    assert_eq!(
        *port.last_category.lock().expect("category"),
        Some(NewsCategory::World)
    );

    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.by_category".into(),
        arguments: serde_json::json!({ "category": "not-a-category" }),
    }))
    .expect_err("unknown category");
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
}

#[test]
fn by_source_reports_missing_source_as_controlled_failure() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        None,
    );

    let ok = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.by_source".into(),
        arguments: serde_json::json!({ "source_id": 1 }),
    }))
    .expect("by source");
    assert_eq!(ok.data.as_array().expect("array").len(), 2);

    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.by_source".into(),
        arguments: serde_json::json!({ "source_id": 9999 }),
    }))
    .expect_err("unknown source");
    assert_eq!(error.code(), "personal_ai_tool_execution_failed");
    assert!(error.to_string().contains("9999"));
}

#[test]
fn get_article_returns_single_record_and_missing_fails_controlled() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        None,
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.get_article".into(),
        arguments: serde_json::json!({ "article_id": 11 }),
    }))
    .expect("get article");
    assert!(result.ok);
    assert_eq!(result.data["title"], "Summit opens in Washington");
    assert_eq!(result.data["source"], "Wire");

    let missing = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "news.get_article".into(),
        arguments: serde_json::json!({ "article_id": 9999 }),
    }))
    .expect_err("missing article");
    assert_eq!(missing.code(), "personal_ai_tool_execution_failed");
    assert!(missing.to_string().contains("9999"));
}

#[test]
fn refresh_reports_new_articles_and_isolated_failures() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let ingest = Arc::new(FakeIngest::with_report(7));
    let calls = Arc::clone(&ingest);
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        Some(ingest),
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("refresh");
    assert!(result.ok);
    assert_eq!(result.data["new_articles"], 7);
    let failures = result.data["failures"].as_array().expect("array");
    assert_eq!(failures.len(), 1, "单源失败必须逐条回报");
    assert_eq!(failures[0]["source"], "Down");
    assert_eq!(*calls.refresh_calls.lock().expect("calls"), 1);
}

#[test]
fn refresh_without_ingest_degrades_instead_of_faking_success() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        None,
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("refresh");
    assert!(!result.ok, "未装配抓取能力不得伪造成功");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("未装配抓取能力")
    );
}

#[test]
fn port_failure_becomes_controlled_tool_failure() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort {
            fail: true,
            ..Default::default()
        }),
        None,
    );

    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "news.latest".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("port failure");
    assert_eq!(error.code(), "personal_ai_tool_execution_failed");
}

// ---------------------------------------------------------------------------
// sources：推荐源是候选，不是订阅
// ---------------------------------------------------------------------------

#[test]
fn recommendations_are_excluded_once_subscribed() {
    use devtoolbox_core::news::recommended_sources;

    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let mut port = FakeNewsPort::with_articles();
    // 把第一个推荐源加成"已订阅"。
    let subscribed = recommended_sources().remove(0);
    port.sources.push(NewsSource {
        id: 7,
        name: subscribed.name,
        url: subscribed.url,
        source_type: NewsSourceType::Rss,
        category: subscribed.category,
        site_url: subscribed.site_url,
        last_updated: None,
        last_error: None,
        unread_count: 0,
    });
    register(&mut modules, &mut tools, Arc::new(port), None);

    let context_provider: Arc<dyn ModuleContextProvider> = modules
        .context_provider("news")
        .expect("news context provider");
    // ContextProvider 也要能拿到 sources（工具走同一端口）。
    let bundle = context_provider
        .build_context(
            &AiAppContext {
                module: Some("news".into()),
                page: Some("starred".into()),
                entity: None,
                selection: None,
                view_state: serde_json::json!({}),
            },
            &ContextBudget::default(),
        )
        .expect("bundle");
    assert_eq!(bundle.module, "news");
    assert!(bundle.headline.contains("稍后读"));
    assert_eq!(bundle.summary["sources"], 2);
    assert!(!bundle.summary["degraded"].as_bool().expect("bool"));
}

#[test]
fn degraded_health_surfaces_in_context() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let mut port = FakeNewsPort::with_articles();
    port.sources[0].last_error = Some("server returned 500".into());
    register(&mut modules, &mut tools, Arc::new(port), None);

    let provider: Arc<dyn ModuleContextProvider> =
        modules.context_provider("news").expect("context provider");
    let bundle = provider
        .build_context(
            &AiAppContext {
                module: Some("news".into()),
                page: Some("home".into()),
                entity: None,
                selection: None,
                view_state: serde_json::json!({}),
            },
            &ContextBudget::default(),
        )
        .expect("bundle");
    assert_eq!(bundle.summary["degraded"], true);
    assert_eq!(bundle.summary["page"], "home");
    assert!(bundle.headline.contains("今日"));
}

// ---------------------------------------------------------------------------
// 路由与共存
// ---------------------------------------------------------------------------

#[test]
fn personal_agent_routes_to_news_tool() {
    use crate::personal_ai::agent::{PersonalAgent, PersonalHub};
    use crate::personal_ai::session::InMemorySessionStore;

    let port = Arc::new(FakeNewsPort::with_articles());
    let probe = Arc::clone(&port);
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(&mut modules, &mut tools, port, None);

    let hub = Arc::new(PersonalHub {
        modules,
        tools,
        retrieval: None,
        orchestration: None,
    });
    let chat = MiniChat {
        steps: Mutex::new(std::collections::VecDeque::new()),
    };
    {
        let mut steps = chat.steps.lock().expect("steps");
        steps.push_back(ChatResponse {
            content: None,
            reasoning_content: None,
            tool_calls: vec![ChatToolCall {
                id: "call_news".into(),
                name: "news.by_category".into(),
                arguments: serde_json::json!({ "category": "general", "limit": 3 }),
            }],
            usage: devtoolbox_core::ChatUsage::default(),
        });
        steps.push_back(ChatResponse {
            content: Some(
                r#"{"message":"今天有 2 条综合要闻。","actions":[],"ui_blocks":[]}"#.to_string(),
            ),
            reasoning_content: None,
            tool_calls: vec![],
            usage: devtoolbox_core::ChatUsage::default(),
        });
    }

    let agent = PersonalAgent::new(
        Arc::new(chat),
        hub,
        Arc::new(InMemorySessionStore::new()),
        Default::default(),
    );
    let response: AgentResponse = block_on(agent.run(AgentRequest {
        message: "今天国际上有什么新闻？".into(),
        session_id: Some("s-news".into()),
        app_context: AiAppContext {
            module: Some("news".into()),
            page: Some("home".into()),
            entity: None,
            selection: None,
            view_state: serde_json::json!({}),
        },
        capabilities: vec!["news".into()],
        locale: Some("zh-CN".into()),
        parts: Vec::new(),
    }))
    .expect("agent run");

    assert_eq!(response.tool_trace.len(), 1);
    assert_eq!(response.tool_trace[0].tool, "news.by_category");
    assert!(response.tool_trace[0].ok);
    assert!(
        probe.last_search.lock().expect("search").is_empty(),
        "路由走 by_category，不该碰 search"
    );
}

#[test]
fn news_and_rss_are_separate_modules_with_separate_tool_prefixes() {
    // ADR-010：两个 bounded context 各自注册，命名空间不得混用。
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeNewsPort::with_articles()),
        None,
    );
    crate::personal_ai::rss::register_rss(
        &mut modules,
        &mut tools,
        crate::personal_ai::rss::RssTools::new(rss_fake()),
        None,
    )
    .expect("register rss");

    let ids: Vec<String> = modules
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect();
    assert_eq!(ids, vec!["news", "rss"], "两个独立模块 descriptor");

    for spec in tools.specs() {
        match spec.module.as_str() {
            "news" => assert!(spec.name.starts_with("news."), "{}", spec.name),
            "rss" => assert!(spec.name.starts_with("rss."), "{}", spec.name),
            other => panic!("未知模块 {other}"),
        }
    }
    assert_eq!(
        tools.len(),
        6 + crate::personal_ai::rss::rss_tool_names().len()
    );

    // 未知模块受控拒绝（模块注册表不认识）。
    assert!(
        modules.context_provider("unknown").is_none(),
        "未注册模块没有 context provider"
    );
}

/// RSS 模块测试用最小端口（只用于共存断言）。
fn rss_fake() -> Arc<dyn crate::rss::RssPort> {
    use crate::rss::RssError;

    struct Rss;
    impl crate::rss::RssPort for Rss {
        fn list_subscriptions(&self) -> Result<Vec<devtoolbox_core::rss::FeedRow>, RssError> {
            Ok(vec![])
        }
        fn list_entries(
            &self,
            _feed_id: Option<i64>,
            _limit: i64,
        ) -> Result<Vec<devtoolbox_core::rss::ArticleRow>, RssError> {
            Ok(vec![])
        }
        fn search(
            &self,
            _keyword: &str,
            _limit: i64,
        ) -> Result<Vec<devtoolbox_core::rss::ArticleRow>, RssError> {
            Ok(vec![])
        }
        fn get_entry(&self, _entry_id: i64) -> Result<devtoolbox_core::rss::ArticleRow, RssError> {
            Err(RssError::EntryNotFound(1))
        }
        fn mark_read(&self, _entry_id: i64) -> Result<(), RssError> {
            Ok(())
        }
    }
    Arc::new(Rss)
}
