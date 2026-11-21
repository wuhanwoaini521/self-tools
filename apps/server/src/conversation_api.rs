//! 会话历史（V11-K）的 HTTP 接口。
//!
//! 与桌面端 `conversation_*` 命令共用同一个 `ConversationService` 与
//! `config/conversations.db`：pad 与桌面看到同一份回放记录。
//!
//! **Conversation ≠ Memory**：这里只做「会话回放」，绝不参与记忆写入或检索。
//! 业务策略（正文截断、空标题归一、归档语义）全部在应用层实现，HTTP 层只做
//! 路由、入参归一与错误契约转换。

use std::sync::Arc;

use axum::extract::{Path, Query};
use axum::{Extension, Json};
use devtoolbox_application::ApplicationError;
use devtoolbox_application::personal_ai::ConversationService;
use devtoolbox_core::personal_ai::conversation::{
    Conversation, ConversationMessage, ConversationRole, ConversationSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai_api::ApiError;

/// 桌面端 `conversation_list` 的默认上限（`limit` 缺省时用它）。
const DEFAULT_LIST_LIMIT: usize = 20;

// ============================================================================
// DTO
// ============================================================================

#[derive(Debug, Deserialize, Serialize)]
pub struct ConversationListQuery {
    pub limit: Option<usize>,
    /// 桌面端命令参数是 `includeArchived`（Tauri camelCase），查询串两种写法都收。
    #[serde(default, alias = "includeArchived")]
    pub include_archived: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ConversationCreateDto {
    #[serde(default)]
    pub title: Option<String>,
}

/// PATCH 请求体：`{title?, archived?}`（至少给一个；都给 null 视为无操作）。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ConversationUpdateDto {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConversationMessageDto {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

// ============================================================================
// 序列化（与桌面端命令的 JSON 形状一致）
// ============================================================================

fn summary_json(item: &ConversationSummary) -> Value {
    serde_json::json!({
        "conversation_id": item.conversation_id,
        "title": item.title,
        "module_origin": item.module_origin,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "message_count": item.message_count,
        "archived": item.archived,
    })
}

fn conversation_json(conversation: &Conversation) -> Value {
    serde_json::json!({
        "conversation_id": conversation.conversation_id,
        "title": conversation.title,
        "module_origin": conversation.module_origin,
        "created_at": conversation.created_at,
        "updated_at": conversation.updated_at,
        "archived": conversation.archived,
        "messages": conversation
            .messages
            .iter()
            .map(|message| serde_json::json!({
                "role": message.role.as_str(),
                "content": message.content,
                "provider": message.provider,
                "model": message.model,
                "created_at": message.created_at,
            }))
            .collect::<Vec<_>>(),
    })
}

/// 会话错误 → HTTP 错误契约。
///
/// 用例层把存储错误收敛成 `PersonalAi(Session)`，这里沿用它的稳定 code
/// （`personal_ai_session_error`），前端分类语义与桌面端一致。
fn conversation_error(error: ApplicationError) -> ApiError {
    match error {
        ApplicationError::PersonalAi(agent) => ApiError {
            code: agent.code(),
            message: agent.to_string(),
        },
        other => ApiError {
            code: "conversation_error",
            message: other.to_string(),
        },
    }
}

// ============================================================================
// 端点
// ============================================================================

/// `GET /api/v1/conversations?limit&includeArchived`
pub async fn list_conversations(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Query(params): Query<ConversationListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, 100);
    let items = if params.include_archived.unwrap_or(false) {
        conversation.list_all(limit)
    } else {
        conversation.list(limit)
    }
    .map_err(conversation_error)?;
    Ok(Json(Value::Array(items.iter().map(summary_json).collect())))
}

/// `GET /api/v1/conversations/{id}`
///
/// 不存在返回 404（前端传输层把它映射成 `null`，抽屉据此提示「会话不存在」）。
pub async fn load_conversation(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let loaded = conversation.load(&id).map_err(conversation_error)?;
    let conversation = loaded.ok_or_else(|| ApiError {
        code: "not_found",
        message: "会话不存在".to_string(),
    })?;
    Ok(Json(conversation_json(&conversation)))
}

/// `POST /api/v1/conversations`
pub async fn create_conversation(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Json(request): Json<ConversationCreateDto>,
) -> Result<Json<Value>, ApiError> {
    let created = conversation
        .create(request.title.as_deref().unwrap_or(""), None)
        .map_err(conversation_error)?;
    Ok(Json(serde_json::json!({
        "conversation_id": created.conversation_id,
        "title": created.title,
    })))
}

/// `PATCH /api/v1/conversations/{id}`：`{title?, archived?}`。
pub async fn update_conversation(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Path(id): Path<String>,
    Json(request): Json<ConversationUpdateDto>,
) -> Result<Json<Value>, ApiError> {
    if let Some(title) = request.title {
        conversation
            .rename(&id, &title)
            .map_err(conversation_error)?;
    }
    if let Some(archived) = request.archived {
        conversation
            .set_archived(&id, archived)
            .map_err(conversation_error)?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/v1/conversations/{id}/messages`
pub async fn append_message(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Path(id): Path<String>,
    Json(request): Json<ConversationMessageDto>,
) -> Result<Json<Value>, ApiError> {
    let normalized = request.role.trim().to_ascii_lowercase();
    let role = match normalized.as_str() {
        "user" => ConversationRole::User,
        "assistant" => ConversationRole::Assistant,
        "tool" => ConversationRole::Tool,
        other => {
            return Err(ApiError {
                code: "conversation_invalid_role",
                message: format!("未知消息角色：{other}"),
            });
        }
    };
    let mut message = ConversationMessage::new(role, request.content);
    message.provider = request.provider;
    message.model = request.model;
    conversation
        .append(&id, message)
        .map_err(conversation_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /api/v1/conversations/{id}`
pub async fn delete_conversation(
    Extension(conversation): Extension<Arc<ConversationService>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    conversation.delete(&id).map_err(conversation_error)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
