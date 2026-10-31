//! Learning OS 的 HTTP 接口（网页端）。
//!
//! 此前网页端只能用 History / Language / News —— 而 Collections、Graph、Review
//! Center、Home 的今日面板全都走 `learning_*` 命令，网页端一个都调不到，
//! 于是这些页面在浏览器里永远是空的。
//!
//! 与桌面端调用**同一个** `LearningService`，返回同一批 Rust 结构体。

use std::sync::Arc;

use devtoolbox_application::learning::LearningService;
use devtoolbox_core::learning::{KnowledgeReviewCardDraft, LearningEvent, ReviewRating};
use serde::{Deserialize, Serialize};

use crate::ai_api::ApiError;

type Shared = Arc<LearningService>;

fn ok<T: Serialize>(value: T) -> Result<axum::Json<serde_json::Value>, ApiError> {
    serde_json::to_value(value)
        .map(axum::Json)
        .map_err(|error| ApiError {
            code: "encode_failed",
            message: error.to_string(),
        })
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "invalid",
        message: message.into(),
    }
}

/// 取当前时间（由扩展注入的时钟）。
fn now(clock: &Arc<dyn Fn() -> i64 + Send + Sync>) -> i64 {
    clock()
}

// ---------------------------------------------------------------- Progress

/// `GET /api/v1/learning/progress?entityKey=…`
pub async fn get_progress(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Query(params): axum::extract::Query<EntityKeyQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.get_progress(&params.entity_key).map_err(backend)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityKeyQuery {
    pub entity_key: String,
}

/// 把 `?module=` 的**空字符串**归一成 `None`（= 不按模块过滤）。
///
/// 前端 transport 对可选参数统一发送 `module=${value ?? ""}`，于是「全部模块」
/// 会变成 `module=`。若原样传给存储层，等于按空模块名过滤 → 队列为空，
/// 而统计走的是另一条查询（不过滤）→ 出现
/// **「统计说有 N 张要复习，界面却说已全部完成」**的自相矛盾。
fn normalize_module(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

/// `GET /api/v1/learning/progress?module=…&status=…&limit=…`
pub async fn list_progress(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Query(params): axum::extract::Query<ListProgressQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    use devtoolbox_core::learning::LearningStatus;
    let status = params.status.as_deref().map(LearningStatus::parse);
    let list = service
        .list_progress(
            normalize_module(params.module.as_deref()),
            status,
            params.limit.unwrap_or(50).clamp(1, 500),
        )
        .map_err(backend)?;
    ok(list)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProgressQuery {
    pub module: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// `POST /api/v1/learning/events` —— 记录学习行为。
#[derive(Debug, Deserialize)]
pub struct RecordEventBody {
    /// 透传前端构造的 `LearningEvent`（id / timestamp 缺省由服务层归一化）。
    pub event: serde_json::Value,
}

pub async fn record_event(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<RecordEventBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let event: LearningEvent = serde_json::from_value(body.event)
        .map_err(|error| bad(format!("学习事件格式不正确：{error}")))?;
    ok(service.record_event(&event, now(&clock)).map_err(backend)?)
}

// ---------------------------------------------------------------- Review

/// `GET /api/v1/learning/review/queue?module=…&limit=…`
pub async fn review_queue(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::extract::Query(params): axum::extract::Query<ModuleQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let queue = service
        .get_review_queue(
            normalize_module(params.module.as_deref()),
            now(&clock),
            params.limit.unwrap_or(30).clamp(1, 200),
        )
        .map_err(backend)?;
    ok(queue)
}

/// `GET /api/v1/learning/review/stats`
pub async fn review_stats(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.get_review_stats(now(&clock)).map_err(backend)?)
}

/// `POST /api/v1/learning/review` —— 提交评分。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReviewBody {
    pub card_id: String,
    pub rating: ReviewRating,
    pub request_id: Option<String>,
}

pub async fn submit_review(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<SubmitReviewBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let outcome = match body.request_id.as_deref() {
        Some(request_id) => {
            service.submit_review_once(&body.card_id, body.rating, request_id, now(&clock))
        }
        None => service.submit_review(&body.card_id, body.rating, now(&clock)),
    }
    .map_err(backend)?;
    ok(outcome)
}

/// `GET /api/v1/learning/review/cards/{id}`
pub async fn get_review_card(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    service
        .get_review_card(&id)
        .map_err(backend)?
        .map(ok)
        .unwrap_or_else(|| Err(not_found("review card", &id)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveKnowledgeReviewCardBody {
    pub draft: KnowledgeReviewCardDraft,
}

/// `POST /api/v1/learning/review/cards/from-knowledge`
pub async fn save_knowledge_review_card(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<SaveKnowledgeReviewCardBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service
        .save_knowledge_review_card(&body.draft, now(&clock))
        .map_err(backend)?)
}

/// `DELETE /api/v1/learning/review/cards/{id}`
pub async fn delete_knowledge_review_card(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    service.delete_knowledge_review_card(&id).map_err(backend)?;
    ok(serde_json::json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleQuery {
    pub module: Option<String>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------- Today / Graph / Explore

/// `GET /api/v1/learning/today`
pub async fn today(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.get_today_dashboard(now(&clock)).map_err(backend)?)
}

/// `GET /api/v1/learning/graph?rootId=…&hops=1`
pub async fn graph(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Query(params): axum::extract::Query<GraphQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service
        .get_knowledge_graph(params.root_id.as_deref(), params.hops.unwrap_or(1))
        .map_err(backend)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQuery {
    pub root_id: Option<String>,
    pub hops: Option<u32>,
}

/// `GET /api/v1/learning/explore?limit=…`
pub async fn explore(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Query(params): axum::extract::Query<ModuleQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service
        .get_explore_recommendations(params.limit.unwrap_or(6).clamp(1, 50))
        .map_err(backend)?)
}

// ---------------------------------------------------------------- Collections

/// `GET /api/v1/learning/collections`
pub async fn list_collections(
    axum::Extension(service): axum::Extension<Shared>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.list_collections().map_err(backend)?)
}

/// `POST /api/v1/learning/collections`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionBody {
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub async fn create_collection(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<CreateCollectionBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    if body.title.trim().is_empty() {
        return Err(bad("合集标题不能为空"));
    }
    ok(service
        .create_collection(
            &body.title,
            body.description.as_deref(),
            &body.tags,
            now(&clock),
        )
        .map_err(backend)?)
}

/// `DELETE /api/v1/learning/collections/{id}`
pub async fn delete_collection(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    service.delete_collection(&id).map_err(backend)?;
    ok(serde_json::json!({ "ok": true }))
}

/// `GET /api/v1/learning/collections/{id}/items`
pub async fn list_collection_items(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.list_collection_items(&id).map_err(backend)?)
}

/// `POST /api/v1/learning/collections/{id}/items`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCollectionItemBody {
    pub module: String,
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub note: Option<String>,
}

pub async fn add_collection_item(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<AddCollectionItemBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service
        .add_collection_item(
            &id,
            devtoolbox_core::learning::CollectionItemRef {
                module: body.module,
                entity_type: body.entity_type,
                entity_id: body.entity_id,
                title: body.title,
                note: body.note,
            },
            now(&clock),
        )
        .map_err(backend)?)
}

/// `PATCH /api/v1/learning/collection-items/{id}/note`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollectionItemNoteBody {
    #[serde(default)]
    pub note: Option<String>,
}

pub async fn update_collection_item_note(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(body): axum::Json<UpdateCollectionItemNoteBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service
        .update_collection_item_note(&id, body.note.as_deref(), now(&clock))
        .map_err(backend)?)
}

/// `POST /api/v1/learning/collection-items/{itemId}/remove`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveItemBody {
    pub item_id: String,
}

pub async fn remove_collection_item(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Json(body): axum::Json<RemoveItemBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    service
        .remove_collection_item(&body.item_id)
        .map_err(backend)?;
    ok(serde_json::json!({ "ok": true }))
}

fn backend(error: devtoolbox_application::learning::LearningPortError) -> ApiError {
    let code = match &error {
        devtoolbox_application::learning::LearningPortError::NotFound(_) => "not_found",
        devtoolbox_application::learning::LearningPortError::InvalidArgument(_) => "invalid",
        devtoolbox_application::learning::LearningPortError::Store(_) => "learning_failed",
    };
    ApiError {
        code,
        message: error.to_string(),
    }
}

fn not_found(kind: &str, id: &str) -> ApiError {
    ApiError {
        code: "not_found",
        message: format!("{kind} not found: {id}"),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use axum::middleware;
    use axum::routing::{get, patch, post};
    use axum::{Extension, Router};
    use parking_lot::Mutex;
    use tower::ServiceExt;

    use super::*;

    /// 回归：前端 transport 对可选参数统一发 `module=${value ?? ""}`。
    /// 「全部模块」因此是 `module=`（空串）。若不归一成 `None`，
    /// 队列按空模块名过滤返回空，而统计不过滤 →
    /// **「今日待复习 3 张」却显示「已全部完成」**。
    #[test]
    fn empty_module_means_all_modules() {
        assert_eq!(normalize_module(Some("")), None);
        assert_eq!(normalize_module(Some("   ")), None);
        assert_eq!(normalize_module(None), None);
        assert_eq!(normalize_module(Some("language")), Some("language"));
    }

    #[tokio::test]
    async fn knowledge_card_http_round_trip_persists_source_and_review_progress() {
        let store = Arc::new(Mutex::new(
            devtoolbox_infrastructure::LearningStore::open_in_memory().expect("open test DB"),
        ));
        let service = Arc::new(LearningService::new(Arc::new(
            devtoolbox_infrastructure::LearningStoreAdapter::new(store),
        )));
        let clock: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(|| 1_700_000_000);
        let app = Router::new()
            .route("/api/v1/learning/review", post(submit_review))
            .route(
                "/api/v1/learning/review/cards/from-knowledge",
                post(save_knowledge_review_card),
            )
            .route(
                "/api/v1/learning/review/cards/{id}",
                get(get_review_card).delete(delete_knowledge_review_card),
            )
            .route("/api/v1/learning/progress/key", get(get_progress))
            .route(
                "/api/v1/learning/collection-items/{id}/note",
                patch(update_collection_item_note),
            )
            .layer(Extension(clock))
            .layer(Extension(service.clone()))
            .layer(middleware::from_fn(crate::security::local_http_guard));

        let source_id = "event-anlu-luoyang";
        let card_id = "knowledge:history:event:event-anlu-luoyang";
        let save_body = serde_json::json!({
            "draft": {
                "module": "history",
                "entityType": "event",
                "entityId": source_id,
                "sourceTitle": "洛阳失守",
                "prompt": "叛军何时攻陷洛阳？",
                "answer": "公元 756 年。",
                "userNote": "个人补充"
            }
        });
        let saved = request_json(
            &app,
            Method::POST,
            "/api/v1/learning/review/cards/from-knowledge",
            Some(save_body.clone()),
            true,
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK);
        let saved_card = saved.body;
        assert_eq!(saved_card["id"], card_id);
        assert_eq!(saved_card["source_title"], "洛阳失守");
        assert_eq!(saved_card["user_note"], "个人补充");

        let review_body = serde_json::json!({
            "cardId": card_id,
            "rating": "good",
            "requestId": "browser-review-123"
        });
        let first = request_json(
            &app,
            Method::POST,
            "/api/v1/learning/review",
            Some(review_body.clone()),
            true,
        )
        .await;
        assert_eq!(first.status, StatusCode::OK);
        assert_eq!(first.body["repetition_count"], 1);
        let retry = request_json(
            &app,
            Method::POST,
            "/api/v1/learning/review",
            Some(review_body),
            true,
        )
        .await;
        assert_eq!(retry.status, StatusCode::OK);
        assert_eq!(retry.body, first.body, "retry returns its original receipt");

        let progress = request_json(
            &app,
            Method::GET,
            "/api/v1/learning/progress/key?entityKey=history%3Aevent%3Aevent-anlu-luoyang",
            None,
            false,
        )
        .await;
        assert_eq!(progress.status, StatusCode::OK);
        assert_eq!(progress.body["review_count"], 1);
        assert_eq!(progress.body["correct_count"], 1);
        assert_eq!(progress.body["next_review_at"], first.body["due_at"]);

        let collection = service
            .create_collection("洛阳验收", None, &[], 1_700_000_000)
            .expect("create collection");
        let collection_item = service
            .add_collection_item(
                &collection.id,
                devtoolbox_core::learning::CollectionItemRef {
                    module: "history".to_string(),
                    entity_type: "event".to_string(),
                    entity_id: source_id.to_string(),
                    title: "洛阳失守".to_string(),
                    note: Some("原始合集备注".to_string()),
                },
                1_700_000_000,
            )
            .expect("save collection item");
        let updated_note = request_json(
            &app,
            Method::PATCH,
            &format!(
                "/api/v1/learning/collection-items/{}/note",
                collection_item.id
            ),
            Some(serde_json::json!({ "note": "更新后的个人备注" })),
            true,
        )
        .await;
        assert_eq!(updated_note.status, StatusCode::OK);
        assert_eq!(updated_note.body["id"], collection_item.id);
        assert_eq!(updated_note.body["entity_id"], source_id);
        assert_eq!(updated_note.body["note"], "更新后的个人备注");
        let denied_note_update = request_json(
            &app,
            Method::PATCH,
            &format!(
                "/api/v1/learning/collection-items/{}/note",
                collection_item.id
            ),
            Some(serde_json::json!({ "note": "未经授权" })),
            false,
        )
        .await;
        assert_eq!(denied_note_update.status, StatusCode::FORBIDDEN);

        let source = request_json(
            &app,
            Method::GET,
            &format!("/api/v1/learning/review/cards/{card_id}"),
            None,
            false,
        )
        .await;
        assert_eq!(source.status, StatusCode::OK);
        assert_eq!(source.body["entity_id"], source_id);
        assert_eq!(source.body["repetition_count"], 1);

        let deleted = request_json(
            &app,
            Method::DELETE,
            &format!("/api/v1/learning/review/cards/{card_id}"),
            None,
            true,
        )
        .await;
        assert_eq!(deleted.status, StatusCode::OK);
        let missing = request_json(
            &app,
            Method::GET,
            &format!("/api/v1/learning/review/cards/{card_id}"),
            None,
            false,
        )
        .await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);

        let denied = request_json(
            &app,
            Method::POST,
            "/api/v1/learning/review/cards/from-knowledge",
            Some(save_body),
            false,
        )
        .await;
        assert_eq!(
            denied.status,
            StatusCode::FORBIDDEN,
            "browser writes require loopback Origin"
        );
    }

    struct JsonResponse {
        status: StatusCode,
        body: serde_json::Value,
    }

    async fn request_json(
        app: &Router,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
        include_origin: bool,
    ) -> JsonResponse {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1:1420")
            .header(header::CONTENT_TYPE, "application/json");
        if include_origin {
            builder = builder.header(header::ORIGIN, "http://127.0.0.1:1420");
        }
        let request = builder
            .body(
                body.map(|value| Body::from(value.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .expect("build request");
        let response = app.clone().oneshot(request).await.expect("route response");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let body = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("JSON response")
        };
        JsonResponse { status, body }
    }
}
