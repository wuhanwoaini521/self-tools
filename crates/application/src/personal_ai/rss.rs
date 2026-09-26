//! RSS 模块适配器（V12 / ADR-010：**个人订阅阅读器** 的 AI 面）。
//!
//! 注册 descriptor + 6 个工具 + ContextProvider。**PersonalAgent 不含任何
//! rss 业务分支**：全部数据经 `RssPort`（desktop/MCP 组合根装配
//! `RssService` → `RssRepositoryPort` → `config/dashboard.db`）。
//!
//! 工具面（与 News 的 `news.*` 完全分开，ADR-010）：
//!
//! - `rss.list_subscriptions()` → 我订阅了什么（含未读数与抓取状态）
//! - `rss.list_entries(feed_id?, limit?)` → 条目列表（跨订阅或单订阅）
//! - `rss.search(query, limit?)` → 关键词检索（读本地 dashboard.db）
//! - `rss.get_entry(entry_id)` → 单条条目详情
//! - `rss.mark_read(entry_id)` → 标已读（`SafeWrite`，本地状态往返）
//! - `rss.refresh()` → 立即刷新全部订阅（联网；`SafeWrite`）
//!
//! 场景区分：「看看我关注的博客今天有什么更新」→ `rss.*`；
//! 「今天 AI 有什么重要新闻」→ `news.*`。
//!
//! 风险分级：前 4 个 `Read`；`mark_read` / `refresh` 写 `dashboard.db`
//! → `SafeWrite`（registry 门禁放行）。

use std::sync::Arc;

use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};
use crate::rss::service::{RssError, RssIngestPort, RssPort};

const TOOL_LIST_SUBSCRIPTIONS: &str = "rss.list_subscriptions";
const TOOL_LIST_ENTRIES: &str = "rss.list_entries";
const TOOL_SEARCH: &str = "rss.search";
const TOOL_GET_ENTRY: &str = "rss.get_entry";
const TOOL_MARK_READ: &str = "rss.mark_read";
const TOOL_REFRESH: &str = "rss.refresh";

const TOOL_NAMES: [&str; 6] = [
    TOOL_LIST_SUBSCRIPTIONS,
    TOOL_LIST_ENTRIES,
    TOOL_SEARCH,
    TOOL_GET_ENTRY,
    TOOL_MARK_READ,
    TOOL_REFRESH,
];

/// RSS 工具执行器（一个结构体、六个身份；dispatch 属模块内部实现细节）。
pub struct RssTools {
    port: Arc<dyn RssPort>,
    /// 刷新能力（联网）；未装配（如纯读 MCP 入口）时工具如实降级。
    ingest: Option<Arc<dyn RssIngestPort>>,
}

impl RssTools {
    #[must_use]
    pub fn new(port: Arc<dyn RssPort>) -> Self {
        Self { port, ingest: None }
    }

    /// 装配刷新能力（`rss.refresh` 需要）。组合根调用。
    #[must_use]
    pub fn with_ingest(mut self, ingest: Arc<dyn RssIngestPort>) -> Self {
        self.ingest = Some(ingest);
        self
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let (input_schema, description, risk) = match name {
            TOOL_LIST_ENTRIES => (
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feed_id": {
                            "type": "integer",
                            "description": "可选：限定某个订阅（id 来自 rss.list_subscriptions）；不传 = 跨全部订阅最新"
                        },
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "订阅条目列表（跨订阅最新，或限定单个订阅）",
                ToolRisk::Read,
            ),
            TOOL_SEARCH => (
                serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {"type": "string", "description": "关键词，匹配标题/署名/摘要"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "在已抓取的订阅条目中检索关键词（读本地 dashboard.db，不联网）",
                ToolRisk::Read,
            ),
            TOOL_GET_ENTRY => (
                serde_json::json!({
                    "type": "object",
                    "required": ["entry_id"],
                    "properties": {
                        "entry_id": {"type": "integer"}
                    }
                }),
                "读取单条订阅条目详情（标题/署名/订阅名/时间/摘要/原文链接）",
                ToolRisk::Read,
            ),
            TOOL_MARK_READ => (
                serde_json::json!({
                    "type": "object",
                    "required": ["entry_id"],
                    "properties": {
                        "entry_id": {"type": "integer"}
                    }
                }),
                "把一条订阅条目标为已读（幂等；本地状态，不联网）",
                ToolRisk::SafeWrite,
            ),
            TOOL_REFRESH => (
                serde_json::json!({"type": "object"}),
                "立即刷新全部订阅（联网抓取；单源失败不影响其它订阅）",
                ToolRisk::SafeWrite,
            ),
            _ => (
                serde_json::json!({"type": "object"}),
                "列出我订阅了哪些源（含未读数与最近抓取状态）",
                ToolRisk::Read,
            ),
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk,
            module: "rss".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 薄执行器
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    tools: Arc<RssTools>,
    spec_cache: std::sync::OnceLock<ToolSpec>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        self.spec_cache
            .get_or_init(|| self.tools.spec_for(self.name))
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_LIST_SUBSCRIPTIONS => self.tools.list_subscriptions(),
            TOOL_LIST_ENTRIES => self.tools.list_entries(&arguments),
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_GET_ENTRY => self.tools.get_entry(&arguments),
            TOOL_MARK_READ => self.tools.mark_read(&arguments),
            _ => self.tools.refresh().await,
        }
    }
}

// ---------------------------------------------------------------------------
// 具体工具逻辑
// ---------------------------------------------------------------------------

fn limit_or(arguments: &serde_json::Value, default: i64) -> i64 {
    arguments
        .get("limit")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(default)
        .clamp(1, 100)
}

fn entry_id_arg(arguments: &serde_json::Value, tool: &str) -> Result<i64, AgentError> {
    arguments
        .get("entry_id")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| AgentError::tool_invalid_argument(format!("{tool}: entry_id is required")))
}

/// `RssError` → 工具失败。消息来自端口（稳定、可读），不含正文。
fn port_failure(tool: &str, error: &RssError) -> AgentError {
    AgentError::tool_execution_failed(format!("{tool}: {error}"))
}

fn subscription_json(feed: &devtoolbox_core::rss::FeedRow) -> serde_json::Value {
    serde_json::json!({
        "id": feed.id,
        "title": feed.title,
        "url": feed.url,
        "site_url": feed.site_url,
        "unread_count": feed.unread_count,
        "last_updated": feed.last_updated,
        "last_error": feed.last_error,
    })
}

fn entry_json(entry: &devtoolbox_core::rss::ArticleRow) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "feed_id": entry.feed_id,
        "subscription": entry.feed_title,
        "title": entry.title,
        "url": entry.url,
        "author": entry.author,
        "image_url": entry.image_url,
        "published_at": entry.published_at,
        "summary": entry.summary,
        "is_read": entry.is_read,
        "starred": entry.starred,
    })
}

impl RssTools {
    fn list_subscriptions(&self) -> Result<ToolResult, AgentError> {
        let feeds = self
            .port
            .list_subscriptions()
            .map_err(|error| port_failure(TOOL_LIST_SUBSCRIPTIONS, &error))?;
        let list: Vec<serde_json::Value> = feeds.iter().map(subscription_json).collect();
        let count = list.len();
        Ok(ToolResult::ok_with_metadata(
            serde_json::Value::Array(list),
            serde_json::json!({ "count": count, "tool": TOOL_LIST_SUBSCRIPTIONS }),
        ))
    }

    fn list_entries(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let feed_id = arguments.get("feed_id").and_then(serde_json::Value::as_i64);
        let limit = limit_or(arguments, 20);
        let entries = self
            .port
            .list_entries(feed_id, limit)
            .map_err(|error| port_failure(TOOL_LIST_ENTRIES, &error))?;
        let list: Vec<serde_json::Value> = entries.iter().map(entry_json).collect();
        let count = list.len();
        let note = if entries.is_empty() {
            "暂无条目：订阅源尚未刷新（可调用 rss.refresh）"
        } else {
            "按发布时间倒序"
        };
        Ok(ToolResult::ok_with_metadata(
            serde_json::Value::Array(list),
            serde_json::json!({ "count": count, "note": note, "tool": TOOL_LIST_ENTRIES }),
        ))
    }

    fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if query.is_empty() {
            return Err(AgentError::tool_invalid_argument(format!(
                "{TOOL_SEARCH}: query is required"
            )));
        }
        let limit = limit_or(arguments, 20);
        let entries = self
            .port
            .search(&query, limit)
            .map_err(|error| port_failure(TOOL_SEARCH, &error))?;
        let list: Vec<serde_json::Value> = entries.iter().map(entry_json).collect();
        let count = list.len();
        let note = if entries.is_empty() {
            "本地已抓取的条目里没有命中该关键词"
        } else {
            "命中按发布时间倒序"
        };
        Ok(ToolResult::ok_with_metadata(
            serde_json::Value::Array(list),
            serde_json::json!({ "count": count, "note": note, "tool": TOOL_SEARCH }),
        ))
    }

    fn get_entry(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let entry_id = entry_id_arg(arguments, TOOL_GET_ENTRY)?;
        let entry = self
            .port
            .get_entry(entry_id)
            .map_err(|error| port_failure(TOOL_GET_ENTRY, &error))?;
        Ok(ToolResult::ok(entry_json(&entry)))
    }

    fn mark_read(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let entry_id = entry_id_arg(arguments, TOOL_MARK_READ)?;
        self.port
            .mark_read(entry_id)
            .map_err(|error| port_failure(TOOL_MARK_READ, &error))?;
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({ "entry_id": entry_id, "is_read": true }),
            serde_json::json!({ "tool": TOOL_MARK_READ }),
        ))
    }

    /// 立即刷新全部订阅（联网）。未装配抓取能力时如实降级。
    async fn refresh(&self) -> Result<ToolResult, AgentError> {
        let Some(ingest) = self.ingest.as_ref() else {
            return Ok(ToolResult::fail(
                "rss refresh 未装配抓取能力（当前入口不提供联网抓取）",
            ));
        };
        let report = ingest
            .refresh()
            .await
            .map_err(|error| port_failure(TOOL_REFRESH, &error))?;
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "new_articles": report.new_articles,
                "failures": report
                    .failures
                    .iter()
                    .map(|failure| serde_json::json!({
                        "source": failure.feed_title,
                        "message": failure.message,
                    }))
                    .collect::<Vec<_>>(),
            }),
            serde_json::json!({ "tool": TOOL_REFRESH }),
        ))
    }
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// RSS 模块上下文提供方：AppContext{page} → 订阅与未读概览。
pub struct RssProviderOwned {
    tools: Arc<RssTools>,
}

impl ModuleContextProvider for RssProviderOwned {
    fn module_id(&self) -> &str {
        "rss"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let page = app_context
            .page
            .clone()
            .unwrap_or_else(|| "home".to_string());
        let headline = match page.as_str() {
            "reader" => "RSS · 阅读".to_string(),
            _ => "RSS · 订阅".to_string(),
        };
        // 上下文只给结构概览；不预拉条目正文（token 预算归工具调用）。
        let feeds = self
            .tools
            .port
            .list_subscriptions()
            .map_err(|error| AgentError::context(format!("rss context: {error}")))?;
        let summary = serde_json::json!({
            "page": page,
            "subscriptions": feeds.len(),
            "unread_total": feeds.iter().map(|feed| feed.unread_count).sum::<i64>(),
            "degraded": feeds.iter().any(|feed| feed.last_error.is_some()),
            "note": "需要具体条目时调用 rss.list_entries / rss.search",
        });
        Ok(ContextBundle {
            module: "rss".to_string(),
            headline,
            summary,
        })
    }
}

// ---------------------------------------------------------------------------
// 注册
// ---------------------------------------------------------------------------

/// 注册 RSS 模块（descriptor + 6 工具 + context provider）。
///
/// `ingest` 可缺省：`rss.refresh` 在未装配抓取能力的入口会如实降级。
pub fn register_rss(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    mut rss_tools: RssTools,
    ingest: Option<Arc<dyn RssIngestPort>>,
) -> Result<(), AgentError> {
    if let Some(ingest) = ingest {
        rss_tools = rss_tools.with_ingest(ingest);
    }
    let rss = Arc::new(rss_tools);
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "rss".into(),
            display_name: "RSS".into(),
            description: "个人订阅阅读器：订阅清单、条目、检索、详情、已读、刷新".into(),
            capabilities: vec![
                "subscriptions".into(),
                "entries".into(),
                "search".into(),
                "read_state".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(RssProviderOwned {
            tools: Arc::clone(&rss),
        })),
    })?;
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&rss),
            spec_cache: std::sync::OnceLock::new(),
        }))?;
    }
    Ok(())
}

/// 导出工具名（外部测试 / 组合根引用）。
#[must_use]
pub fn rss_tool_names() -> [&'static str; 6] {
    TOOL_NAMES
}

#[cfg(test)]
mod rss_tests;
