//! Knowledge 模块适配器（V6 Track D，§52-§69）。
//!
//! `knowledge` 是**统一检索入口**（facade）：一次 query 覆盖 Memory / Documents /
//! Files / Module 四类知识源，返回统一形状（每条都带 provenance）。四类源在物理上
//! 仍然独立（不同表、不同端口、不同模块），本模块只做「合并后的表达」：
//!
//! - `knowledge.search(query, sources?, limit?)` → 统一结果 + 观测（`KnowledgeDiagnostics`）
//! - ContextProvider：模块总览（已注册源 + 预算），**不自动检索**（§67：
//!   检索必须由 query 触发，命中不了就如实说「没有找到」）。
//!
//! 排序 / 去重 / 预算截断全部由 `KnowledgeRetrievalService` 负责；本模块不复制业务逻辑。

use std::sync::Arc;

use devtoolbox_core::knowledge::{KnowledgeQuery, KnowledgeResult, KnowledgeSourceKind};
use devtoolbox_core::memory::MemoryCategory;
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::knowledge::KnowledgeRetrievalService;
use crate::personal_ai::args::{require_string, tool_error, usize_arg};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_SEARCH: &str = "knowledge.search";

const TOOL_NAMES: [&str; 1] = [TOOL_SEARCH];

/// 工具名清单（组合根与测试引用）。
#[must_use]
pub fn knowledge_tool_names() -> [&'static str; 1] {
    TOOL_NAMES
}

/// Knowledge 工具集（统一检索入口）。
pub struct KnowledgeTools {
    service: Arc<KnowledgeRetrievalService>,
}

impl KnowledgeTools {
    #[must_use]
    pub fn new(service: Arc<KnowledgeRetrievalService>) -> Self {
        Self { service }
    }

    #[must_use]
    pub fn service(&self) -> &Arc<KnowledgeRetrievalService> {
        &self.service
    }
}

fn spec() -> ToolSpec {
    ToolSpec {
        name: TOOL_SEARCH.to_string(),
        description:
            "统一检索个人资料（记忆 / 文档 / 文件 / 模块知识），一次查询覆盖全部已启用的知识源，\
                      结果按相关性排序且每条都带来源（provenance）。\
                      没有命中时必须如实说明「没有找到」，禁止编造。"
                .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "检索问题或关键词"},
                "sources": {
                    "type": "array",
                    "description": "限定知识源（缺省 = 全部）",
                    "items": {"type": "string", "enum": source_values()}
                },
                "limit": {"type": "integer", "minimum": 1, "maximum": 20}
            }
        }),
        risk: ToolRisk::Read,
        module: "knowledge".to_string(),
    }
}

fn source_values() -> Vec<&'static str> {
    KnowledgeSourceKind::ALL
        .into_iter()
        .map(KnowledgeSourceKind::as_str)
        .collect()
}

/// 工具的薄执行器（满足 `ToolRegistry` 契约）。
struct ToolImpl {
    tools: Arc<KnowledgeTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        static CACHE: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(spec);
        &CACHE
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        self.tools.search(&arguments)
    }
}

impl KnowledgeTools {
    /// 统一检索（§58）：调用编排服务；空结果如实返回「没有找到」+ 观测。
    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = require_string(arguments, "query")?;
        let sources = sources_arg(arguments)?;
        let limit = usize_arg(arguments, "limit");
        let outcome = self
            .service
            .search(&KnowledgeQuery {
                query,
                sources,
                module: None,
                limit,
            })
            .map_err(tool_error)?;
        let diagnostics =
            serde_json::to_value(&outcome.diagnostics).unwrap_or(serde_json::Value::Null);
        if outcome.results.is_empty() {
            return Ok(ToolResult::ok(serde_json::json!({
                "items": [],
                "note": "没有找到相关个人资料",
                "diagnostics": diagnostics,
            })));
        }
        let items: Vec<serde_json::Value> = outcome.results.iter().map(result_json).collect();
        let blocks = ui_blocks(&outcome.results);
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "items": items,
                "count": items.len(),
                "diagnostics": diagnostics,
            }),
            serde_json::json!({"ui_hint": {"ui_blocks": blocks}}),
        ))
    }
}

/// 统一结果 JSON（`KnowledgeResult` 原样；provenance 必须保留，§66）。
fn result_json(result: &KnowledgeResult) -> serde_json::Value {
    serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
}

/// 按来源分组生成 UI Block（§5 形状；每组最多一个 block，空组不产出）。
fn ui_blocks(results: &[KnowledgeResult]) -> Vec<serde_json::Value> {
    let grouped = |kind: KnowledgeSourceKind| -> Vec<&KnowledgeResult> {
        results
            .iter()
            .filter(|result| result.source_type == kind)
            .collect()
    };
    let mut blocks: Vec<serde_json::Value> = Vec::new();
    let memories = grouped(KnowledgeSourceKind::Memory);
    if !memories.is_empty() {
        blocks.push(block(
            "memory_list",
            KnowledgeSourceKind::Memory,
            memories.iter().map(|result| memory_item(result)).collect(),
        ));
    }
    let documents = grouped(KnowledgeSourceKind::Document);
    if !documents.is_empty() {
        blocks.push(block(
            "document_list",
            KnowledgeSourceKind::Document,
            documents
                .iter()
                .map(|result| document_item(result))
                .collect(),
        ));
    }
    let files = grouped(KnowledgeSourceKind::File);
    if !files.is_empty() {
        blocks.push(block(
            "file_list",
            KnowledgeSourceKind::File,
            files.iter().map(|result| file_item(result)).collect(),
        ));
    }
    blocks
}

fn block(
    kind: &str,
    source: KnowledgeSourceKind,
    items: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "title": format!("相关{}", source.label()),
        "data": {"items": items},
    })
}

/// `memory_list` item（§5）；检索结果来自模型可见路径 → 无需确认。
fn memory_item(result: &KnowledgeResult) -> serde_json::Value {
    let category = meta_str(result, "category");
    let label = MemoryCategory::parse(&category)
        .map_or_else(|| category.clone(), |category| category.label().to_string());
    serde_json::json!({
        "id": result.source_id,
        "category": category,
        "category_label": label,
        "content": result.snippet,
        "status": meta_value(result, "status"),
        "source_type": meta_value(result, "source_type"),
        "sensitivity": meta_value(result, "sensitivity"),
        "updated_at": meta_value(result, "updated_at"),
        "needs_confirmation": false,
    })
}

/// `document_list` item（§5）。
fn document_item(result: &KnowledgeResult) -> serde_json::Value {
    serde_json::json!({
        "document_id": result.source_id,
        "title": result.title,
        "document_type": meta_value(result, "document_type"),
        "relative_path": meta_value(result, "relative_path"),
        "location": result.location,
        "snippet": result.snippet,
        "score": result.score,
        "modified_at": meta_value(result, "modified_at"),
    })
}

/// `file_list` item（§5）。
fn file_item(result: &KnowledgeResult) -> serde_json::Value {
    serde_json::json!({
        "file_id": result.source_id,
        "file_name": result.title,
        "relative_path": result.location,
        "path": result.provenance.path,
        "extension": meta_value(result, "extension"),
        "size_bytes": meta_value(result, "size_bytes"),
        "modified_at": meta_value(result, "modified_at"),
        "restricted": meta_value(result, "restricted"),
    })
}

/// 读 `metadata` 键（缺失 → `null`；模块层不猜测服务未提供的字段）。
fn meta_value(result: &KnowledgeResult, key: &str) -> serde_json::Value {
    result
        .metadata
        .get(key)
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// 读 `metadata` 字符串键（缺失 → 空串）。
fn meta_str(result: &KnowledgeResult, key: &str) -> String {
    result
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `sources` 数组 → 知识源枚举（未知值 → 参数错误，绝不静默忽略）。
fn sources_arg(arguments: &serde_json::Value) -> Result<Vec<KnowledgeSourceKind>, AgentError> {
    let Some(raw) = arguments.get("sources") else {
        return Ok(Vec::new());
    };
    let Some(values) = raw.as_array() else {
        return Err(AgentError::tool_invalid_argument(
            "`sources` must be an array of source kinds",
        ));
    };
    let mut sources: Vec<KnowledgeSourceKind> = Vec::with_capacity(values.len());
    for value in values {
        let raw = value.as_str().unwrap_or_default();
        let kind = KnowledgeSourceKind::parse(raw).ok_or_else(|| {
            AgentError::tool_invalid_argument(format!("unknown source kind `{raw}`"))
        })?;
        if !sources.contains(&kind) {
            sources.push(kind);
        }
    }
    Ok(sources)
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Knowledge 模块上下文提供方：只给总览（源 + 预算），不代替检索（§67）。
pub struct KnowledgeProviderOwned {
    tools: KnowledgeTools,
}

impl KnowledgeProviderOwned {
    #[must_use]
    pub fn new(service: Arc<KnowledgeRetrievalService>) -> Self {
        Self {
            tools: KnowledgeTools::new(service),
        }
    }
}

impl ModuleContextProvider for KnowledgeProviderOwned {
    fn module_id(&self) -> &str {
        "knowledge"
    }

    fn build_context(
        &self,
        _app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let service = &self.tools.service;
        let sources: Vec<&'static str> = service
            .kinds()
            .into_iter()
            .map(KnowledgeSourceKind::as_str)
            .collect();
        let budget = serde_json::to_value(service.budget()).unwrap_or(serde_json::Value::Null);
        Ok(ContextBundle {
            module: "knowledge".to_string(),
            headline: "Knowledge".to_string(),
            summary: serde_json::json!({
                "module": "knowledge",
                "sources": sources,
                "budget": budget,
                "note": "知识总览：检索需通过工具按 query 触发",
            }),
        })
    }
}

/// 注册 Knowledge 模块（descriptor + 1 工具 + context provider）。
pub fn register_knowledge(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    service: Arc<KnowledgeRetrievalService>,
) -> Result<(), AgentError> {
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "knowledge".into(),
            display_name: "Knowledge".into(),
            description: "个人知识统一检索入口：一次查询覆盖记忆 / 文档 / 文件 / 模块知识；\
                          四类源在物理上相互独立（不同表与端口），这里只统一表达与预算。"
                .into(),
            capabilities: vec![
                "search".into(),
                "memory".into(),
                "documents".into(),
                "files".into(),
                "provenance".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(KnowledgeProviderOwned::new(Arc::clone(&service)))),
    })?;
    let shared = Arc::new(KnowledgeTools::new(service));
    tools.register(Arc::new(ToolImpl { tools: shared }))?;
    Ok(())
}

#[cfg(test)]
mod knowledge_tests;
