//! Documents 模块适配器（V6 Track B，§27-§39）。
//!
//! Documents 是标准 Personal AI 模块：注册 descriptor + 5 个 Read 工具 + ContextProvider。
//! **索引写入不是模型工具**（§6：`documents.scan` 不存在），模型只能检索 / 读取已索引内容；
//! 单份文档永不整份进入 prompt（§31：默认按 chunk / section / 字符区间取）。
//!
//! - `documents.search(query, document_type?, limit?)` → 文档命中（含位置与片段）
//! - `documents.get(document_id)` → 文档卡片（元数据）
//! - `documents.read(document_id, chunk_id?, section?, offset?, max_chars?)` → 正文片段
//! - `documents.get_context(document_id, max_chars?)` → 紧凑上下文（章节 + 文首）
//! - `documents.list_recent(limit?)` → 最近索引的文档
//!
//! 设置为组合根注入的闭包，**每次工具调用 / 上下文构建时读取**（如 `chunk_config`），
//! 改设置立即生效。

use std::sync::Arc;

use devtoolbox_core::documents::{
    DocumentMeta, DocumentReadRequest, DocumentReadResult, DocumentType, chunk_text,
};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::settings::KnowledgeSettings;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::documents::{DocumentHit, DocumentService};
use crate::personal_ai::args::{optional_string, require_string, tool_error, usize_arg};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_SEARCH: &str = "documents.search";
const TOOL_GET: &str = "documents.get";
const TOOL_READ: &str = "documents.read";
const TOOL_GET_CONTEXT: &str = "documents.get_context";
const TOOL_LIST_RECENT: &str = "documents.list_recent";

const TOOL_NAMES: [&str; 5] = [
    TOOL_SEARCH,
    TOOL_GET,
    TOOL_READ,
    TOOL_GET_CONTEXT,
    TOOL_LIST_RECENT,
];

/// 检索 / 列表默认上限（与 `DocumentConfig::search_limit` 一致）。
const DEFAULT_LIMIT: usize = 10;
/// `documents.get_context` 的 `head` 默认字符数（§31：绝不整份读取）。
const DEFAULT_HEAD_CHARS: usize = 800;
/// ContextProvider 里 `sections` 的展示上限。
const CONTEXT_SECTIONS: usize = 10;
/// `outline` 的重算窗口：4 × `target_chars`，覆盖标题与前若干字符。
const OUTLINE_SCAN_CHARS: usize = 4_800;

/// 工具名清单（组合根与测试引用）。
#[must_use]
pub fn documents_tool_names() -> [&'static str; 5] {
    TOOL_NAMES
}

/// Documents 工具集（一个结构体、五个身份；dispatch 属模块内部实现细节）。
pub struct DocumentsTools {
    service: Arc<DocumentService>,
    settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    budget: ContextBudget,
}

impl DocumentsTools {
    #[must_use]
    pub fn new(
        service: Arc<DocumentService>,
        settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self {
            service,
            settings,
            budget: ContextBudget::default(),
        }
    }

    #[must_use]
    pub fn service(&self) -> &Arc<DocumentService> {
        &self.service
    }

    /// 当前设置（每次调用读取，组合根改动立即生效）。
    fn settings(&self) -> KnowledgeSettings {
        (self.settings)()
    }

    /// 结果上限：显式参数优先，否则受上下文预算与默认值双重约束。
    fn limit(&self, requested: Option<usize>) -> usize {
        requested.unwrap_or_else(|| self.budget.max_items.min(DEFAULT_LIMIT))
    }
}

fn spec_for(name: &str) -> ToolSpec {
    let (description, input_schema) = match name {
        TOOL_SEARCH => (
            "在已索引文档中检索（标题命中优先于正文命中），返回文档 id、类型、位置与片段。\
             未命中时必须如实说明「没有找到」。",
            serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string", "description": "检索关键词（如 docker 数据目录）"},
                    "document_type": {"type": "string", "enum": document_type_values()},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
        ),
        TOOL_GET => (
            "取单个文档的元数据卡片（类型 / 路径 / 大小 / 修改时间 / chunk 数 / 内容是否可读）。",
            serde_json::json!({
                "type": "object",
                "required": ["document_id"],
                "properties": {"document_id": {"type": "string"}}
            }),
        ),
        TOOL_READ => (
            "读取文档正文片段（chunk / 章节 / 字符区间三选一，默认不整份读取）。\
             仅索引了元数据的文档（PDF 未抽取 / 超大 / 解析失败）无法读取正文。",
            serde_json::json!({
                "type": "object",
                "required": ["document_id"],
                "properties": {
                    "document_id": {"type": "string"},
                    "chunk_id": {"type": "string", "description": "指定 chunk（优先级最高）"},
                    "section": {"type": "string", "description": "指定章节标题（子串匹配）"},
                    "offset": {"type": "integer", "minimum": 0, "description": "起始字符位置"},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": 20000}
                }
            }),
        ),
        TOOL_GET_CONTEXT => (
            "取文档的紧凑上下文：元数据 + 章节标题列表 + 文首片段。\
             用于回答「这份文档讲了什么」而不把整份文档读进上下文。",
            serde_json::json!({
                "type": "object",
                "required": ["document_id"],
                "properties": {
                    "document_id": {"type": "string"},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": 4000}
                }
            }),
        ),
        _ => (
            "列出最近索引的文档（按索引时间倒序）。",
            serde_json::json!({
                "type": "object",
                "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 50}}
            }),
        ),
    };
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        risk: ToolRisk::Read,
        module: "documents".to_string(),
    }
}

fn document_type_values() -> Vec<&'static str> {
    DocumentType::ALL
        .into_iter()
        .map(DocumentType::as_str)
        .collect()
}

/// 单个工具的薄执行器（满足 `ToolRegistry` 契约）。
struct ToolImpl {
    name: &'static str,
    tools: Arc<DocumentsTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        static CACHE: std::sync::LazyLock<[ToolSpec; 5]> = std::sync::LazyLock::new(|| {
            [
                spec_for(TOOL_SEARCH),
                spec_for(TOOL_GET),
                spec_for(TOOL_READ),
                spec_for(TOOL_GET_CONTEXT),
                spec_for(TOOL_LIST_RECENT),
            ]
        });
        match self.name {
            TOOL_SEARCH => &CACHE[0],
            TOOL_GET => &CACHE[1],
            TOOL_READ => &CACHE[2],
            TOOL_GET_CONTEXT => &CACHE[3],
            _ => &CACHE[4],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_GET => self.tools.get(&arguments),
            TOOL_READ => self.tools.read(&arguments),
            TOOL_GET_CONTEXT => self.tools.get_context(&arguments),
            _ => self.tools.list_recent(&arguments),
        }
    }
}

impl DocumentsTools {
    /// 检索（§30）：命中为空时如实返回「没有找到」，不编造（§67）。
    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = require_string(arguments, "query")?;
        let document_type = document_type_arg(arguments)?;
        let limit = self.limit(usize_arg(arguments, "limit"));
        let hits = self
            .service
            .search(&query, document_type, limit)
            .map_err(tool_error)?;
        if hits.is_empty() {
            return Ok(ToolResult::ok(serde_json::json!({
                "items": [],
                "note": "没有找到匹配的文档",
            })));
        }
        let scored = self.service.to_knowledge_results(&hits, &query);
        let items: Vec<serde_json::Value> = hits
            .iter()
            .zip(scored.iter())
            .map(|(hit, result)| document_hit_json(hit, result.score))
            .collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({"items": items, "count": items.len()}),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [document_list_block("文档命中", &items)]}
            }),
        ))
    }

    pub fn get(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let document_id = require_string(arguments, "document_id")?;
        let meta = self.service.get(&document_id).map_err(tool_error)?;
        let card = document_card_json(&meta);
        Ok(ToolResult::ok_with_metadata(
            card.clone(),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [{
                    "kind": "document_card",
                    "title": meta.title,
                    "data": card,
                }]}
            }),
        ))
    }

    /// 读取（§31）：chunk / section / range；仅元数据的文档 → 受控错误。
    pub fn read(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let document_id = require_string(arguments, "document_id")?;
        let request = DocumentReadRequest {
            chunk_id: optional_string(arguments, "chunk_id"),
            section: optional_string(arguments, "section"),
            offset: usize_arg(arguments, "offset").unwrap_or(0),
            // 0 → 服务默认上限（`DocumentConfig`），模块层不重复定义。
            max_chars: usize_arg(arguments, "max_chars").unwrap_or(0),
        };
        let DocumentReadResult {
            document_id,
            title,
            text,
            location,
            chunk_ids,
            total_chunks,
            truncated,
        } = self
            .service
            .read(&document_id, &request)
            .map_err(tool_error)?;
        let location = location.describe();
        let snippet: String = text.chars().take(200).collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "document_id": document_id,
                "title": title,
                "text": text,
                "location": location,
                "chunk_ids": chunk_ids,
                "total_chunks": total_chunks,
                "truncated": truncated,
            }),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [{
                    "kind": "document_reference",
                    "title": title,
                    "data": {
                        "document_id": document_id,
                        "title": title,
                        "location": location,
                        "snippet": snippet,
                    },
                }]}
            }),
        ))
    }

    /// 紧凑上下文：元数据 + 章节标题 + 文首片段（不把整份文档交给模型）。
    pub fn get_context(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let document_id = require_string(arguments, "document_id")?;
        let max_chars = usize_arg(arguments, "max_chars").unwrap_or(DEFAULT_HEAD_CHARS);
        let meta = self.service.get(&document_id).map_err(tool_error)?;
        let (sections, head) = self.outline(&meta)?;
        Ok(ToolResult::ok(serde_json::json!({
            "document": document_card_json(&meta),
            "sections": sections,
            "head": truncate_chars(&head, max_chars),
            "chunk_count": meta.chunk_count,
        })))
    }

    pub fn list_recent(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let limit = self.limit(usize_arg(arguments, "limit"));
        let recent = self.service.recent(limit).map_err(tool_error)?;
        let items: Vec<serde_json::Value> = recent.iter().map(meta_json).collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({"items": items, "count": items.len()}),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [document_list_block("最近索引的文档", &items)]}
            }),
        ))
    }

    /// 文档大纲：去重后的章节标题（文档顺序）+ 首个 chunk 文本。
    ///
    /// 索引里的 chunk 由 core 的纯函数 `chunk_text` 按 `settings.chunk_config()` 生成；
    /// 这里用**同一个函数**在同一份文本上重算，因此 `sections` 与 `head` 与索引一致，
    /// 而返回给模型的只有标题与前若干字符（正文不进 prompt，§31）。
    /// 仅元数据的文档（未抽取 / 超大 / 失败）→ 空大纲，不报错（§91 降级）。
    fn outline(&self, meta: &DocumentMeta) -> Result<(Vec<String>, String), AgentError> {
        if !meta.is_indexed_content() {
            return Ok((Vec::new(), String::new()));
        }
        // 有界读取：`OUTLINE_SCAN_CHARS` 足以覆盖章节标题与前若干字符，
        // 不把整份文档驻留内存（审查 V6-SEC-008；§31 绝不整份读取）。
        let text = self
            .service
            .read(
                &meta.document_id,
                &DocumentReadRequest {
                    chunk_id: None,
                    section: None,
                    offset: 0,
                    max_chars: OUTLINE_SCAN_CHARS,
                },
            )
            .map_err(tool_error)?
            .text;
        let chunks = chunk_text(&meta.document_id, &text, &self.settings().chunk_config());
        let mut sections: Vec<String> = Vec::new();
        for chunk in &chunks {
            if let Some(section) = chunk.location.section.as_deref()
                && !sections.iter().any(|known| known == section)
            {
                sections.push(section.to_string());
            }
        }
        let head = chunks
            .first()
            .map(|chunk| chunk.text.clone())
            .unwrap_or_default();
        Ok((sections, head))
    }
}

/// 文档命中 JSON（§5 `document_list` item 形状）。
fn document_hit_json(hit: &DocumentHit, score: f32) -> serde_json::Value {
    serde_json::json!({
        "document_id": hit.meta.document_id,
        "title": hit.meta.title,
        "document_type": hit.meta.document_type.as_str(),
        "relative_path": hit.meta.relative_path,
        "location": hit.location,
        "snippet": hit.snippet,
        "score": score,
        "modified_at": hit.meta.modified_at,
    })
}

/// 文档卡片 JSON（§5 `document_card` 形状）。
#[must_use]
pub fn document_card_json(meta: &DocumentMeta) -> serde_json::Value {
    serde_json::json!({
        "document_id": meta.document_id,
        "title": meta.title,
        "document_type": meta.document_type.as_str(),
        "path": meta.path,
        "relative_path": meta.relative_path,
        "size_bytes": meta.size_bytes,
        "modified_at": meta.modified_at,
        "chunk_count": meta.chunk_count,
        "content_available": meta.content_available,
        "index_error": meta.index_error,
    })
}

/// 文档索引元数据原样 JSON（`documents.list_recent` 用）。
fn meta_json(meta: &DocumentMeta) -> serde_json::Value {
    serde_json::to_value(meta).unwrap_or(serde_json::Value::Null)
}

fn document_list_block(title: &str, items: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "kind": "document_list",
        "title": title,
        "data": {"items": items},
    })
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn document_type_arg(arguments: &serde_json::Value) -> Result<Option<DocumentType>, AgentError> {
    match optional_string(arguments, "document_type") {
        None => Ok(None),
        Some(raw) => DocumentType::parse(&raw).map(Some).ok_or_else(|| {
            AgentError::tool_invalid_argument(format!("unknown document_type `{raw}`"))
        }),
    }
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Documents 模块上下文提供方：把 UI 选中的文档变紧凑上下文，否则给总览。
pub struct DocumentsProviderOwned {
    tools: DocumentsTools,
}

impl DocumentsProviderOwned {
    #[must_use]
    pub fn new(
        service: Arc<DocumentService>,
        settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self {
            tools: DocumentsTools::new(service, settings),
        }
    }
}

impl ModuleContextProvider for DocumentsProviderOwned {
    fn module_id(&self) -> &str {
        "documents"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        if let Some(entity) = &app_context.entity {
            if entity.kind != "document" {
                return Err(AgentError::context(format!(
                    "documents context: unsupported entity kind `{}` (expected document)",
                    entity.kind
                )));
            }
            let meta = self.tools.service.get(&entity.id).map_err(tool_error)?;
            let (sections, head) = self.tools.outline(&meta)?;
            return Ok(ContextBundle {
                module: "documents".to_string(),
                headline: format!("Documents · {}", meta.title),
                summary: serde_json::json!({
                    "module": "documents",
                    "entity": {"kind": "document", "id": meta.document_id, "label": meta.title},
                    "document": document_card_json(&meta),
                    "sections": sections.iter().take(CONTEXT_SECTIONS).collect::<Vec<_>>(),
                    "head": truncate_chars(&head, 400),
                }),
            });
        }
        let stats = self.tools.service.stats().map_err(tool_error)?;
        let recent = self.tools.service.recent(5).map_err(tool_error)?;
        Ok(ContextBundle {
            module: "documents".to_string(),
            headline: "Documents".to_string(),
            summary: serde_json::json!({
                "module": "documents",
                "note": "文档总览",
                "stats": {
                    "documents": stats.documents,
                    "chunks": stats.chunks,
                    "content_available": stats.content_available,
                    "metadata_only": stats.metadata_only,
                    "failed": stats.failed,
                },
                "recent_titles": recent.iter().map(|meta| meta.title.clone()).collect::<Vec<_>>(),
            }),
        })
    }
}

/// 注册 Documents 模块（descriptor + 5 工具 + context provider）。
pub fn register_documents(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    service: Arc<DocumentService>,
    settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
) -> Result<(), AgentError> {
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "documents".into(),
            display_name: "Documents".into(),
            description: "已索引文档：检索 / 元数据 / 分块读取 / 引用（索引写入不是模型工具）"
                .into(),
            capabilities: vec![
                "search".into(),
                "read".into(),
                "chunk".into(),
                "reference".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(DocumentsProviderOwned::new(
            Arc::clone(&service),
            Arc::clone(&settings),
        ))),
    })?;
    let shared = Arc::new(DocumentsTools::new(service, settings));
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&shared),
        }))?;
    }
    Ok(())
}

#[cfg(test)]
mod documents_tests;
