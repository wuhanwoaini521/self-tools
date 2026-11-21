//! Knowledge（Memory / Documents / Files / 全局检索）的 HTTP 接口。
//!
//! ## 为什么需要这一层
//!
//! 这些能力**在 Rust 侧早已完整实现**（`KnowledgeRuntime`，三个 SQLite 索引库 +
//! 三个域服务 + 统一检索），但历史上只有桌面组合根装配了它，server 端完全没接。
//! 结果是：**这台机器明明就是服务器**，浏览器打开却处处显示
//! 「浏览器预览不支持本地知识库」「浏览器预览不支持打开本地文件」。
//!
//! 同一份数据、同一套服务，只是换一条 IPC 通道——不是降级，是把已有的能力接出来。
//!
//! 与桌面端共用 `KnowledgeRuntime::build`，因此两端行为一致，
//! 数据也落在同一个 `<config>/` 目录（memory.db / documents.db / files.db）。

use std::sync::Arc;

use axum::extract::{Path, Query};
use axum::{Extension, Json};
use devtoolbox_application::ApplicationError;
use devtoolbox_core::files::KnowledgeRoot;
use devtoolbox_core::memory::{MemoryCategory, MemoryDraft, MemoryQuery, MemorySensitivity};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai_api::ApiError;

// ============================================================================
// DTO
// ============================================================================

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryListQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchQuery {
    pub query: String,
    pub limit: Option<usize>,
}

/// 记忆统计（与桌面端 `MemoryStatsDto` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryStatsDto {
    pub total: usize,
    pub active: usize,
    pub candidates: usize,
    pub archived: usize,
    pub rejected: usize,
    pub expired: usize,
    pub categories: Vec<MemoryCategoryCountDto>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryCategoryCountDto {
    pub category: String,
    pub count: usize,
}

/// 文档索引状态（与桌面端 `DocumentStatusDto` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct DocumentStatusDto {
    /// 允许根（设置页据此展示「目录已配置」；网页端也需要）。
    pub roots: Vec<KnowledgeRoot>,
    pub configured: bool,
    pub documents: usize,
    pub chunks: usize,
    pub content_available: usize,
    pub metadata_only: usize,
    pub failed: usize,
}

/// 文件索引状态（与桌面端 `FileStatusDto` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct FileStatusDto {
    pub roots: Vec<KnowledgeRoot>,
    pub configured: bool,
    pub files: usize,
    pub text_files: usize,
    pub binary_files: usize,
    pub restricted: usize,
    pub failed: usize,
}

/// 知识运行时总览（与桌面端 `KnowledgeStatusDto` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct KnowledgeStatusDto {
    pub sources: Vec<String>,
    pub budget: devtoolbox_core::knowledge::KnowledgeBudget,
    pub metrics: devtoolbox_application::knowledge::KnowledgeMetricsSnapshot,
    pub configured_roots: Vec<KnowledgeRoot>,
}

/// 允许根：文件根 + 文档有效根（去重），与桌面端 `knowledge_roots` 同义。
fn knowledge_roots(settings: &devtoolbox_core::settings::KnowledgeSettings) -> Vec<KnowledgeRoot> {
    let mut roots = settings.file_roots.clone();
    for root in settings.effective_document_roots() {
        if !roots.iter().any(|existing| existing.id == root.id) {
            roots.push(root);
        }
    }
    roots
}

/// 三个域服务的错误 → HTTP 错误契约。
///
/// 错误码与桌面端 `CommandError` 保持一致（`memory_error` / `documents_error` /
/// `files_error`），这样两端页面按 code 分类展示的行为不变。
fn knowledge_error(error: ApplicationError) -> ApiError {
    let code = match &error {
        ApplicationError::Memory { .. } => "memory_error",
        ApplicationError::Documents { .. } => "documents_error",
        ApplicationError::Files { .. } => "files_error",
        _ => "knowledge_error",
    };
    ApiError {
        code,
        message: error.to_string(),
    }
}

fn parse_memory_category(raw: Option<&str>) -> Result<Option<MemoryCategory>, ApiError> {
    match raw {
        None => Ok(None),
        // 入参校验失败用 `invalid`（400）；域服务失败保留 `memory_error` 等码（桌面端同义）。
        Some(raw) => MemoryCategory::parse(raw)
            .map(Some)
            .ok_or_else(|| ApiError {
                code: "invalid",
                message: format!("未知记忆分类：{raw}"),
            }),
    }
}

fn parse_memory_sensitivity(raw: Option<&str>) -> Result<Option<MemorySensitivity>, ApiError> {
    match raw {
        None => Ok(None),
        Some(raw) => MemorySensitivity::parse(raw)
            .map(Some)
            .ok_or_else(|| ApiError {
                code: "invalid",
                message: format!("未知敏感度：{raw}"),
            }),
    }
}

/// 把可序列化结果包成 JSON（这些 DTO 不是 `Value`，直接 `Json(x)` 类型不通）。
fn to_json<T: serde::Serialize>(
    value: T,
    code: &'static str,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| crate::ai_api::ApiError {
            code,
            message: error.to_string(),
        })
}

// ============================================================================
// Memory
// ============================================================================

/// `GET /api/v1/memory/list`
pub async fn memory_list(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Query(params): Query<MemoryListQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let spec = MemoryQuery {
        query: params.query.unwrap_or_default(),
        // 与桌面端 `parse_memory_category/status` 语义一致：缺省 = 不筛选。
        category: params
            .category
            .as_deref()
            .and_then(devtoolbox_core::memory::MemoryCategory::parse),
        status: params
            .status
            .as_deref()
            .and_then(devtoolbox_core::memory::MemoryStatus::parse),
        // 管理页是用户显式动作：与桌面端一致，可见敏感项（模型路径仍过滤）。
        include_sensitive: true,
        limit: params.limit.unwrap_or(0),
    };
    let items = knowledge
        .memory
        .list(&spec)
        .map_err(|error| crate::ai_api::ApiError {
            code: "memory_list_failed",
            message: error.to_string(),
        })?;
    Ok(Json(Value::Array(
        items
            .iter()
            .map(devtoolbox_application::personal_ai::memory::memory_json)
            .collect(),
    )))
}

/// `GET /api/v1/memory/stats`
pub async fn memory_stats(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let stats = knowledge
        .memory
        .stats()
        .map_err(|error| crate::ai_api::ApiError {
            code: "memory_stats_failed",
            message: error.to_string(),
        })?;
    let categories = knowledge
        .memory
        .category_counts()
        .map_err(|error| crate::ai_api::ApiError {
            code: "memory_category_counts_failed",
            message: error.to_string(),
        })?
        .into_iter()
        .map(|(category, count)| MemoryCategoryCountDto {
            category: category.as_str().to_string(),
            count,
        })
        .collect();
    Ok(Json(serde_json::json!(MemoryStatsDto {
        total: stats.total(),
        active: stats.active,
        candidates: stats.candidates,
        archived: stats.archived,
        rejected: stats.rejected,
        expired: stats.expired,
        categories,
    })))
}

// ============================================================================
// Memory 写入（桌面端 `memory_save` / `memory_update` / 三态流转的同义端点）
// ============================================================================

/// 新建记忆请求（与桌面端 `MemoryDraftRequest` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryDraftDto {
    pub category: String,
    pub content: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_reference: Option<String>,
    #[serde(default)]
    pub sensitivity: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
}

impl MemoryDraftDto {
    fn into_draft(self) -> Result<MemoryDraft, ApiError> {
        let category = parse_memory_category(Some(&self.category))?.ok_or_else(|| ApiError {
            code: "invalid",
            message: "记忆分类不能为空".to_string(),
        })?;
        let source_type = match self.source_type.as_deref() {
            None => devtoolbox_core::memory::MemorySourceType::ExplicitUser,
            Some(raw) => {
                devtoolbox_core::memory::MemorySourceType::parse(raw).ok_or_else(|| ApiError {
                    code: "invalid",
                    message: format!("未知来源类型：{raw}"),
                })?
            }
        };
        let sensitivity = parse_memory_sensitivity(self.sensitivity.as_deref())?
            .unwrap_or(MemorySensitivity::Normal);
        Ok(MemoryDraft {
            category,
            content: self.content,
            source_type,
            source_reference: self.source_reference,
            sensitivity,
            confidence: self.confidence.unwrap_or(0.9),
            expires_at: None,
            metadata: Value::Null,
        })
    }
}

/// `GET /api/v1/memory/{id}`
pub async fn memory_get(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge.memory.get(&id, true).map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

/// `POST /api/v1/memory`：用户显式保存（唯一能把记忆写成 ACTIVE 的入口之一）。
pub async fn memory_save(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<MemoryDraftDto>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge
        .memory
        .save_confirmed(request.into_draft()?)
        .map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

/// `POST /api/v1/memory/update`
#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryUpdateDto {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sensitivity: Option<String>,
}

pub async fn memory_update(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<MemoryUpdateDto>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge
        .memory
        .update(
            &request.id,
            &request.content,
            parse_memory_category(request.category.as_deref())?,
            parse_memory_sensitivity(request.sensitivity.as_deref())?,
        )
        .map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

/// `POST /api/v1/memory/{id}/confirm`
pub async fn memory_confirm(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge.memory.confirm(&id).map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

/// `POST /api/v1/memory/{id}/reject`
pub async fn memory_reject(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge.memory.reject(&id).map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

/// `POST /api/v1/memory/{id}/archive`
pub async fn memory_archive(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let item = knowledge.memory.archive(&id).map_err(knowledge_error)?;
    Ok(Json(
        devtoolbox_application::personal_ai::memory::memory_json(&item),
    ))
}

// ============================================================================
// Documents / Files
// ============================================================================

/// `GET /api/v1/documents/status`
pub async fn documents_status(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let stats = knowledge
        .documents
        .stats()
        .map_err(|error| crate::ai_api::ApiError {
            code: "documents_status_failed",
            message: error.to_string(),
        })?;
    let settings = knowledge.settings();
    Ok(Json(serde_json::json!(DocumentStatusDto {
        configured: !settings.effective_document_roots().is_empty(),
        roots: knowledge_roots(&settings),
        documents: stats.documents,
        chunks: stats.chunks,
        content_available: stats.content_available,
        metadata_only: stats.metadata_only,
        failed: stats.failed,
    })))
}

/// `GET /api/v1/documents/recent`
pub async fn documents_recent(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    to_json(
        knowledge
            .documents
            .recent(params.limit.unwrap_or(20))
            .map_err(|error| crate::ai_api::ApiError {
                code: "documents_recent_failed",
                message: error.to_string(),
            })?,
        "documents_recent_encode_failed",
    )
}

/// `GET /api/v1/documents/search?q=`
pub async fn documents_search(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    // 与桌面端共用整形函数（`document_hit_json`），两端返回形状一致。
    let hits = devtoolbox_infrastructure::knowledge_runtime::search_documents(
        knowledge.as_ref(),
        &params.query,
        None,
        params.limit.unwrap_or(20),
    )
    .map_err(|message| crate::ai_api::ApiError {
        code: "documents_search_failed",
        message,
    })?;
    Ok(Json(Value::Array(hits)))
}

/// 扫描请求（桌面端命令参数 `rootId`；查询串/body 都接受两种写法）。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ScanRequest {
    #[serde(default, alias = "rootId")]
    pub root_id: Option<String>,
}

/// `POST /api/v1/documents/scan`：只写本地索引库（与桌面端 `documents_scan` 同一用例）。
///
/// 扫描会读磁盘，放到阻塞线程池里跑，避免占住 async 执行器。
pub async fn documents_scan(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<ScanRequest>,
) -> Result<Json<Value>, ApiError> {
    let settings = knowledge.settings();
    let roots: Vec<KnowledgeRoot> = settings
        .effective_document_roots()
        .into_iter()
        .filter(|root| root.enabled)
        .filter(|root| request.root_id.as_deref().is_none_or(|id| id == root.id))
        .collect();
    if roots.is_empty() {
        return Err(ApiError {
            code: "documents_error",
            message: "未配置文档目录：请在 设置 → 知识 中添加允许目录".to_string(),
        });
    }
    let documents = Arc::clone(&knowledge.documents);
    let metrics = Arc::clone(&knowledge.metrics);
    let started = std::time::Instant::now();
    let reports = tokio::task::spawn_blocking(move || {
        let mut reports = Vec::new();
        for root in &roots {
            reports.push(
                documents
                    .index_root(root, &settings)
                    .map_err(knowledge_error)?,
            );
        }
        metrics.record_index(&reports, &[], started.elapsed().as_millis() as u64);
        Ok::<_, ApiError>(reports)
    })
    .await
    .map_err(|error| ApiError {
        code: "documents_error",
        message: error.to_string(),
    })??;
    to_json(reports, "documents_scan_encode_failed")
}

/// `GET /api/v1/documents/{id}`
pub async fn documents_get(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Path(document_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let meta = knowledge
        .documents
        .get(&document_id)
        .map_err(knowledge_error)?;
    to_json(meta, "documents_get_encode_failed")
}

/// 文档读取请求（与桌面端 `DocumentReadRequestDto` 字段一致）。
#[derive(Debug, Deserialize, Serialize)]
pub struct DocumentReadDto {
    pub document_id: String,
    #[serde(default)]
    pub chunk_id: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// `POST /api/v1/documents/read`
pub async fn documents_read(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<DocumentReadDto>,
) -> Result<Json<Value>, ApiError> {
    let read = devtoolbox_core::documents::DocumentReadRequest {
        chunk_id: request.chunk_id,
        section: request.section,
        offset: request.offset.unwrap_or(0),
        max_chars: request.max_chars.unwrap_or(2_000),
    };
    let result = knowledge
        .documents
        .read(&request.document_id, &read)
        .map_err(knowledge_error)?;
    to_json(result, "documents_read_encode_failed")
}

/// `GET /api/v1/files/status`
pub async fn files_status(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let stats = knowledge
        .files
        .stats()
        .map_err(|error| crate::ai_api::ApiError {
            code: "files_status_failed",
            message: error.to_string(),
        })?;
    let settings = knowledge.settings();
    Ok(Json(serde_json::json!(FileStatusDto {
        roots: knowledge_roots(&settings),
        configured: settings.file_policy().is_configured(),
        files: stats.files,
        text_files: stats.text_files,
        binary_files: stats.binary_files,
        restricted: stats.restricted,
        failed: stats.failed,
    })))
}

/// `GET /api/v1/files/recent`
pub async fn files_recent(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    to_json(
        knowledge
            .files
            .recent(params.limit.unwrap_or(20))
            .map_err(|error| crate::ai_api::ApiError {
                code: "files_recent_failed",
                message: error.to_string(),
            })?,
        "files_recent_encode_failed",
    )
}

/// `GET /api/v1/files/search?q=`
pub async fn files_search(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    to_json(
        knowledge
            .files
            .search(
                &knowledge.settings(),
                &devtoolbox_core::files::FileQuery {
                    query: params.query.clone(),
                    limit: params.limit.unwrap_or(20),
                    ..Default::default()
                },
            )
            .map_err(|error| crate::ai_api::ApiError {
                code: "files_search_failed",
                message: error.to_string(),
            })?,
        "files_search_encode_failed",
    )
}

/// `POST /api/v1/files/scan`：与桌面端 `files_scan` 同一用例与同一条策略校验。
pub async fn files_scan(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<ScanRequest>,
) -> Result<Json<Value>, ApiError> {
    let settings = knowledge.settings();
    if !settings.file_policy().is_configured() {
        return Err(ApiError {
            code: "files_error",
            message: "未配置允许目录：请在 设置 → 知识 中添加允许目录".to_string(),
        });
    }
    let roots: Vec<KnowledgeRoot> = settings
        .file_policy()
        .enabled_roots()
        .into_iter()
        .filter(|root| request.root_id.as_deref().is_none_or(|id| id == root.id))
        .cloned()
        .collect();
    let files = Arc::clone(&knowledge.files);
    let metrics = Arc::clone(&knowledge.metrics);
    let started = std::time::Instant::now();
    let reports = tokio::task::spawn_blocking(move || {
        let mut reports = Vec::new();
        for root in &roots {
            let mut single = settings.clone();
            single.file_roots = vec![root.clone()];
            reports.extend(
                files
                    .index_configured_roots(&single)
                    .map_err(knowledge_error)?,
            );
        }
        metrics.record_index(&[], &reports, started.elapsed().as_millis() as u64);
        Ok::<_, ApiError>(reports)
    })
    .await
    .map_err(|error| ApiError {
        code: "files_error",
        message: error.to_string(),
    })??;
    to_json(reports, "files_scan_encode_failed")
}

/// 文件目标请求：`target` 可以是索引里的 `file_id`，也可以是允许根内的路径。
///
/// 两种形态都走 `FileService::authorize` 的 canonicalize + 根前缀 + deny 规则校验；
/// 网页端不提供「按任意路径打开」的逃逸口。
#[derive(Debug, Deserialize, Serialize)]
pub struct FileTargetDto {
    pub target: String,
}

/// `POST /api/v1/files/metadata`
pub async fn files_metadata(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<FileTargetDto>,
) -> Result<Json<Value>, ApiError> {
    let settings = knowledge.settings();
    let metadata = knowledge
        .files
        .metadata(&settings, &request.target)
        .map_err(knowledge_error)?;
    to_json(metadata, "files_metadata_encode_failed")
}

/// 文本读取请求（桌面端参数 `maxChars`）。
#[derive(Debug, Deserialize, Serialize)]
pub struct FileReadTextDto {
    pub target: String,
    #[serde(default, alias = "maxChars")]
    pub max_chars: Option<usize>,
}

/// `POST /api/v1/files/read_text`
pub async fn files_read_text(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Json(request): Json<FileReadTextDto>,
) -> Result<Json<Value>, ApiError> {
    let settings = knowledge.settings();
    let result = knowledge
        .files
        .read_text(&settings, &request.target, request.max_chars.unwrap_or(0))
        .map_err(knowledge_error)?;
    to_json(result, "files_read_text_encode_failed")
}

// ============================================================================
// 知识运行时总览
// ============================================================================

/// `GET /api/v1/knowledge/status`
pub async fn knowledge_status(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
) -> Result<Json<Value>, ApiError> {
    let settings = knowledge.settings();
    Ok(Json(serde_json::json!(KnowledgeStatusDto {
        sources: knowledge
            .retrieval
            .kinds()
            .into_iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        budget: knowledge.retrieval.budget(),
        metrics: knowledge.metrics.snapshot(),
        configured_roots: knowledge_roots(&settings),
    })))
}

// ============================================================================
// 全局检索（零 LLM 依赖）
// ============================================================================

/// `GET /api/v1/search/global?q=`
pub async fn global_search(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
    Extension(learning): Extension<Arc<devtoolbox_application::learning::LearningService>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(crate::ai_api::ApiError {
            code: "global_search_empty_query",
            message: "搜索关键词不能为空".to_string(),
        });
    }
    // `GlobalSearchService::search` 是同步的（零 LLM 依赖，纯本地索引）。
    let collection_search = devtoolbox_application::search::CollectionSearchPort::new(learning);
    let result = knowledge.search.search_with_port(
        &devtoolbox_core::search::GlobalSearchQuery {
            query: query.to_string(),
            limit_per_source: params.limit.unwrap_or(5).clamp(1, 20),
            sources: Vec::new(),
        },
        &collection_search,
    );
    to_json(result, "global_search_encode_failed")
}
