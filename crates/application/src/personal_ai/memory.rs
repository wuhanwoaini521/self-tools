//! Memory 标准模块适配器（V6 Track A，§17-§25）。
//!
//! - 注册 `memory` descriptor + 5 个工具 + ContextProvider；
//! - **写入边界**：`memory.save` 风险为 `SafeWrite`，但语义上只产生 `CANDIDATE`
//!   （§14/§113），并返回 `ConfirmMemorySave` Action 让前端向用户确认；
//!   工具路径**永不**产生 `ACTIVE`（§15/§16）。
//! - `memory.archive` 同样是可恢复操作（不删除，§20）。
//! - 读写都不返回 `SENSITIVE` 条目（§69）。

use std::sync::Arc;

use devtoolbox_core::memory::{
    MemoryCategory, MemoryDraft, MemoryItem, MemoryQuery, MemorySensitivity, MemorySourceType,
    MemoryStatus,
};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{Action, AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::memory::MemoryService;
use crate::personal_ai::args::{optional_string, require_string, tool_error, usize_arg};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_SEARCH: &str = "memory.search";
const TOOL_LIST: &str = "memory.list";
const TOOL_GET: &str = "memory.get";
const TOOL_SAVE: &str = "memory.save";
const TOOL_ARCHIVE: &str = "memory.archive";

const TOOL_NAMES: [&str; 5] = [TOOL_SEARCH, TOOL_LIST, TOOL_GET, TOOL_SAVE, TOOL_ARCHIVE];

/// 工具名清单（组合根与测试引用）。
#[must_use]
pub fn memory_tool_names() -> [&'static str; 5] {
    TOOL_NAMES
}

/// Memory 工具集（一个结构体、五个身份；dispatch 属模块内部实现细节）。
pub struct MemoryTools {
    service: Arc<MemoryService>,
}

impl MemoryTools {
    #[must_use]
    pub fn new(service: Arc<MemoryService>) -> Self {
        Self { service }
    }

    #[must_use]
    pub fn service(&self) -> &Arc<MemoryService> {
        &self.service
    }
}

fn spec_for(name: &str) -> ToolSpec {
    let (description, input_schema, risk) = match name {
        TOOL_SEARCH => (
            "搜索用户的长期记忆（只返回已确认生效的记忆）。keyword 为空则按分类返回最近的记忆。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "关键词（如 docker / 旅行偏好）"},
                    "category": {"type": "string", "enum": category_values()},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
            ToolRisk::Read,
        ),
        TOOL_LIST => (
            "列出长期记忆（按状态/分类过滤）。用户问「你记住了我什么」时使用。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["candidate", "active", "rejected", "archived", "expired"]},
                    "category": {"type": "string", "enum": category_values()},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                }
            }),
            ToolRisk::Read,
        ),
        TOOL_GET => (
            "按 id 取一条长期记忆。",
            serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}}
            }),
            ToolRisk::Read,
        ),
        TOOL_SAVE => (
            "请求保存一条长期记忆（偏好/个人事实/项目事实/环境/习惯/指令）。\
             注意：本工具**只产生候选**，必须由用户在界面上确认后才会生效；\
             保存后请在回答中带上 metadata.ui_hint 里的 confirm_memory 动作，向用户询问是否记住。\
             禁止保存密码/密钥/令牌等凭据。",
            serde_json::json!({
                "type": "object",
                "required": ["content", "category"],
                "properties": {
                    "content": {"type": "string", "description": "一句话事实，不超过 400 字符"},
                    "category": {"type": "string", "enum": category_values()},
                    "source_type": {"type": "string", "enum": ["explicit_user", "conversation_candidate", "import", "system"]},
                    "sensitivity": {"type": "string", "enum": ["normal", "private", "sensitive"]},
                    "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                    "source_reference": {"type": "string"}
                }
            }),
            ToolRisk::SafeWrite,
        ),
        _ => (
            "归档一条长期记忆（可恢复；归档后不再参与检索）。不要用删除：本系统不提供删除。",
            serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}}
            }),
            ToolRisk::SafeWrite,
        ),
    };
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        risk,
        module: "memory".to_string(),
    }
}

fn category_values() -> Vec<&'static str> {
    MemoryCategory::ALL
        .into_iter()
        .map(MemoryCategory::as_str)
        .collect()
}

/// 单个工具的薄执行器（满足 `ToolRegistry` 契约）。
struct ToolImpl {
    name: &'static str,
    tools: Arc<MemoryTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        static CACHE: std::sync::OnceLock<[ToolSpec; 5]> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| {
            [
                spec_for(TOOL_SEARCH),
                spec_for(TOOL_LIST),
                spec_for(TOOL_GET),
                spec_for(TOOL_SAVE),
                spec_for(TOOL_ARCHIVE),
            ]
        });
        match self.name {
            TOOL_SEARCH => &cache[0],
            TOOL_LIST => &cache[1],
            TOOL_GET => &cache[2],
            TOOL_SAVE => &cache[3],
            _ => &cache[4],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_LIST => self.tools.list(&arguments),
            TOOL_GET => self.tools.get(&arguments),
            TOOL_SAVE => self.tools.save(&arguments),
            _ => self.tools.archive(&arguments),
        }
    }
}

impl MemoryTools {
    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let keyword = optional_string(arguments, "query").unwrap_or_default();
        let category = category_arg(arguments, "category")?;
        let limit = usize_arg(arguments, "limit").unwrap_or(10);
        let items = self
            .service
            .search(&keyword, category, limit)
            .map_err(tool_error)?;
        Ok(items_result(&items))
    }

    pub fn list(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let status = status_arg(arguments, "status")?;
        let category = category_arg(arguments, "category")?;
        let limit = usize_arg(arguments, "limit").unwrap_or(20);
        let items = self
            .service
            .list(&MemoryQuery {
                query: String::new(),
                category,
                status,
                include_sensitive: false,
                limit,
            })
            .map_err(tool_error)?;
        Ok(items_result(&items))
    }

    pub fn get(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = require_string(arguments, "id")?;
        let item = self.service.get(&id, false).map_err(tool_error)?;
        Ok(ToolResult::ok_with_metadata(
            memory_json(&item),
            serde_json::json!({"ui_hint": {"ui_blocks": [memory_list_block(&[item])]}}),
        ))
    }

    /// 模型路径：**只产生候选**（§14）。返回确认动作供前端执行（§25/§81）。
    pub fn save(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let content = require_string(arguments, "content")?;
        let category = category_arg(arguments, "category")?.ok_or_else(|| {
            AgentError::tool_invalid_argument("memory.save: category is required")
        })?;
        let source_type = source_type_arg(arguments, "source_type")?
            .unwrap_or(MemorySourceType::ConversationCandidate);
        let sensitivity =
            sensitivity_arg(arguments, "sensitivity")?.unwrap_or(MemorySensitivity::Normal);
        let confidence = arguments
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .map_or(0.6, |value| value as f32);
        let draft = MemoryDraft {
            category,
            content,
            source_type,
            source_reference: optional_string(arguments, "source_reference"),
            sensitivity,
            confidence,
            expires_at: None,
            metadata: serde_json::Value::Null,
        };
        let item = self.service.propose(draft).map_err(tool_error)?;
        debug_assert_eq!(item.status, MemoryStatus::Candidate);
        let action = Action::confirm_memory(serde_json::json!({
            "memory_id": item.id,
            "category": item.category.as_str(),
            "content": item.content,
            "source_type": item.source_type.as_str(),
        }));
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "memory": memory_json(&item),
                "needs_confirmation": true,
                "note": "已创建候选记忆，需用户确认后生效（请向用户询问是否记住）",
            }),
            serde_json::json!({
                "ui_hint": {
                    "actions": [action],
                    "ui_blocks": [memory_list_block(std::slice::from_ref(&item))]
                }
            }),
        ))
    }

    pub fn archive(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = require_string(arguments, "id")?;
        let item = self.service.archive(&id).map_err(tool_error)?;
        Ok(ToolResult::ok(serde_json::json!({
            "memory": memory_json(&item),
            "note": "已归档（可恢复，不再参与检索）",
        })))
    }
}

/// 列表结果 + `memory_list` UI Block 提示。
fn items_result(items: &[MemoryItem]) -> ToolResult {
    if items.is_empty() {
        return ToolResult::ok(serde_json::json!({
            "items": [],
            "count": 0,
            "note": "没有找到相关记忆",
        }));
    }
    ToolResult::ok_with_metadata(
        serde_json::json!({"items": items.iter().map(memory_json).collect::<Vec<_>>(), "count": items.len()}),
        serde_json::json!({"ui_hint": {"ui_blocks": [memory_list_block(items)]}}),
    )
}

fn memory_list_block(items: &[MemoryItem]) -> serde_json::Value {
    serde_json::json!({
        "kind": "memory_list",
        "title": "个人记忆",
        "data": {"items": items.iter().map(memory_json).collect::<Vec<_>>()},
    })
}

/// 记忆的 JSON 形状（前端与模型共用；不含任何敏感字段）。
#[must_use]
pub fn memory_json(item: &MemoryItem) -> serde_json::Value {
    serde_json::json!({
        "id": item.id,
        "category": item.category.as_str(),
        "category_label": item.category.label(),
        "content": item.content,
        "status": item.status.as_str(),
        "source_type": item.source_type.as_str(),
        "source_reference": item.source_reference,
        "sensitivity": item.sensitivity.as_str(),
        "confidence": item.confidence,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "last_used_at": item.last_used_at,
        "expires_at": item.expires_at,
        "needs_confirmation": item.status == MemoryStatus::Candidate,
    })
}

fn category_arg(
    arguments: &serde_json::Value,
    key: &str,
) -> Result<Option<MemoryCategory>, AgentError> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(raw) => MemoryCategory::parse(&raw)
            .map(Some)
            .ok_or_else(|| AgentError::tool_invalid_argument(format!("unknown category `{raw}`"))),
    }
}

fn status_arg(
    arguments: &serde_json::Value,
    key: &str,
) -> Result<Option<MemoryStatus>, AgentError> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(raw) => MemoryStatus::parse(&raw)
            .map(Some)
            .ok_or_else(|| AgentError::tool_invalid_argument(format!("unknown status `{raw}`"))),
    }
}

fn source_type_arg(
    arguments: &serde_json::Value,
    key: &str,
) -> Result<Option<MemorySourceType>, AgentError> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(raw) => MemorySourceType::parse(&raw).map(Some).ok_or_else(|| {
            AgentError::tool_invalid_argument(format!("unknown source_type `{raw}`"))
        }),
    }
}

fn sensitivity_arg(
    arguments: &serde_json::Value,
    key: &str,
) -> Result<Option<MemorySensitivity>, AgentError> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(raw) => MemorySensitivity::parse(&raw).map(Some).ok_or_else(|| {
            AgentError::tool_invalid_argument(format!("unknown sensitivity `{raw}`"))
        }),
    }
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Memory 模块上下文提供方。
pub struct MemoryProviderOwned {
    tools: MemoryTools,
}

impl MemoryProviderOwned {
    #[must_use]
    pub fn new(service: Arc<MemoryService>) -> Self {
        Self {
            tools: MemoryTools::new(service),
        }
    }
}

impl ModuleContextProvider for MemoryProviderOwned {
    fn module_id(&self) -> &str {
        "memory"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let service = &self.tools.service;
        if let Some(entity) = &app_context.entity
            && entity.kind == "memory"
        {
            let item = service
                .get(&entity.id, false)
                .map_err(|error| AgentError::context(error.to_string()))?;
            return Ok(ContextBundle {
                module: "memory".to_string(),
                headline: format!("Personal Memory · {}", item.category.label()),
                summary: serde_json::json!({
                    "module": "memory",
                    "entity": {"kind": "memory", "id": item.id, "label": item.category.label()},
                    "memory": memory_json(&item),
                }),
            });
        }
        let stats = service
            .stats()
            .map_err(|error| AgentError::context(error.to_string()))?;
        let recent = service
            .list(&MemoryQuery {
                limit: 5,
                ..MemoryQuery::default()
            })
            .map_err(|error| AgentError::context(error.to_string()))?;
        Ok(ContextBundle {
            module: "memory".to_string(),
            headline: "Personal Memory".to_string(),
            summary: serde_json::json!({
                "module": "memory",
                "note": "记忆总览：需要具体内容时调用 memory.search/list",
                "stats": {
                    "active": stats.active,
                    "candidates": stats.candidates,
                    "archived": stats.archived,
                    "rejected": stats.rejected,
                    "expired": stats.expired,
                },
                "recent": recent.iter().map(memory_json).collect::<Vec<_>>(),
            }),
        })
    }
}

/// 注册 Memory 模块（descriptor + 5 工具 + context provider）。
pub fn register_memory(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    service: Arc<MemoryService>,
) -> Result<(), AgentError> {
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "memory".into(),
            display_name: "Personal Memory".into(),
            description: "长期用户信息：偏好/个人事实/项目事实/环境/习惯/指令（写入需用户确认）"
                .into(),
            capabilities: vec![
                "search".into(),
                "list".into(),
                "save".into(),
                "archive".into(),
                "lifecycle".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(MemoryProviderOwned::new(Arc::clone(&service)))),
    })?;
    let shared = Arc::new(MemoryTools::new(service));
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&shared),
        }))?;
    }
    Ok(())
}

#[cfg(test)]
mod memory_tests;
