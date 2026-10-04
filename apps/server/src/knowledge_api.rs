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

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::Query;
use axum::{Extension, Json};
use devtoolbox_core::memory::MemoryQuery;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 运行时装配依赖。
pub struct KnowledgeDeps {
    pub config_dir: PathBuf,
    pub settings_loader: devtoolbox_infrastructure::knowledge_runtime::SettingsLoader,
    pub language: Arc<devtoolbox_application::language::LanguageService>,
}

/// 惰性构建 KnowledgeRuntime：server 启动时不因知识库失败而起不来。
pub fn build(
    deps: &KnowledgeDeps,
) -> Result<Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>, String> {
    devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime::build(
        &deps.config_dir,
        Arc::clone(&deps.settings_loader),
        devtoolbox_core::knowledge::KnowledgeBudget::default(),
        Arc::clone(&deps.language),
    )
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

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
    pub configured: bool,
    pub documents: usize,
    pub chunks: usize,
    pub content_available: usize,
    pub metadata_only: usize,
    pub failed: usize,
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
    Ok(Json(serde_json::json!({
        "total": stats.files,
        "configured": knowledge.settings().file_policy().is_configured(),
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

// ============================================================================
// 全局检索（零 LLM 依赖）
// ============================================================================

/// `GET /api/v1/search/global?q=`
pub async fn global_search(
    Extension(knowledge): Extension<
        Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    >,
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
    let result = knowledge
        .search
        .search(&devtoolbox_core::search::GlobalSearchQuery {
            query: query.to_string(),
            limit_per_source: params.limit.unwrap_or(5).clamp(1, 20),
            sources: Vec::new(),
        });
    to_json(result, "global_search_encode_failed")
}
/// 装配失败时的占位运行时：三个库仍然打开（保证进程可用），
/// 只是没有任何数据。**不静默**——调用 `startup_sync()` 时会在日志里说明。
#[must_use]
pub fn unavailable_runtime(
    config_dir: &std::path::Path,
) -> Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime> {
    let loader: devtoolbox_infrastructure::knowledge_runtime::SettingsLoader =
        Arc::new(|| Err("知识库未成功装配".to_string()));
    match build(&KnowledgeDeps {
        config_dir: config_dir.to_path_buf(),
        settings_loader: loader,
        language: Arc::new(devtoolbox_application::language::LanguageService::new(
            Arc::new(devtoolbox_infrastructure::ports::LanguageStoreAdapter::new(
                Arc::new(parking_lot::Mutex::new(
                    devtoolbox_infrastructure::language::LanguageStore::open(
                        config_dir.join("language.db"),
                    )
                    .expect("语言库始终可用（它是 server 的硬依赖）"),
                )),
            )),
        )),
    }) {
        Ok(runtime) => runtime,
        // 连兜底都失败时 panic：此时 server 本来也起不来，留在启动阶段暴露更诚实。
        Err(error) => panic!("知识库兜底装配失败：{error}"),
    }
}
