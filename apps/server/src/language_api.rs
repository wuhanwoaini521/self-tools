//! Language 的 HTTP 接口（网页端）。
//!
//! 桌面端走 `#[tauri::command]`，网页端走这里。两者调用的是**同一个**
//! `LanguageLearningService` / `LanguageService`，因此数据与字段完全一致——
//! 不会出现「网页端一份、桌面端另一份」的语义漂移。
//!
//! 路径与前端 `transport.ts` 的 `HTTP_ENDPOINTS` 一一对应。

use std::sync::Arc;

use devtoolbox_core::language::{LanguageCode, LanguageItemType, LearningItemType};
use devtoolbox_core::learning::ReviewRating;
use serde::{Deserialize, Serialize};

use devtoolbox_application::language::{LanguageLearningService, LanguageService};

/// 从扩展里的时钟取当前时间（所有写路径共用一个时间源，便于测试）。
pub(crate) fn now_from(clock: &Arc<dyn Fn() -> i64 + Send + Sync>) -> i64 {
    clock()
}

/// 错误契约：`{"code": .., "message": ..}`（与 history 侧保持一致）。
#[derive(Debug, Serialize)]
pub struct LanguageErrorBody {
    pub code: &'static str,
    pub message: String,
}

impl axum::response::IntoResponse for LanguageErrorBody {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code {
            "not_found" => axum::http::StatusCode::NOT_FOUND,
            "invalid" => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}

/// `GET /api/v1/language/study-queue?language=jpn&limit=20`
pub async fn study_queue(
    axum::Extension(service): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<StudyQueueQuery>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let language = parse_language(params.language.as_deref())?;
    let service = service
        .study_queue(language, params.limit.unwrap_or(20), now_from(&clock))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(service).map_err(
        |error| LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        },
    )?))
}

#[derive(Debug, Deserialize)]
pub struct StudyQueueQuery {
    pub language: Option<String>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/language/items/{id}` —— 学习条目（word / phrase / sentence / article）。
pub async fn learning_item(
    axum::Extension(content): axum::Extension<
        Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    >,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    match content.learning_item(&id).map_err(port_error)? {
        Some(item) => Ok(axum::Json(serde_json::to_value(item).map_err(|error| {
            LanguageErrorBody {
                code: "language_encode",
                message: error.to_string(),
            }
        })?)),
        None => Err(LanguageErrorBody {
            code: "not_found",
            message: format!("未找到学习对象：{id}"),
        }),
    }
}

/// `GET /api/v1/language/detail/{id}` —— 词典详情（多读音 / 释义 / 例句 / 来源）。
pub async fn item_detail(
    axum::Extension(dictionary): axum::Extension<Arc<LanguageService>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    match dictionary.detail(&id).map_err(to_error)? {
        Some(detail) => Ok(axum::Json(serde_json::to_value(detail).map_err(
            |error| LanguageErrorBody {
                code: "language_encode",
                message: error.to_string(),
            },
        )?)),
        None => Err(LanguageErrorBody {
            code: "not_found",
            message: format!("词典中没有这条内容：{id}"),
        }),
    }
}

/// `GET /api/v1/language/search?language=jpn&q=駅&limit=30`
pub async fn search(
    axum::Extension(dictionary): axum::Extension<Arc<LanguageService>>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let hits = dictionary
        .search(
            // 空串 = 不按语言过滤（前端 transport 会把缺省值发成 `language=`）。
            params
                .language
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
            &params.q,
            params.limit.unwrap_or(30),
        )
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(hits).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub language: Option<String>,
    pub q: String,
    pub limit: Option<usize>,
}

/// `GET /api/v1/language/sentences?language=jpn&limit=20`
pub async fn sentences(
    axum::Extension(dictionary): axum::Extension<Arc<LanguageService>>,
    axum::extract::Query(params): axum::extract::Query<LanguageQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = dictionary
        .sentences(
            params.language.as_deref().unwrap_or("jpn"),
            params.limit.unwrap_or(20),
        )
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `GET /api/v1/language/sentences/{id}/study` —— 句子学习视图（逐词拆解）。
pub async fn sentence_study(
    axum::Extension(service): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    match service.sentence_study(&id).map_err(to_error)? {
        Some(study) => Ok(axum::Json(serde_json::to_value(study).map_err(
            |error| LanguageErrorBody {
                code: "language_encode",
                message: error.to_string(),
            },
        )?)),
        None => Err(LanguageErrorBody {
            code: "not_found",
            message: format!("词典里没有这句话：{id}"),
        }),
    }
}

/// `GET /api/v1/language/lessons?language=jpn&limit=20`
pub async fn lessons(
    axum::Extension(service): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LanguageQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let code = params.language.as_deref().and_then(LanguageCode::from_code);
    let list = service
        .lessons(code, params.limit.unwrap_or(20))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `GET /api/v1/language/lessons/{id}` —— 课程 + 恢复位置 + 每步完整条目。
pub async fn lesson(
    axum::Extension(service): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let lesson = service
        .lesson(&id)
        .map_err(to_error)?
        .ok_or_else(|| LanguageErrorBody {
            code: "not_found",
            message: format!("课程不存在：{id}"),
        })?;
    let step_index = service.resume_step(&lesson);
    let items = service.lesson_steps(&lesson).map_err(to_error)?;
    Ok(axum::Json(serde_json::json!({
        "id": lesson.id,
        "title": lesson.title,
        "language": lesson.language,
        "description": lesson.description,
        "steps": lesson.steps,
        "created_at": lesson.created_at,
        "updated_at": lesson.updated_at,
        "step_index": step_index,
        "items": items,
    })))
}

/// `GET /api/v1/language/continue?limit=5`
pub async fn continue_lessons(
    axum::Extension(service): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = service
        .continue_lessons(params.limit.unwrap_or(5))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct LanguageQuery {
    pub language: Option<String>,
    pub limit: Option<usize>,
}

/// `POST /api/v1/language/study` —— 记录一次学习行为（view / study / complete）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyRequest {
    pub entity_id: String,
    pub action: String,
}

pub async fn record_study(
    axum::Extension(content): axum::Extension<
        Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    >,
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(request): axum::Json<StudyRequest>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let item = require_item(&content, &request.entity_id)?;
    let action = match request.action.as_str() {
        "view" => devtoolbox_application::language::StudyAction::View,
        "complete" => devtoolbox_application::language::StudyAction::Complete,
        _ => devtoolbox_application::language::StudyAction::Study,
    };
    let progress = learning
        .record_study(&item, action, now_from(&clock))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(progress).map_err(
        |error| LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        },
    )?))
}

/// `POST /api/v1/language/add-to-review`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddToReviewRequest {
    pub entity_id: String,
}

pub async fn add_to_review(
    axum::Extension(content): axum::Extension<
        Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    >,
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(request): axum::Json<AddToReviewRequest>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let item = require_item(&content, &request.entity_id)?;
    learning
        .add_to_review(&item, now_from(&clock))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/v1/language/review-queue?limit=20`
pub async fn review_queue(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let queue = learning
        .review_queue(params.limit.unwrap_or(20), now_from(&clock))
        .map_err(to_error)?;
    let _ = &clock;
    Ok(axum::Json(serde_json::to_value(queue).map_err(
        |error| LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        },
    )?))
}

/// `POST /api/v1/language/review` —— 提交复习评分（答错自动记错题，答对清除）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReviewRequest {
    pub card_id: String,
    pub rating: ReviewRating,
    #[serde(default)]
    pub user_answer: String,
}

pub async fn submit_review(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(request): axum::Json<SubmitReviewRequest>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let card = learning
        .review_card(&request.card_id)
        .map_err(to_error)?
        .ok_or_else(|| LanguageErrorBody {
            code: "not_found",
            message: format!("复习卡不存在：{}", request.card_id),
        })?;
    let outcome = learning
        .submit_review(
            &card,
            &request.user_answer,
            request.rating,
            now_from(&clock),
        )
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(outcome).map_err(
        |error| LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        },
    )?))
}

/// `GET /api/v1/language/mistakes?limit=50`
pub async fn mistakes(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = learning
        .mistakes(params.limit.unwrap_or(50))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `POST /api/v1/language/lessons` —— 建课（只引用条目 id）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLessonRequest {
    pub language: String,
    pub title: String,
    pub item_ids: Vec<String>,
}

pub async fn create_lesson(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(request): axum::Json<CreateLessonRequest>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let language = parse_language(Some(&request.language))?;
    if request.item_ids.is_empty() {
        return Err(LanguageErrorBody {
            code: "invalid",
            message: "Lesson 至少需要一个条目".to_string(),
        });
    }
    let lesson = learning
        .create_lesson(language, request.title, &request.item_ids, now_from(&clock))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(lesson).map_err(
        |error| LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        },
    )?))
}

/// `POST /api/v1/language/lessons/{id}/position` —— 保存学习位置（支持「继续学习」）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRequest {
    pub step_index: usize,
}

pub async fn save_lesson_position(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
    axum::Json(request): axum::Json<PositionRequest>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    learning
        .save_position(&id, request.step_index, now_from(&clock))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/v1/language/progress?limit=100`
pub async fn progress(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = learning
        .progress(params.limit.unwrap_or(100))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `GET /api/v1/language/weak-items?limit=8`
pub async fn weak_items(
    axum::Extension(learning): axum::Extension<Arc<LanguageLearningService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = learning
        .weak_items(params.limit.unwrap_or(8))
        .map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `GET /api/v1/language/languages`
pub async fn languages(
    axum::Extension(dictionary): axum::Extension<Arc<LanguageService>>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = dictionary.languages().map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

/// `GET /api/v1/language/sources` —— 词库来源与许可证（网页端也要能看数据出处）。
pub async fn sources(
    axum::Extension(dictionary): axum::Extension<Arc<LanguageService>>,
) -> Result<axum::Json<serde_json::Value>, LanguageErrorBody> {
    let list = dictionary.sources().map_err(to_error)?;
    Ok(axum::Json(serde_json::to_value(list).map_err(|error| {
        LanguageErrorBody {
            code: "language_encode",
            message: error.to_string(),
        }
    })?))
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn parse_language(raw: Option<&str>) -> Result<LanguageCode, LanguageErrorBody> {
    match raw {
        None => Ok(LanguageCode::Jap),
        Some(value) => LanguageCode::from_code(value).ok_or_else(|| LanguageErrorBody {
            code: "invalid",
            message: format!("未知语言代码：{value}"),
        }),
    }
}

fn require_item(
    content: &Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    entity_id: &str,
) -> Result<devtoolbox_core::language::LanguageLearningItem, LanguageErrorBody> {
    content
        .learning_item(entity_id)
        .map_err(port_error)?
        .ok_or_else(|| LanguageErrorBody {
            code: "not_found",
            message: format!("未找到学习对象：{entity_id}"),
        })
}

/// 端口层错误（`Result<_, String>`）→ HTTP 错误体。
fn port_error(message: String) -> LanguageErrorBody {
    LanguageErrorBody {
        code: "language_error",
        message,
    }
}

fn to_error(error: devtoolbox_application::error::ApplicationError) -> LanguageErrorBody {
    LanguageErrorBody {
        code: "language_error",
        message: error.to_string(),
    }
}

// 让未使用的类型也参与文档化检查（学习条目类型是这些端点的契约一部分）。
const _: Option<LanguageItemType> = None;
const _: Option<LearningItemType> = None;
