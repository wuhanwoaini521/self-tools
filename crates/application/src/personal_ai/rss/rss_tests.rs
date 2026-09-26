//! RSS 模块集成测试（V12 / ADR-010；Fake 全依赖，不触网、不触 SQLite）。
//!
//! 覆盖：模块注册与工具发现 / 风险面 / 订阅清单 / 条目列表（跨源与单源）/
//! 检索与空关键词 / 详情与缺失 / 标已读 / 刷新（装配与未装配）/
//! ContextProvider / 与 News 模块共存。

use std::sync::{Arc, Mutex};

use devtoolbox_core::ToolCallRequest;
use devtoolbox_core::ToolRisk;
use devtoolbox_core::rss::{ArticleRow, FeedRow};

use super::{RssTools, register_rss, rss_tool_names};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};
use crate::rss::service::{RssError, RssIngestPort, RssPort};
use crate::rss::workflows::RefreshReport;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeRssPort {
    feeds: Vec<FeedRow>,
    entries: Vec<ArticleRow>,
    fail: bool,
    last_feed_id: Mutex<Option<i64>>,
    last_search: Mutex<String>,
    last_read: Mutex<Option<i64>>,
}

impl FakeRssPort {
    fn with_entries() -> Self {
        let mut port = Self::default();
        port.feeds.push(FeedRow {
            id: 1,
            title: "V2EX".into(),
            url: "https://v2ex.com/feed.xml".into(),
            site_url: Some("https://v2ex.com/".into()),
            last_updated: Some(1_800_000_000),
            last_error: None,
            unread_count: 2,
        });
        port.entries.push(ArticleRow {
            id: 11,
            feed_id: 1,
            feed_title: "V2EX".into(),
            guid: "id:11".into(),
            url: "https://v2ex.com/t/1".into(),
            title: "[程序员] 一号员工满月了".into(),
            author: Some("olddogs".into()),
            image_url: None,
            published_at: Some(1_800_000_000),
            summary: Some("工作台复盘".into()),
            is_read: false,
            starred: false,
        });
        port.entries.push(ArticleRow {
            id: 12,
            feed_id: 1,
            feed_title: "V2EX".into(),
            guid: "id:12".into(),
            url: "https://v2ex.com/t/2".into(),
            title: "[问与答] 美区 paypal 解封".into(),
            author: Some("94nb".into()),
            image_url: None,
            published_at: Some(1_700_000_000),
            summary: Some("扫码上传身份证".into()),
            is_read: true,
            starred: false,
        });
        port
    }
}

impl RssPort for FakeRssPort {
    fn list_subscriptions(&self) -> Result<Vec<FeedRow>, RssError> {
        if self.fail {
            return Err(RssError::Store("boom".into()));
        }
        Ok(self.feeds.clone())
    }

    fn list_entries(&self, feed_id: Option<i64>, limit: i64) -> Result<Vec<ArticleRow>, RssError> {
        if self.fail {
            return Err(RssError::Store("boom".into()));
        }
        *self.last_feed_id.lock().expect("feed_id") = feed_id;
        if let Some(feed_id) = feed_id {
            if !self.feeds.iter().any(|feed| feed.id == feed_id) {
                return Err(RssError::FeedNotFound(feed_id));
            }
            return Ok(self
                .entries
                .iter()
                .filter(|entry| entry.feed_id == feed_id)
                .take(limit as usize)
                .cloned()
                .collect());
        }
        Ok(self.entries.iter().take(limit as usize).cloned().collect())
    }

    fn search(&self, keyword: &str, limit: i64) -> Result<Vec<ArticleRow>, RssError> {
        *self.last_search.lock().expect("search") = keyword.to_string();
        Ok(self
            .entries
            .iter()
            .filter(|entry| entry.title.to_lowercase().contains(&keyword.to_lowercase()))
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn get_entry(&self, entry_id: i64) -> Result<ArticleRow, RssError> {
        self.entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .cloned()
            .ok_or(RssError::EntryNotFound(entry_id))
    }

    fn mark_read(&self, entry_id: i64) -> Result<(), RssError> {
        *self.last_read.lock().expect("read") = Some(entry_id);
        if self.entries.iter().any(|entry| entry.id == entry_id) {
            Ok(())
        } else {
            Err(RssError::EntryNotFound(entry_id))
        }
    }
}

struct FakeIngest {
    calls: Mutex<u32>,
}

impl FakeIngest {
    fn new() -> Self {
        Self {
            calls: Mutex::new(0),
        }
    }
}

#[async_trait::async_trait]
impl RssIngestPort for FakeIngest {
    async fn refresh(&self) -> Result<RefreshReport, RssError> {
        *self.calls.lock().expect("calls") += 1;
        Ok(RefreshReport {
            new_articles: 3,
            failures: vec![crate::rss::RefreshFailure {
                feed_title: "Down".into(),
                message: "server returned 500".into(),
            }],
        })
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
    port: Arc<dyn RssPort>,
    ingest: Option<Arc<dyn RssIngestPort>>,
) {
    register_rss(modules, tools, RssTools::new(port), ingest).expect("register rss");
}

// ---------------------------------------------------------------------------
// 注册与工具发现
// ---------------------------------------------------------------------------

#[test]
fn rss_module_registers_six_tools_with_expected_risk() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeRssPort::with_entries()),
        None,
    );

    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "rss");
    assert_eq!(descriptors[0].tools.len(), 6);
    assert_eq!(tools.len(), 6);

    for name in rss_tool_names() {
        let spec = tools.spec(name).expect("missing rss tool");
        assert_eq!(spec.module, "rss");
        assert!(name.starts_with("rss."), "工具名必须是 rss.*：{name}");
    }

    for read_tool in [
        "rss.list_subscriptions",
        "rss.list_entries",
        "rss.search",
        "rss.get_entry",
    ] {
        assert_eq!(
            tools.spec(read_tool).expect("spec").risk,
            ToolRisk::Read,
            "{read_tool} 必须 Read"
        );
    }
    for write_tool in ["rss.mark_read", "rss.refresh"] {
        assert_eq!(
            tools.spec(write_tool).expect("spec").risk,
            ToolRisk::SafeWrite,
            "{write_tool} 写 dashboard.db，不得伪装 Read"
        );
    }
    assert!(tools.spec("rss.missing").is_none());
}

// ---------------------------------------------------------------------------
// 工具行为
// ---------------------------------------------------------------------------

#[test]
fn list_subscriptions_reports_unread_and_health() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeRssPort::with_entries()),
        None,
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.list_subscriptions".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("list subscriptions");
    assert!(result.ok);
    let feeds = result.data.as_array().expect("array");
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["title"], "V2EX");
    assert_eq!(feeds[0]["unread_count"], 2);
    assert_eq!(result.metadata["count"], 1);
}

#[test]
fn list_entries_crosses_subscriptions_and_scopes_by_feed() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeRssPort::with_entries());
    register(&mut modules, &mut tools, port.clone(), None);

    let all = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.list_entries".into(),
        arguments: serde_json::json!({ "limit": 10 }),
    }))
    .expect("all");
    assert_eq!(all.data.as_array().expect("array").len(), 2);
    assert_eq!(*port.last_feed_id.lock().expect("feed_id"), None);
    assert_eq!(
        all.data.as_array().expect("array")[0]["title"],
        "[程序员] 一号员工满月了"
    );

    let scoped = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "rss.list_entries".into(),
        arguments: serde_json::json!({ "feed_id": 1 }),
    }))
    .expect("scoped");
    assert_eq!(scoped.data.as_array().expect("array").len(), 2);

    let missing = block_on(tools.execute(&ToolCallRequest {
        id: "c3".into(),
        name: "rss.list_entries".into(),
        arguments: serde_json::json!({ "feed_id": 9999 }),
    }))
    .expect_err("unknown feed");
    assert_eq!(missing.code(), "personal_ai_tool_execution_failed");
}

#[test]
fn search_requires_query_and_reaches_port() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeRssPort::with_entries());
    register(&mut modules, &mut tools, port.clone(), None);

    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.search".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("missing query");
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
    assert!(port.last_search.lock().expect("search").is_empty());

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "rss.search".into(),
        arguments: serde_json::json!({ "query": "paypal" }),
    }))
    .expect("search");
    assert_eq!(result.data.as_array().expect("array").len(), 1);
    assert_eq!(*port.last_search.lock().expect("search"), "paypal");
}

#[test]
fn get_entry_and_mark_read_round_trip() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = Arc::new(FakeRssPort::with_entries());
    register(&mut modules, &mut tools, port.clone(), None);

    let detail = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.get_entry".into(),
        arguments: serde_json::json!({ "entry_id": 11 }),
    }))
    .expect("get entry");
    assert!(detail.ok);
    assert_eq!(detail.data["subscription"], "V2EX");
    assert_eq!(detail.data["author"], "olddogs");
    assert!(!detail.data["is_read"].as_bool().expect("bool"));

    let missing = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "rss.get_entry".into(),
        arguments: serde_json::json!({ "entry_id": 9999 }),
    }))
    .expect_err("missing entry");
    assert_eq!(missing.code(), "personal_ai_tool_execution_failed");

    let read = block_on(tools.execute(&ToolCallRequest {
        id: "c3".into(),
        name: "rss.mark_read".into(),
        arguments: serde_json::json!({ "entry_id": 11 }),
    }))
    .expect("mark read");
    assert!(read.ok);
    assert_eq!(read.data["is_read"], true);
    assert_eq!(*port.last_read.lock().expect("read"), Some(11));
}

#[test]
fn refresh_reports_new_articles_and_isolated_failures() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let ingest = Arc::new(FakeIngest::new());
    let calls = Arc::clone(&ingest);
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeRssPort::with_entries()),
        Some(ingest),
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("refresh");
    assert!(result.ok);
    assert_eq!(result.data["new_articles"], 3);
    let failures = result.data["failures"].as_array().expect("array");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["source"], "Down");
    assert_eq!(*calls.calls.lock().expect("calls"), 1);
}

#[test]
fn refresh_without_ingest_degrades_instead_of_faking_success() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeRssPort::with_entries()),
        None,
    );

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.refresh".into(),
        arguments: serde_json::json!({}),
    }))
    .expect("refresh");
    assert!(!result.ok);
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
    let port = FakeRssPort {
        fail: true,
        ..FakeRssPort::default()
    };
    register(&mut modules, &mut tools, Arc::new(port), None);

    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "rss.list_subscriptions".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("port failure");
    assert_eq!(error.code(), "personal_ai_tool_execution_failed");
}

#[test]
fn context_provider_reports_subscriptions_and_unread() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register(
        &mut modules,
        &mut tools,
        Arc::new(FakeRssPort::with_entries()),
        None,
    );

    let provider: Arc<dyn ModuleContextProvider> = modules
        .context_provider("rss")
        .expect("rss context provider");
    let bundle = provider
        .build_context(
            &devtoolbox_core::personal_ai::AppContext {
                module: Some("rss".into()),
                page: Some("reader".into()),
                entity: None,
                selection: None,
                view_state: serde_json::json!({}),
            },
            &ContextBudget::default(),
        )
        .expect("bundle");
    assert_eq!(bundle.module, "rss");
    assert!(bundle.headline.contains("阅读"));
    assert_eq!(bundle.summary["subscriptions"], 1);
    assert_eq!(bundle.summary["unread_total"], 2);
    assert_eq!(bundle.summary["degraded"], false);
}
