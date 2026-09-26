//! News 模块适配器（V12 / ADR-010：News 是独立 bounded context）。
//!
//! 注册 descriptor + 6 个工具 + ContextProvider。**PersonalAgent 不含任何
//! news 业务分支**：全部数据经 `NewsPort`（desktop/MCP 组合根装配
//! `NewsService` → `NewsRepositoryPort` → `config/news.db`）。
//!
//! 工具面（与 RSS 的 `rss.*` 完全分开，ADR-010）：
//!
//! - `news.latest(limit?)` → 今日最新（跨源，发布时间倒序）
//! - `news.search(query, limit?)` → 关键词检索（读本地 news.db）
//! - `news.by_category(category, limit?)` → 按分类（general/tech/finance/world/china）
//! - `news.by_source(source_id, limit?)` → 按新闻源
//! - `news.get_article(article_id)` → 新闻详情
//! - `news.refresh()` → 立即刷新（联网；`SafeWrite`，未装配时如实降级）
//!
//! 边界：**不碰 RSS 订阅**。看「我关注的博客今天有什么更新」走 `rss.*`，
//! 看「今天 AI 有什么重要新闻」走本模块。
//!
//! 风险分级：前 5 个 `Read`（本地库读）；`news.refresh` 写 `news.db`
//! （新文章 + 各源 `last_error`）→ `SafeWrite`，由 registry 门禁放行。

use std::sync::Arc;

use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::news::ports::{NewsError, NewsIngestPort, NewsPort, NewsSourceHealth};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_LATEST: &str = "news.latest";
const TOOL_SEARCH: &str = "news.search";
const TOOL_BY_CATEGORY: &str = "news.by_category";
const TOOL_BY_SOURCE: &str = "news.by_source";
const TOOL_GET_ARTICLE: &str = "news.get_article";
const TOOL_REFRESH: &str = "news.refresh";

const TOOL_NAMES: [&str; 6] = [
    TOOL_LATEST,
    TOOL_SEARCH,
    TOOL_BY_CATEGORY,
    TOOL_BY_SOURCE,
    TOOL_GET_ARTICLE,
    TOOL_REFRESH,
];

/// News 工具执行器（一个结构体、六个身份；dispatch 属模块内部实现细节）。
pub struct NewsTools {
    port: Arc<dyn NewsPort>,
    /// 抓取能力（`news.refresh`）；未装配（如纯读 MCP 入口）时工具如实降级。
    ingest: Option<Arc<dyn NewsIngestPort>>,
}

impl NewsTools {
    #[must_use]
    pub fn new(port: Arc<dyn NewsPort>, ingest: Option<Arc<dyn NewsIngestPort>>) -> Self {
        Self { port, ingest }
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let (input_schema, description, risk) = match name {
            TOOL_SEARCH => (
                serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {"type": "string", "description": "关键词，匹配标题/署名/摘要"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "在已抓取的新闻中检索关键词（读本地 news.db，不联网）",
                ToolRisk::Read,
            ),
            TOOL_BY_CATEGORY => (
                serde_json::json!({
                    "type": "object",
                    "required": ["category"],
                    "properties": {
                        "category": {
                            "type": "string",
                            "enum": ["general", "tech", "finance", "world", "china"],
                            "description": "新闻分类"
                        },
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "按分类取最新新闻（综合/科技/财经/国际/中国）",
                ToolRisk::Read,
            ),
            TOOL_BY_SOURCE => (
                serde_json::json!({
                    "type": "object",
                    "required": ["source_id"],
                    "properties": {
                        "source_id": {"type": "integer", "description": "新闻源 id（由 news.sources 列出）"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "按新闻源取最新新闻",
                ToolRisk::Read,
            ),
            TOOL_GET_ARTICLE => (
                serde_json::json!({
                    "type": "object",
                    "required": ["article_id"],
                    "properties": {
                        "article_id": {"type": "integer"}
                    }
                }),
                "读取单条新闻详情（标题/作者/来源/发布时间/正文摘要/原文链接）",
                ToolRisk::Read,
            ),
            TOOL_REFRESH => (
                serde_json::json!({"type": "object"}),
                "立即刷新全部新闻源（联网抓取；单源失败不影响其它源）",
                ToolRisk::SafeWrite,
            ),
            _ => (
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                    }
                }),
                "今日最新新闻（跨源，按发布时间倒序）。未抓取时如实返回空，不编造",
                ToolRisk::Read,
            ),
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk,
            module: "news".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 薄执行器
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    tools: Arc<NewsTools>,
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
            TOOL_LATEST => self.tools.latest(&arguments),
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_BY_CATEGORY => self.tools.by_category(&arguments),
            TOOL_BY_SOURCE => self.tools.by_source(&arguments),
            TOOL_GET_ARTICLE => self.tools.get_article(&arguments),
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

fn int_arg(arguments: &serde_json::Value, key: &str, tool: &str) -> Result<i64, AgentError> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| AgentError::tool_invalid_argument(format!("{tool}: {key} is required")))
}

fn string_arg<'a>(
    arguments: &'a serde_json::Value,
    key: &str,
    tool: &str,
) -> Result<&'a str, AgentError> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AgentError::tool_invalid_argument(format!("{tool}: {key} is required")))
}

/// `NewsError` → 工具失败。消息来自端口（稳定、可读），不含正文。
fn port_failure(tool: &str, error: &NewsError) -> AgentError {
    AgentError::tool_execution_failed(format!("{tool}: {error}"))
}

/// 一条新闻 → JSON（工具返回与 Context 共用形状）。
fn article_json(article: &devtoolbox_core::news::NewsArticle) -> serde_json::Value {
    serde_json::json!({
        "id": article.id,
        "source_id": article.source_id,
        "source": article.source_name,
        "title": article.title,
        "url": article.url,
        "author": article.author,
        "image_url": article.image_url,
        "published_at": article.published_at,
        "summary": article.summary,
        "is_read": article.is_read,
        "starred": article.starred,
    })
}

fn articles_result(
    tool: &str,
    articles: &[devtoolbox_core::news::NewsArticle],
    note: &str,
) -> ToolResult {
    let list: Vec<serde_json::Value> = articles.iter().map(article_json).collect();
    let count = list.len();
    ToolResult::ok_with_metadata(
        serde_json::Value::Array(list),
        serde_json::json!({ "count": count, "note": note, "tool": tool }),
    )
}

impl NewsTools {
    fn latest(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let limit = limit_or(arguments, 20);
        let articles = self
            .port
            .latest(limit)
            .map_err(|error| port_failure(TOOL_LATEST, &error))?;
        let note = if articles.is_empty() {
            "暂无新闻：新闻源尚未刷新（可调用 news.refresh）"
        } else {
            "按发布时间倒序；用 article_id 调 news.get_article 读详情"
        };
        Ok(articles_result(TOOL_LATEST, &articles, note))
    }

    fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = string_arg(arguments, "query", TOOL_SEARCH)?;
        let limit = limit_or(arguments, 20);
        let articles = self
            .port
            .search(query, limit)
            .map_err(|error| port_failure(TOOL_SEARCH, &error))?;
        let note = if articles.is_empty() {
            "本地已抓取的新闻里没有命中该关键词"
        } else {
            "命中按发布时间倒序"
        };
        Ok(articles_result(TOOL_SEARCH, &articles, note))
    }

    fn by_category(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let raw = string_arg(arguments, "category", TOOL_BY_CATEGORY)?;
        let category = devtoolbox_core::news::NewsCategory::from_id(raw).ok_or_else(|| {
            AgentError::tool_invalid_argument(format!(
                "{TOOL_BY_CATEGORY}: unknown category `{raw}`"
            ))
        })?;
        let limit = limit_or(arguments, 20);
        let articles = self
            .port
            .by_category(category, limit)
            .map_err(|error| port_failure(TOOL_BY_CATEGORY, &error))?;
        Ok(articles_result(
            TOOL_BY_CATEGORY,
            &articles,
            &format!("分类 {}", category.label()),
        ))
    }

    fn by_source(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let source_id = int_arg(arguments, "source_id", TOOL_BY_SOURCE)?;
        let limit = limit_or(arguments, 20);
        let articles = self
            .port
            .by_source(source_id, limit)
            .map_err(|error| port_failure(TOOL_BY_SOURCE, &error))?;
        Ok(articles_result(TOOL_BY_SOURCE, &articles, "按源最新"))
    }

    fn get_article(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let article_id = int_arg(arguments, "article_id", TOOL_GET_ARTICLE)?;
        let article = self
            .port
            .get_article(article_id)
            .map_err(|error| port_failure(TOOL_GET_ARTICLE, &error))?;
        Ok(ToolResult::ok(article_json(&article)))
    }

    /// 立即刷新（联网）。未装配抓取能力时如实降级，不伪造成功。
    async fn refresh(&self) -> Result<ToolResult, AgentError> {
        let Some(ingest) = self.ingest.as_ref() else {
            return Ok(ToolResult::fail(
                "news refresh 未装配抓取能力（当前入口不提供联网抓取）",
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
                        "source": failure.source,
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

/// News 模块上下文提供方：AppContext{page} → 源与未读概览。
pub struct NewsProviderOwned {
    tools: Arc<NewsTools>,
}

impl ModuleContextProvider for NewsProviderOwned {
    fn module_id(&self) -> &str {
        "news"
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
            "starred" => "News · 稍后读".to_string(),
            "sources" => "News · 新闻源".to_string(),
            "reader" => "News · 阅读".to_string(),
            _ => "News · 今日".to_string(),
        };
        // 上下文只给结构概览；不预拉正文（token 预算归工具调用）。
        let view = self
            .tools
            .port
            .sources()
            .map_err(|error| AgentError::context(format!("news context: {error}")))?;
        let summary = serde_json::json!({
            "page": page,
            "sources": view.sources.len(),
            "unread_total": view
                .sources
                .iter()
                .map(|source| source.unread_count)
                .sum::<i64>(),
            "degraded": matches!(view.health, NewsSourceHealth::Degraded),
            "note": "需要具体新闻时调用 news.latest / news.search / news.by_category",
        });
        Ok(ContextBundle {
            module: "news".to_string(),
            headline,
            summary,
        })
    }
}

// ---------------------------------------------------------------------------
// 注册
// ---------------------------------------------------------------------------

/// 注册 News 模块（descriptor + 6 工具 + context provider）。
///
/// `ingest` 可缺省：`news.refresh` 在未装配抓取能力的入口（如纯读 MCP）
/// 会如实返回降级，而不是伪造成功。
pub fn register_news(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    port: Arc<dyn NewsPort>,
    ingest: Option<Arc<dyn NewsIngestPort>>,
) -> Result<(), AgentError> {
    let news = Arc::new(NewsTools::new(port, ingest));
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "news".into(),
            display_name: "News".into(),
            description: "新闻发现与阅读：最新、分类、按源、检索、详情、刷新".into(),
            capabilities: vec![
                "news".into(),
                "latest".into(),
                "search".into(),
                "category".into(),
                "source".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(NewsProviderOwned {
            tools: Arc::clone(&news),
        })),
    })?;
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&news),
            spec_cache: std::sync::OnceLock::new(),
        }))?;
    }
    Ok(())
}

/// 导出工具名（外部测试 / 组合根引用）。
#[must_use]
pub fn news_tool_names() -> [&'static str; 6] {
    TOOL_NAMES
}

#[cfg(test)]
mod news_tests;
