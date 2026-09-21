//! Travel 模块适配器（V5 §35-§44，Gate 5）。
//!
//! Travel 是 V5 第二个标准 Personal AI 模块：注册 descriptor + 4 个 Read 工具 +
//! ContextProvider。**不重新设计 Travel domain**：全部数据经
//! `TravelAiPort`（组合根把既有 SearchProvider / TravelStore 缓存装配进来）。
//!
//! - `travel.search_destination(query, limit?)` → 精简目的地命中
//! - `travel.get_destination(city)` → 已缓存目的地速览
//! - `travel.get_trip_context(city)` → 当前行程上下文（供「第二天太累了」等指代场景）
//! - `travel.plan_trip(city, days?)` → 规划预览（默认不写永久数据，V5 §43）
//!
//! 全部 `ToolRisk::Read`。规划/保存仍走既有 Travel 页面流程，模型不偷偷写 itinerary。

use std::sync::Arc;

use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::ToolExecutor;
use crate::travel::ports::{TravelAiPort, TravelSearchHit, TripContext};

const TOOL_SEARCH_DESTINATION: &str = "travel.search_destination";
const TOOL_GET_DESTINATION: &str = "travel.get_destination";
const TOOL_GET_TRIP_CONTEXT: &str = "travel.get_trip_context";
const TOOL_PLAN_TRIP: &str = "travel.plan_trip";

/// Travel 工具执行器（一个结构体、四个身份；dispatch 属模块内部实现细节）。
pub struct TravelTools {
    port: Arc<dyn TravelAiPort>,
    budget: ContextBudget,
}

impl TravelTools {
    #[must_use]
    pub fn new(port: Arc<dyn TravelAiPort>) -> Self {
        Self {
            port,
            budget: ContextBudget::default(),
        }
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let input_schema = match name {
            TOOL_SEARCH_DESTINATION => serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string", "description": "目的地/兴趣关键词，如「大连 看海」"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
            TOOL_GET_DESTINATION | TOOL_GET_TRIP_CONTEXT => serde_json::json!({
                "type": "object",
                "required": ["city"],
                "properties": {"city": {"type": "string"}}
            }),
            _ => serde_json::json!({
                "type": "object",
                "required": ["city"],
                "properties": {
                    "city": {"type": "string"},
                    "days": {"type": "integer", "minimum": 1, "maximum": 30}
                }
            }),
        };
        let description = match name {
            TOOL_SEARCH_DESTINATION => "搜索旅行目的地与兴趣点（只读，返回精炼命中列表）",
            TOOL_GET_DESTINATION => "获取已缓存目的地的速览信息（只读；未研究过则明确告知）",
            TOOL_GET_TRIP_CONTEXT => "获取当前行程上下文（城市/天数/摘要/亮点，供调整行程类指代问题）",
            _ => "生成行程规划预览（默认不写入永久数据；保存请走 Travel 页既有流程）",
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk: ToolRisk::Read,
            module: "travel".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 每个工具一个薄执行器（满足 ToolRegistry 的 ToolExecutor 契约）
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    tools: Arc<TravelTools>,
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
            TOOL_SEARCH_DESTINATION => self.tools.search_destination(&arguments).await,
            TOOL_GET_DESTINATION => self.tools.get_destination(&arguments).await,
            TOOL_GET_TRIP_CONTEXT => self.tools.get_trip_context(&arguments).await,
            _ => self.tools.plan_trip(&arguments).await,
        }
    }
}

// ---------------------------------------------------------------------------
// 具体工具逻辑
// ---------------------------------------------------------------------------

fn city_argument(arguments: &serde_json::Value, tool: &str) -> Result<String, AgentError> {
    let city = arguments
        .get("city")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if city.is_empty() {
        return Err(AgentError::tool_invalid_argument(format!(
            "{tool}: city is required"
        )));
    }
    Ok(city.to_string())
}

fn limit_or(arguments: &serde_json::Value, default: usize) -> usize {
    arguments
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
        .clamp(1, 20)
}

impl TravelTools {
    pub async fn search_destination(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let query = arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if query.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "travel.search_destination: query is empty",
            ));
        }
        let limit = limit_or(arguments, 8);
        if !self.port.configured() {
            return Ok(ToolResult::fail("travel search 未配置（请检查搜索后端设置）"));
        }
        let hits: Vec<TravelSearchHit> = self
            .port
            .search_destination(&query, limit)
            .await
            .map_err(|error| AgentError::tool_execution_failed(format!("travel search: {error}")))?;
        let list: Vec<serde_json::Value> = hits
            .into_iter()
            .map(|hit| {
                serde_json::json!({
                    "title": hit.title,
                    "url": hit.url,
                    "snippet": hit.snippet,
                    "domain": hit.domain,
                })
            })
            .collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::Value::Array(list),
            serde_json::json!({"count": hits_count(&list)}),
        ))
    }

    pub async fn get_destination(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let city = city_argument(arguments, "travel.get_destination")?;
        match self
            .port
            .trip_context(&city)
            .map_err(|error| AgentError::tool_execution_failed(format!("travel cache: {error}")))?
        {
            Some(context) => Ok(ToolResult::ok(trip_context_json(&context))),
            None => Ok(ToolResult::fail(format!(
                "目的地「{city}」暂无已缓存资料，请先在 Travel 页发起研究"
            ))),
        }
    }

    pub async fn get_trip_context(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let city = city_argument(arguments, "travel.get_trip_context")?;
        match self
            .port
            .trip_context(&city)
            .map_err(|error| AgentError::tool_execution_failed(format!("travel cache: {error}")))?
        {
            Some(context) => Ok(ToolResult::ok(trip_context_json(&context))),
            None => Ok(ToolResult::fail(format!(
                "「{city}」当前没有已保存行程；可提示用户先研究该城市"
            ))),
        }
    }

    pub async fn plan_trip(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let city = city_argument(arguments, "travel.plan_trip")?;
        let days = arguments
            .get("days")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as u8)
            .unwrap_or(3)
            .clamp(1, 30);
        match self
            .port
            .plan_preview(&city, days)
            .map_err(|error| AgentError::tool_execution_failed(format!("travel preview: {error}")))?
        {
            Some(preview) => Ok(ToolResult::ok_with_metadata(
                trip_context_json(&preview),
                serde_json::json!({"note": "预览未写入永久数据；确认后可到 Travel 页保存"}),
            )),
            None => Ok(ToolResult::fail(format!(
                "暂无「{city} {days} 天」的规划预览；请先在 Travel 页完成研究后再规划"
            ))),
        }
    }
}

fn hits_count(list: &[serde_json::Value]) -> usize {
    list.len()
}

fn trip_context_json(context: &TripContext) -> serde_json::Value {
    serde_json::json!({
        "city": context.city,
        "days": context.days,
        "summary": context.summary,
        "highlights": context.highlights,
        "sources_count": context.sources_count,
        "from_cache": context.from_cache,
    })
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Travel 模块上下文提供方（V5 §37）：AppContext{destination, days, preferences} → 紧凑行程。
pub struct TravelContextProvider {
    port: Arc<dyn TravelAiPort>,
    budget: ContextBudget,
}

impl ModuleContextProvider for TravelContextProvider {
    fn module_id(&self) -> &str {
        "travel"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let Some(entity) = &app_context.entity else {
            return Ok(ContextBundle {
                module: "travel".to_string(),
                headline: "Travel".to_string(),
                summary: serde_json::json!({"note": "当前无目的地上下文（Travel 总览）"}),
            });
        };
        // 支持 kind=destination / kind=city；entity.id 为城市名。
        let city = entity.id.trim().to_string();
        if city.is_empty() {
            return Ok(ContextBundle {
                module: "travel".to_string(),
                headline: "Travel".to_string(),
                summary: serde_json::json!({"note": "当前无目的地上下文（Travel 总览）"}),
            });
        }
        let days = view_state_days(app_context);
        let headline = match days {
            Some(days) => format!("Travel · {}（{} 天）", city, days),
            None => format!("Travel · {city}"),
        };

        match self
            .port
            .trip_context(&city)
            .map_err(|error| AgentError::context(format!("travel context: {error}")))?
        {
            Some(context) => {
                // ContextBudget：亮点截断到 max_items。
                let highlights: Vec<String> = context
                    .highlights
                    .into_iter()
                    .take(budget.max_items)
                    .collect();
                let mut summary = trip_context_json(&TripContext {
                    city: context.city,
                    days: days.unwrap_or(context.days),
                    summary: context.summary,
                    highlights,
                    sources_count: context.sources_count,
                    from_cache: context.from_cache,
                });
                if let Some(days) = days {
                    summary["days"] = serde_json::json!(days);
                }
                Ok(ContextBundle {
                    module: "travel".to_string(),
                    headline,
                    summary,
                })
            }
            None => Ok(ContextBundle {
                module: "travel".to_string(),
                headline,
                summary: serde_json::json!({
                    "city": city,
                    "days": days.unwrap_or(0),
                    "note": "当前无已缓存行程；如用户询问行程/偏好，提示先在 Travel 页发起研究"
                }),
            }),
        }
    }
}

fn view_state_days(app_context: &AppContext) -> Option<u8> {
    app_context
        .view_state
        .as_object()
        .and_then(|object| object.get("days"))
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as u8)
}

// ---------------------------------------------------------------------------
// 注册
// ---------------------------------------------------------------------------

/// 注册 Travel 模块（descriptor + 4 工具 + context provider）。
pub fn register_travel(
    modules: &mut crate::personal_ai::registry::ModuleRegistry,
    tools: &mut crate::personal_ai::registry::ToolRegistry,
    port: Arc<dyn TravelAiPort>,
) -> Result<(), AgentError> {
    let travel = Arc::new(TravelTools::new(Arc::clone(&port)));
    modules.register(crate::personal_ai::registry::ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "travel".into(),
            display_name: "Travel".into(),
            description: "旅行研究：目的地搜索、行程速览与规划预览".into(),
            capabilities: vec![
                "search".into(),
                "destination".into(),
                "planning".into(),
                "itinerary".into(),
            ],
            tools: vec![
                TOOL_SEARCH_DESTINATION.into(),
                TOOL_GET_DESTINATION.into(),
                TOOL_GET_TRIP_CONTEXT.into(),
                TOOL_PLAN_TRIP.into(),
            ],
        },
        context_provider: Some(Arc::new(TravelContextProvider {
            port: Arc::clone(&port),
            budget: ContextBudget::default(),
        })),
    })?;
    for name in [
        TOOL_SEARCH_DESTINATION,
        TOOL_GET_DESTINATION,
        TOOL_GET_TRIP_CONTEXT,
        TOOL_PLAN_TRIP,
    ] {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&travel),
            spec_cache: std::sync::OnceLock::new(),
        }))?;
    }
    Ok(())
}

/// 导出工具常量（外部测试 / 组合根引用）。
#[must_use]
pub fn travel_tool_names() -> [&'static str; 4] {
    [
        TOOL_SEARCH_DESTINATION,
        TOOL_GET_DESTINATION,
        TOOL_GET_TRIP_CONTEXT,
        TOOL_PLAN_TRIP,
    ]
}

#[cfg(test)]
mod travel_tests;