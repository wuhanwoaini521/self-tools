//! Geography 模块适配器（V5 §44-§48，Track D）。
//!
//! Geography 是标准 Personal AI 模块：注册 descriptor + 4 个 Read 工具 +
//! ContextProvider。**不重做 UX / 不重建数据面**：经 `GeographyQueryPort`
//! （只读查询面 + 收藏写）读 canonical；应用层零 infra 依赖。
//!
//! - `geography.search(query, entity_type?, limit?)` → 精炼命中列表
//! - `geography.get_location(id)` → canonical 实体信息
//! - `geography.get_context(id)` → 实体 + 直接关系 + 来源（compact，budget 截断）
//! - `geography.get_exploration(id)` → 实体 + 相邻关系（供「这里为什么会这么高」式问答）

use std::sync::Arc;

use devtoolbox_core::geography::{GeoEntity, GeoEntityType};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::geography::{GeographyPortError, GeographyQueryPort};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::ToolExecutor;

const TOOL_SEARCH: &str = "geography.search";
const TOOL_GET_LOCATION: &str = "geography.get_location";
const TOOL_GET_CONTEXT: &str = "geography.get_context";
const TOOL_GET_EXPLORATION: &str = "geography.get_exploration";

/// Geography 工具执行器（一个结构体、四个身份；dispatch 属模块内部实现细节，
/// 不是 PersonalAgent 的分支）。
pub struct GeographyTools {
    port: Arc<dyn GeographyQueryPort + Send + Sync>,
    budget: ContextBudget,
}

impl GeographyTools {
    #[must_use]
    pub fn new(port: Arc<dyn GeographyQueryPort + Send + Sync>) -> Self {
        Self {
            port,
            budget: ContextBudget::default(),
        }
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let input_schema = match name {
            TOOL_SEARCH => serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词（如：珠穆朗玛）"},
                    "entity_type": {
                        "type": "string",
                        "enum": ["mountain", "mountain_range", "river", "plateau", "plain", "basin", "desert", "lake", "sea", "ocean", "island", "city", "region", "province", "country", "climate_zone", "tectonic_plate", "world"]
                    },
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
            _ => serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}}
            }),
        };
        let description = match name {
            TOOL_SEARCH => "搜索地理知识库（山/河/湖/城市/区域等），返回精炼命中列表",
            TOOL_GET_LOCATION => "获取地理实体的 canonical 信息（名称/类型/摘要/父级）",
            TOOL_GET_CONTEXT => {
                "获取地理实体的紧凑上下文：canonical + 直接关系 + 来源（供指代解析）"
            }
            _ => "获取地理实体及其相邻关系（探索视图；如「这里为什么会这么高」）",
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk: ToolRisk::Read,
            module: "geography".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 每个工具一个薄执行器（满足 ToolRegistry 的 ToolExecutor 契约）
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    tools: Arc<GeographyTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        // 静态 spec 缓存：按 name 构造（工具名固定）。
        static CACHE: std::sync::OnceLock<[ToolSpec; 4]> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| {
            let tools = GeographyTools {
                port: Arc::<UnavailablePort>::new(UnavailablePort),
                budget: ContextBudget::default(),
            };
            [
                tools.spec_for(TOOL_SEARCH),
                tools.spec_for(TOOL_GET_LOCATION),
                tools.spec_for(TOOL_GET_CONTEXT),
                tools.spec_for(TOOL_GET_EXPLORATION),
            ]
        });
        match self.name {
            TOOL_SEARCH => &cache[0],
            TOOL_GET_LOCATION => &cache[1],
            TOOL_GET_CONTEXT => &cache[2],
            _ => &cache[3],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_GET_LOCATION => self.tools.get_location(&arguments),
            TOOL_GET_CONTEXT => self.tools.get_context(&arguments),
            _ => self.tools.get_exploration(&arguments),
        }
    }
}

/// spec 缓存占位端口（永不执行）。
struct UnavailablePort;
impl GeographyQueryPort for UnavailablePort {
    fn all_entities(&self) -> Result<Vec<GeoEntity>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn recent_ids(&self, _limit: i64) -> Result<Vec<String>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn map_snapshot(
        &self,
    ) -> Result<
        (
            Vec<devtoolbox_core::geography::GeoMapPoint>,
            Vec<devtoolbox_core::geography::GeoMapLine>,
        ),
        GeographyPortError,
    > {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn search(
        &self,
        _query: &str,
        _entity_type: Option<GeoEntityType>,
        _limit: usize,
    ) -> Result<Vec<GeoEntity>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn entity(&self, _id: &str) -> Result<Option<GeoEntity>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn record_view(&self, _id: &str) -> Result<(), GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn relations_for(
        &self,
        _id: &str,
    ) -> Result<Vec<devtoolbox_core::geography::GeoRelation>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn sources(
        &self,
        _ids: &[String],
    ) -> Result<Vec<devtoolbox_core::geography::GeoSource>, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
    fn toggle_favorite(&self, _id: &str) -> Result<bool, GeographyPortError> {
        Err(GeographyPortError("unavailable".to_string()))
    }
}

// ---------------------------------------------------------------------------
// 具体工具逻辑
// ---------------------------------------------------------------------------

fn port_err(error: GeographyPortError) -> AgentError {
    AgentError::tool_execution_failed(error.0)
}

impl GeographyTools {
    /// entity_type 字符串 → 枚举（序列化为 snake_case；失败 → None，best-effort）。
    fn parse_entity_type(raw: &str) -> Option<GeoEntityType> {
        serde_json::from_value::<GeoEntityType>(serde_json::Value::String(raw.to_string())).ok()
    }

    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let entity_type = arguments
            .get("entity_type")
            .and_then(serde_json::Value::as_str)
            .and_then(Self::parse_entity_type);
        let limit = arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(8);
        if query.trim().is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "geography.search: query is empty",
            ));
        }
        let hits = self
            .port
            .search(query, entity_type, limit)
            .map_err(port_err)?;
        let data: Vec<serde_json::Value> = hits
            .into_iter()
            .map(|entity| {
                serde_json::json!({
                    "id": entity.id,
                    "entity_type": entity_type_label(entity.entity_type),
                    "name": entity.name,
                    "name_en": entity.name_en,
                    "summary": cap_chars(Some(&entity.summary), 400),
                })
            })
            .collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::Value::Array(data.clone()),
            serde_json::json!({"count": data.len()}),
        ))
    }

    pub fn get_location(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "geography.get_location: id is empty",
            ));
        }
        let Some(entity) = self.port.entity(id).map_err(port_err)? else {
            return Ok(ToolResult::fail(format!(
                "geography location `{id}` 不存在"
            )));
        };
        Ok(ToolResult::ok(entity_json(&entity, &self.budget)))
    }

    /// 紧凑上下文：canonical + 直接关系 + 来源（budget 截断；供指代解析）。
    pub fn get_context(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "geography.get_context: id is empty",
            ));
        }
        let Some(entity) = self.port.entity(id).map_err(port_err)? else {
            return Ok(ToolResult::fail(format!(
                "geography location `{id}` 不存在"
            )));
        };
        let relations = self.port.relations_for(id).map_err(port_err)?;
        let sources = self.port.sources(&entity.source_ids).map_err(port_err)?;
        let data = serde_json::json!({
            "module": "geography",
            "entity": {"id": entity.id, "name": entity.name, "entity_type": entity_type_label(entity.entity_type)},
            "canonical": entity_json(&entity, &self.budget),
            "relations": cap_list(&relations, self.budget.max_items, |relation| serde_json::json!({
                "kind": relation_kind_text(&relation.kind),
                "from_id": relation.from_id,
                "to_id": relation.to_id,
                "note": relation.note,
            })),
            "sources": cap_list(&sources, self.budget.max_items, |source| serde_json::json!({
                "url": source.url,
                "dataset": source.dataset,
            })),
        });
        Ok(ToolResult::ok(data))
    }

    /// 探索视图：实体 + 相邻关系。
    pub fn get_exploration(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "geography.get_exploration: id is empty",
            ));
        }
        let Some(entity) = self.port.entity(id).map_err(port_err)? else {
            return Ok(ToolResult::fail(format!(
                "geography location `{id}` 不存在"
            )));
        };
        let relations = self.port.relations_for(id).map_err(port_err)?;
        Ok(ToolResult::ok(serde_json::json!({
            "entity": entity_json(&entity, &self.budget),
            "relations": cap_list(&relations, self.budget.max_items, |relation| serde_json::json!({
                "kind": relation_kind_text(&relation.kind),
                "from_id": relation.from_id,
                "to_id": relation.to_id,
                "note": relation.note,
            })),
        })))
    }
}

/// 实体 JSON（紧凑 canonical）。
fn entity_json(entity: &GeoEntity, budget: &ContextBudget) -> serde_json::Value {
    serde_json::json!({
        "id": entity.id,
        "entity_type": entity_type_label(entity.entity_type),
        "name": entity.name,
        "name_en": entity.name_en,
        "aliases": cap_list(&entity.aliases, 6, |alias| serde_json::Value::String(alias.clone())),
        "parent_id": entity.parent_id,
        "summary": cap_chars(Some(&entity.summary), budget.max_chars / 2),
        "source_ids": cap_list(&entity.source_ids, budget.max_items, |source| serde_json::Value::String(source.clone())),
    })
}

fn entity_type_label(entity_type: GeoEntityType) -> &'static str {
    entity_type.label()
}

fn relation_kind_text(kind: &devtoolbox_core::geography::GeoRelationKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{kind:?}"))
}

fn cap_chars(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|text| {
        if text.chars().count() <= max {
            text.to_string()
        } else {
            format!("{}…[截断]", text.chars().take(max).collect::<String>())
        }
    })
}

fn cap_list<T>(
    items: &[T],
    max: usize,
    map: impl Fn(&T) -> serde_json::Value,
) -> Vec<serde_json::Value> {
    items.iter().take(max).map(map).collect()
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Geography 模块上下文提供方（V5 §46）：把 UI AppContext 变紧凑上下文。
pub struct GeographyContextProvider<'a> {
    tools: &'a GeographyTools,
}

impl ModuleContextProvider for GeographyContextProvider<'_> {
    fn module_id(&self) -> &str {
        "geography"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let Some(entity) = &app_context.entity else {
            return Ok(ContextBundle {
                module: "geography".to_string(),
                headline: "Geography".to_string(),
                summary: serde_json::json!({"note": "当前无具体地点上下文（Geography 总览）"}),
            });
        };
        if entity.kind != "location" {
            return Err(AgentError::context(format!(
                "geography context: unsupported entity kind `{}` (expected location)",
                entity.kind
            )));
        }
        let Some(geo) = self.tools.port.entity(&entity.id).map_err(port_err)? else {
            return Err(AgentError::context(format!(
                "geography location `{}` 不存在",
                entity.id
            )));
        };
        let relations = self
            .tools
            .port
            .relations_for(&entity.id)
            .map_err(port_err)?;
        let sources = self.tools.port.sources(&geo.source_ids).map_err(port_err)?;
        let headline = format!("Geography · {}", geo.name);
        let summary = serde_json::json!({
            "module": "geography",
            "entity": {"kind": "location", "id": geo.id, "label": geo.name},
            "canonical": {
                "name": geo.name,
                "entity_type": entity_type_label(geo.entity_type),
                "summary": cap_chars(Some(&geo.summary), budget.max_chars / 3),
                "parent_id": geo.parent_id,
            },
            "relations": cap_list(&relations, budget.max_items, |relation| serde_json::json!({
                "kind": relation_kind_text(&relation.kind),
                "from_id": relation.from_id,
                "to_id": relation.to_id,
                "note": relation.note,
            })),
            "sources_count": sources.len(),
        });
        Ok(ContextBundle {
            module: "geography".into(),
            headline,
            summary,
        })
    }
}

/// 组合根可见的 owned ContextProvider（Arc 化 GeographyTools）。
pub struct GeographyProviderOwned {
    tools: GeographyTools,
}
impl GeographyProviderOwned {
    #[must_use]
    pub fn new(port: Arc<dyn GeographyQueryPort + Send + Sync>) -> Self {
        Self {
            tools: GeographyTools::new(port),
        }
    }
}
impl ModuleContextProvider for GeographyProviderOwned {
    fn module_id(&self) -> &str {
        "geography"
    }
    fn build_context(
        &self,
        ctx: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        GeographyContextProvider { tools: &self.tools }.build_context(ctx, budget)
    }
}

/// 注册 Geography 模块（descriptor + 4 工具 + context provider）。
pub fn register_geography(
    modules: &mut crate::personal_ai::registry::ModuleRegistry,
    tools: &mut crate::personal_ai::registry::ToolRegistry,
    port: Arc<dyn GeographyQueryPort + Send + Sync>,
) -> Result<(), AgentError> {
    let geography = Arc::new(GeographyTools::new(Arc::clone(&port)));
    modules.register(crate::personal_ai::registry::ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "geography".into(),
            display_name: "Geography".into(),
            description: "地理知识库：山/河/湖/城市/区域等地形实体与关系".into(),
            capabilities: vec!["search".into(), "entity".into(), "exploration".into()],
            tools: vec![
                TOOL_SEARCH.into(),
                TOOL_GET_LOCATION.into(),
                TOOL_GET_CONTEXT.into(),
                TOOL_GET_EXPLORATION.into(),
            ],
        },
        context_provider: Some(Arc::new(GeographyProviderOwned::new(Arc::clone(&port)))),
    })?;
    for name in [
        TOOL_SEARCH,
        TOOL_GET_LOCATION,
        TOOL_GET_CONTEXT,
        TOOL_GET_EXPLORATION,
    ] {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&geography),
        }))?;
    }
    Ok(())
}

/// 导出工具常量（外部测试 / 组合根引用）。
#[must_use]
pub fn geography_tool_names() -> [&'static str; 4] {
    [
        TOOL_SEARCH,
        TOOL_GET_LOCATION,
        TOOL_GET_CONTEXT,
        TOOL_GET_EXPLORATION,
    ]
}

#[cfg(test)]
mod geography_tests;
