//! 路由与序列化（Gate 9）：HTTP 边界只做路由 / 参数提取 / 错误契约转换，
//! 用例决策全部在 `devtoolbox_application::history::HistoryService`。
//!
//! - 只读：七个 History 端点 + `/health`，无任何写入端点；
//! - 无鉴权（Gate 9 边界）；不设置 CORS —— 默认无跨源放行，仅当显式配置来源
//!   时才可能有允许列表（当前交付不做）；
//! - 错误契约：`{"code": "...", "message": "..."}`，由 `ApplicationError`
//!   映射而来；HTTP 层不出现桌面端的 `CommandError`。

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::json;
use tracing::warn;

use devtoolbox_application::ApplicationError;
use devtoolbox_application::history::HistoryService;

/// 共享状态（组合根在 main.rs 装配；路由不感知存储细节）。
#[derive(Clone)]
struct AppState {
    service: Arc<HistoryService>,
}

/// 错误契约响应体：`{"code": .., "message": ..}`。
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// 组装全部路由：History + Language + English 课程（由组合根注入服务与存储）。
///
/// 参数多是**组合根的固有形状**（每个 bounded context 一个已装配好的服务），
/// 与 `personal_ai::build_hub` 等同一类；拆成 struct 只会把装配步骤挪个地方。
#[allow(clippy::too_many_arguments)]
#[must_use = "router must be served"]
pub fn router(
    service: Arc<HistoryService>,
    learning: Arc<devtoolbox_application::language::LanguageLearningService>,
    dictionary: Arc<devtoolbox_application::language::LanguageService>,
    content: Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    // 英语课程（NCE）只读接口（写操作属桌面端）。
    course: Arc<devtoolbox_application::language::course::CourseService>,
    // 数据目录（readiness 探测用；与桌面端同一目录）。
    data_dir: std::path::PathBuf,
    // Personal Knowledge 运行时（两端共用同一份索引库）。
    knowledge: Arc<devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime>,
    // 语言库（课时音频端点要直接读文件；路径只取自数据库，不接受前端传参）。
    language_store: Arc<parking_lot::Mutex<devtoolbox_infrastructure::language::LanguageStore>>,
    settings: Arc<dyn crate::ai_api::SettingsAccess>,
    // 平台 LearningService（Collections / Graph / Review Center / Home 今日面板）
    learning_os: Arc<devtoolbox_application::learning::LearningService>,
    geography: Arc<devtoolbox_application::geography::GeographyService>,
    news: Arc<devtoolbox_application::news::NewsService>,
    news_ingest: Arc<dyn devtoolbox_application::news::NewsIngestPort>,
) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/api/v1/readiness",
            get(crate::readiness_api::readiness_report),
        )
        .route(
            "/api/v1/readiness/diagnostics",
            get(crate::readiness_api::readiness_diagnostics),
        )
        // Personal Knowledge（Memory / Documents / Files / 全局检索）：
        // 能力一直在 Rust 侧实现，但此前只有桌面装配，网页端只能显示「不支持」。
        .route(
            "/api/v1/memory/list",
            get(crate::knowledge_api::memory_list),
        )
        .route(
            "/api/v1/memory/stats",
            get(crate::knowledge_api::memory_stats),
        )
        .route(
            "/api/v1/documents/status",
            get(crate::knowledge_api::documents_status),
        )
        .route(
            "/api/v1/documents/recent",
            get(crate::knowledge_api::documents_recent),
        )
        .route(
            "/api/v1/documents/search",
            get(crate::knowledge_api::documents_search),
        )
        .route(
            "/api/v1/files/status",
            get(crate::knowledge_api::files_status),
        )
        .route(
            "/api/v1/files/recent",
            get(crate::knowledge_api::files_recent),
        )
        .route(
            "/api/v1/files/search",
            get(crate::knowledge_api::files_search),
        )
        .route(
            "/api/v1/search/global",
            get(crate::knowledge_api::global_search),
        )
        .route("/api/v1/history/home", get(home))
        .route("/api/v1/history/search", get(search))
        .route("/api/v1/history/periods/{id}", get(period))
        .route("/api/v1/history/events/{id}", get(event))
        .route("/api/v1/history/people/{id}", get(person))
        .route("/api/v1/history/works/{id}", get(work))
        .route("/api/v1/history/stories/{id}", get(story))
        .route(
            "/api/v1/language/languages",
            get(crate::language_api::languages),
        )
        .route(
            "/api/v1/language/sources",
            get(crate::language_api::sources),
        )
        .route(
            "/api/v1/language/course/today",
            get(crate::language_course_api::today),
        )
        .route(
            "/api/v1/language/course/books",
            get(crate::language_course_api::books),
        )
        .route(
            "/api/v1/language/course/book/{id}",
            get(crate::language_course_api::book),
        )
        .route(
            "/api/v1/language/course/lesson/{id}",
            get(crate::language_course_api::lesson),
        )
        .route(
            "/api/v1/language/course/progress",
            get(crate::language_course_api::progress),
        )
        .route(
            "/api/v1/language/course/plan",
            get(crate::language_course_api::plan),
        )
        .route(
            "/api/v1/language/course/dict/{word}",
            get(crate::language_course_api::dict_lookup),
        )
        // 写路径：网页端要能真正「学」，不只是「看」。
        // 此前这里只有只读端点，点「不认识」直接报「尚无网页端接口」。
        .route(
            "/api/v1/language/course/lesson/{lesson_id}/progress",
            post(crate::language_write_api::update_progress),
        )
        .route(
            "/api/v1/language/course/lesson/{lesson_id}/complete",
            post(crate::language_write_api::complete_lesson),
        )
        .route(
            "/api/v1/language/course/lesson/{lesson_id}/mark-word",
            post(crate::language_write_api::mark_word),
        )
        .route(
            "/api/v1/language/course/lookup-word",
            post(crate::language_write_api::lookup_word),
        )
        .route(
            "/api/v1/language/course/quiz",
            get(crate::language_write_api::quiz),
        )
        .route(
            "/api/v1/language/course/quiz/submit",
            post(crate::language_write_api::submit_quiz),
        )
        .route(
            "/api/v1/language/course/plan",
            post(crate::language_write_api::save_plan),
        )
        .route(
            "/api/v1/language/course/lesson/{lesson_id}/audio",
            get(crate::language_write_api::lesson_audio),
        )
        .route(
            "/api/v1/language/course/search",
            get(crate::language_course_api::search),
        )
        .route("/api/v1/language/search", get(crate::language_api::search))
        .route(
            "/api/v1/language/sentences",
            get(crate::language_api::sentences),
        )
        .route(
            "/api/v1/language/sentences/{id}/study",
            get(crate::language_api::sentence_study),
        )
        .route(
            "/api/v1/language/items/{id}",
            get(crate::language_api::learning_item),
        )
        .route(
            "/api/v1/language/detail/{id}",
            get(crate::language_api::item_detail),
        )
        .route(
            "/api/v1/language/study-queue",
            get(crate::language_api::study_queue),
        )
        .route(
            "/api/v1/language/study",
            post(crate::language_api::record_study),
        )
        .route(
            "/api/v1/language/add-to-review",
            post(crate::language_api::add_to_review),
        )
        .route(
            "/api/v1/language/review-queue",
            get(crate::language_api::review_queue),
        )
        .route(
            "/api/v1/language/review",
            post(crate::language_api::submit_review),
        )
        .route(
            "/api/v1/language/mistakes",
            get(crate::language_api::mistakes),
        )
        .route(
            "/api/v1/language/lessons",
            get(crate::language_api::lessons).post(crate::language_api::create_lesson),
        )
        .route(
            "/api/v1/language/lessons/{id}",
            get(crate::language_api::lesson),
        )
        .route(
            "/api/v1/language/lessons/{id}/position",
            post(crate::language_api::save_lesson_position),
        )
        .route(
            "/api/v1/language/continue",
            get(crate::language_api::continue_lessons),
        )
        .route(
            "/api/v1/language/progress",
            get(crate::language_api::progress),
        )
        .route(
            "/api/v1/language/weak-items",
            get(crate::language_api::weak_items),
        )
        // ---- 设置与 AI：网页端没有 Tauri IPC，但两端共用同一个 settings.json ----
        .route(
            "/api/v1/settings",
            get(crate::ai_api::get_settings).post(crate::ai_api::save_settings),
        )
        .route("/api/v1/ai/status", get(crate::ai_api::ai_status))
        .route("/api/v1/ai/chat", post(crate::ai_api::ai_chat))
        // ---- Learning OS：Collections / Graph / Review Center / Home 今日面板 ----
        .route(
            "/api/v1/learning/progress",
            get(crate::learning_api::list_progress).post(crate::learning_api::record_event),
        )
        .route(
            "/api/v1/learning/progress/key",
            get(crate::learning_api::get_progress),
        )
        .route(
            "/api/v1/learning/review/queue",
            get(crate::learning_api::review_queue),
        )
        .route(
            "/api/v1/learning/review/stats",
            get(crate::learning_api::review_stats),
        )
        .route(
            "/api/v1/learning/review",
            post(crate::learning_api::submit_review),
        )
        .route("/api/v1/learning/today", get(crate::learning_api::today))
        .route("/api/v1/learning/graph", get(crate::learning_api::graph))
        .route(
            "/api/v1/learning/explore",
            get(crate::learning_api::explore),
        )
        .route(
            "/api/v1/learning/collections",
            get(crate::learning_api::list_collections).post(crate::learning_api::create_collection),
        )
        .route(
            "/api/v1/learning/collections/{id}",
            axum::routing::delete(crate::learning_api::delete_collection),
        )
        .route(
            "/api/v1/learning/collections/{id}/items",
            get(crate::learning_api::list_collection_items)
                .post(crate::learning_api::add_collection_item),
        )
        .route(
            "/api/v1/learning/collection-items/remove",
            post(crate::learning_api::remove_collection_item),
        )
        // ---- Geography：此前网页端搜索走前端 12 条硬编码示例，不查真实库 ----
        .route("/api/v1/geography/home", get(crate::geography_api::home))
        .route(
            "/api/v1/geography/search",
            get(crate::geography_api::search),
        )
        .route(
            "/api/v1/geography/entities/{id}",
            get(crate::geography_api::detail),
        )
        .route(
            "/api/v1/geography/favorite",
            post(crate::geography_api::toggle_favorite),
        )
        // ---- News：推荐源目录是 core 里的纯函数，网页端同样要能读到 ----
        .route(
            "/api/v1/news/recommended",
            get(crate::news_api::recommended),
        )
        .route("/api/v1/news/sources", get(crate::news_api::sources))
        .route("/api/v1/news/headlines", get(crate::news_api::headlines))
        .route("/api/v1/news/search", get(crate::news_api::search))
        .route("/api/v1/news/starred", get(crate::news_api::starred))
        .route("/api/v1/news/sources", post(crate::news_api::add_source))
        .route(
            "/api/v1/news/sources/{id}/remove",
            post(crate::news_api::remove_source),
        )
        .route(
            "/api/v1/news/articles/{id}/star",
            post(crate::news_api::toggle_star),
        )
        .route(
            "/api/v1/news/articles/{id}/read",
            post(crate::news_api::mark_read),
        )
        .layer(axum::Extension(Arc::clone(&content)))
        .layer(axum::Extension(Arc::clone(&dictionary)))
        .layer(axum::Extension(Arc::clone(&learning)))
        .layer(axum::Extension(Arc::clone(&settings)))
        .layer(axum::Extension(Arc::clone(&learning_os)))
        .layer(axum::Extension(Arc::clone(&geography)))
        .layer(axum::Extension(Arc::clone(&news)))
        .layer(axum::Extension(Arc::clone(&course)))
        .layer(axum::Extension(Arc::new(data_dir)))
        .layer(axum::Extension(knowledge))
        .layer(axum::Extension(language_store))
        .layer(axum::Extension(Arc::clone(&news_ingest)))
        .layer(axum::Extension(
            None::<Arc<dyn crate::ai_api::AiChatRunner>>,
        ))
        .with_state(AppState { service })
        // 显式擦除成 `Arc<dyn Fn()>`：`Extension<T>` 按具体类型匹配，
        // `Arc<fn() -> i64>` 与 `Arc<dyn Fn() -> i64 + Send + Sync>` 不是同一个类型。
        .layer(axum::Extension(
            Arc::new(devtoolbox_infrastructure::now_unix as fn() -> i64)
                as Arc<dyn Fn() -> i64 + Send + Sync>,
        ))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// 客户端参数问题（契约：与用例错误同形的 `{"code","message"}`）。
fn bad_request(message: impl Into<String>) -> Response {
    error_response(StatusCode::BAD_REQUEST, message.into())
}

/// 查询不存在（用例返回 `Ok(None)`）→ 404。
fn not_found(kind: &str, id: &str) -> Response {
    error_response(StatusCode::NOT_FOUND, format!("{kind} not found: {id}"))
}

/// 用例层失败 → 500（只读查询没有可恢复的客户端错误）。
fn use_case_error(error: ApplicationError) -> Response {
    warn!(error = %error, "history request failed");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn error_response(status: StatusCode, message: String) -> Response {
    (
        status,
        Json(ErrorBody {
            code: "history_error".to_string(),
            message,
        }),
    )
        .into_response()
}

fn ok_json(value: impl Serialize) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

async fn home(State(state): State<AppState>) -> Response {
    match state.service.home() {
        Ok(view) => ok_json(view),
        Err(error) => use_case_error(error),
    }
}

/// `GET /api/v1/history/search?q=…`；`q` 缺失 → 400 错误契约，`q` 为空 → 空结果。
/// （axum 0.8 没有 `Option<Query<T>>` 提取器，这里用 map 提取后自行判空。）
async fn search(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(q) = params.get("q") else {
        return bad_request("missing query parameter: q");
    };
    match state.service.search(q) {
        Ok(groups) => ok_json(groups),
        Err(error) => use_case_error(error),
    }
}

async fn period(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.service.period_detail(&id) {
        Ok(Some(detail)) => ok_json(detail),
        Ok(None) => not_found("period", &id),
        Err(error) => use_case_error(error),
    }
}

async fn event(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.service.event_detail(&id) {
        Ok(Some(detail)) => ok_json(detail),
        Ok(None) => not_found("event", &id),
        Err(error) => use_case_error(error),
    }
}

async fn person(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.service.person_detail(&id) {
        Ok(Some(detail)) => ok_json(detail),
        Ok(None) => not_found("person", &id),
        Err(error) => use_case_error(error),
    }
}

async fn work(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.service.work_detail(&id) {
        Ok(Some(detail)) => ok_json(detail),
        Ok(None) => not_found("work", &id),
        Err(error) => use_case_error(error),
    }
}

async fn story(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.service.story_detail(&id) {
        Ok(Some(detail)) => ok_json(detail),
        Ok(None) => not_found("story", &id),
        Err(error) => use_case_error(error),
    }
}

#[cfg(test)]
mod tests {
    //! 路由 + 用例服务集成测试（黑盒，不监听真实端口：`oneshot` 直接调用）。

    use std::sync::Arc;

    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use devtoolbox_application::history::{HistoryPortError, HistoryQueryPort, HistoryService};
    use devtoolbox_core::history_records::{
        DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
        EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
        PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
        PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
        WorkResult,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::router;

    fn boom() -> HistoryPortError {
        HistoryPortError("boom".to_string())
    }

    /// 灌入空数据的假端口（明细全部未命中 → 404；`fail_all` → 500）。
    struct FakeHistory {
        fail_all: bool,
    }

    impl HistoryQueryPort for FakeHistory {
        fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(DatasetStats {
                    people: 0,
                    places: 0,
                    person_relations: 0,
                    person_places: 0,
                    works: 0,
                    historical_texts: 0,
                    events: 0,
                    periods: 0,
                    regimes: 0,
                    stories: 0,
                    event_relations: 0,
                    event_evidences: 0,
                })
            }
        }
        fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_regimes_by_period(&self, _: &str) -> Result<Vec<RegimeResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_events_for_period(&self, _: &str) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_people_for_period(
            &self,
            _: &str,
            _: i64,
        ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_relations_for_period(
            &self,
            _: &str,
        ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_stories_for_period(
            &self,
            _: Option<&str>,
        ) -> Result<Vec<StoryResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_story(&self, _: &str) -> Result<Option<StoryResult>, HistoryPortError> {
            if self.fail_all { Err(boom()) } else { Ok(None) }
        }
        fn get_story_events(&self, _: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_story_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_story_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_story_texts(
            &self,
            _: &str,
        ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_story_evidences(
            &self,
            _: &str,
        ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_event(&self, _: &str) -> Result<Option<EventResult>, HistoryPortError> {
            if self.fail_all { Err(boom()) } else { Ok(None) }
        }
        fn get_event_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_event_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_event_relations(
            &self,
            _: &str,
        ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_event_texts(
            &self,
            _: &str,
        ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_event_evidences(
            &self,
            _: &str,
        ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_person(&self, _: &str) -> Result<Option<PersonResult>, HistoryPortError> {
            if self.fail_all { Err(boom()) } else { Ok(None) }
        }
        fn get_person_relations(
            &self,
            _: &str,
        ) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_person_places(&self, _: &str) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_person_events(&self, _: &str) -> Result<Vec<PersonEventResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_person_stories(&self, _: &str) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_work_by_id(&self, _: &str) -> Result<Option<WorkResult>, HistoryPortError> {
            if self.fail_all { Err(boom()) } else { Ok(None) }
        }
        fn get_work(&self, _: &str, _: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_historical_texts(
            &self,
            _: Option<&str>,
            _: i64,
        ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn search_people(&self, _: &str, _: i64) -> Result<Vec<PersonResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn search_events(&self, _: &str, _: i64) -> Result<Vec<EventResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
        fn get_sources_for_ids(&self, _: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
            if self.fail_all {
                Err(boom())
            } else {
                Ok(Vec::new())
            }
        }
    }

    /// 历史路由测试用的装配：Language 侧给内存库（这些用例只断言 history）。
    fn test_router(fail_all: bool) -> axum::Router {
        let language_store = Arc::new(parking_lot::Mutex::new(
            devtoolbox_infrastructure::LanguageStore::open(std::env::temp_dir().join(format!(
                "self-tools-test-language-{}.db",
                std::process::id()
            )))
            .expect("language store"),
        ));
        let content: Arc<dyn devtoolbox_application::language::LanguageStorePort> = Arc::new(
            devtoolbox_infrastructure::LanguageStoreAdapter::new(Arc::clone(&language_store)),
        );
        let learning_store = Arc::new(parking_lot::Mutex::new(
            devtoolbox_infrastructure::LearningStore::open(std::env::temp_dir().join(format!(
                "self-tools-test-learning-{}.db",
                std::process::id()
            )))
            .expect("learning store"),
        ));
        let geography = Arc::new(devtoolbox_application::geography::GeographyService::new(
            Arc::new(devtoolbox_infrastructure::GeographyQueryAdapter::new(
                Arc::new(parking_lot::Mutex::new(
                    devtoolbox_infrastructure::GeographyStore::open(std::env::temp_dir().join(
                        format!("self-tools-test-geography-{}.db", std::process::id()),
                    ))
                    .expect("geography store"),
                )),
            )),
        ));
        let learning_os = Arc::new(devtoolbox_application::learning::LearningService::new(
            Arc::new(devtoolbox_infrastructure::LearningStoreAdapter::new(
                learning_store,
            )),
        ));
        let learning = Arc::new(
            devtoolbox_application::language::LanguageLearningService::new(
                Arc::clone(&content),
                Arc::clone(&learning_os),
            ),
        );
        let course = Arc::new(
            devtoolbox_application::language::course::CourseService::new(
                Arc::new(devtoolbox_infrastructure::ports::CourseStoreAdapter::new(
                    Arc::clone(&language_store),
                )),
                Arc::clone(&learning_os),
            ),
        );
        router(
            Arc::new(HistoryService::new(Box::new(FakeHistory { fail_all }))),
            learning,
            Arc::new(devtoolbox_application::language::LanguageService::new(
                Arc::clone(&content),
            )),
            content,
            course,
            // readiness 探测用的数据目录（测试里指向系统临时目录）。
            std::env::temp_dir(),
            crate::knowledge_api::unavailable_runtime(&std::env::temp_dir()),
            Arc::clone(&language_store),
            Arc::new(TestSettings),
            learning_os,
            geography,
            news_service(),
            // 泛型参数直传具体类型：FeedFetcherPort 用 `-> impl Future` 声明，
            // 不是 dyn 兼容的。
            Arc::new(devtoolbox_application::news::NewsIngestService::new(
                news_service(),
                devtoolbox_infrastructure::FeedFetcherAdapter::new(
                    devtoolbox_infrastructure::feed_fetcher::feed_client().expect("http client"),
                ),
            )) as Arc<dyn devtoolbox_application::news::NewsIngestPort>,
        )
    }

    /// News 测试装配：内存 news 库 + 不联网的抓取器。
    fn news_service() -> Arc<devtoolbox_application::news::NewsService> {
        Arc::new(devtoolbox_application::news::NewsService::new(Arc::new(
            devtoolbox_infrastructure::NewsRepositoryAdapter::new(Arc::new(
                parking_lot::Mutex::new(
                    devtoolbox_infrastructure::NewsRepository::open(
                        std::env::temp_dir()
                            .join(format!("self-tools-test-news-{}.db", std::process::id())),
                    )
                    .expect("news store"),
                ),
            )),
        )))
    }

    /// 设置测试替身（不碰磁盘）。
    struct TestSettings;

    impl crate::ai_api::SettingsAccess for TestSettings {
        fn load(&self) -> devtoolbox_core::settings::AppSettings {
            devtoolbox_core::settings::AppSettings::default()
        }
        fn save(&self, _: &devtoolbox_core::settings::AppSettings) -> Result<(), String> {
            Ok(())
        }
    }

    /// 契约护栏：前端 `transport.ts` 的映射必须与服务端路由一一对应。
    /// 少注册一个，网页端对应功能就是「静默 404」。
    #[test]
    fn language_endpoints_are_registered() {
        let router = test_router(false);
        let source = include_str!("routes.rs");
        for path in [
            "/api/v1/language/languages",
            "/api/v1/language/sources",
            "/api/v1/language/search",
            "/api/v1/language/sentences",
            "/api/v1/language/items/{id}",
            "/api/v1/language/detail/{id}",
            "/api/v1/language/study-queue",
            "/api/v1/language/study",
            "/api/v1/language/add-to-review",
            "/api/v1/language/review-queue",
            "/api/v1/language/review",
            "/api/v1/language/mistakes",
            "/api/v1/language/lessons",
            "/api/v1/language/continue",
            "/api/v1/language/progress",
            "/api/v1/language/weak-items",
        ] {
            assert!(source.contains(path), "路由未注册：{path}");
        }
        let _ = router;
    }

    /// 请求体必须与前端 `aiClient` **实际发送的形状一致**（外层包 `request`）。
    ///
    /// 回归：server 曾自造 `{message, session_id}`，而前端发
    /// `{request:{message, app_context, capabilities, ...}}`，于是网页端
    /// 一点「问 AI」就 422，报错还被包装成「无法连接本地数据服务」——
    /// 看起来像服务没起，实际是两端契约从未对齐。
    /// 这个用例直接用前端真实形状，契约再改就会红。
    #[tokio::test]
    async fn ai_chat_accepts_frontend_request_shape() {
        let app = test_router(false);
        let (status, body) = post_json(
            &app,
            "/api/v1/ai/chat",
            &serde_json::json!({
                "request": {
                    "message": "你好",
                    "session_id": null,
                    "app_context": {
                        "module": "language",
                        "page": "home",
                        "entity": null,
                        "view_state": {}
                    },
                    "capabilities": [],
                    "locale": "zh"
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
        assert_eq!(
            body["code"], "ai_not_configured",
            "请求体应被接受并走到「未配置」分支，而不是 422：{body}"
        );
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|m| m.contains("未配置")),
            "错误信息应说明未配置：{body}"
        );
    }

    #[tokio::test]
    async fn ai_status_reports_unconfigured() {
        let app = test_router(false);
        let (status, body) = request(&app, "/api/v1/ai/status").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["configured"], false);
    }

    #[tokio::test]
    async fn settings_round_trips_through_http() {
        let app = test_router(false);
        let (status, body) = request(&app, "/api/v1/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.get("ai").is_some(), "设置应包含 ai 段");
    }

    /// 契约护栏：Learning OS 的端点必须全部注册。
    /// 少注册一个，网页端 Collections / Graph / Review Center / Home 今日面板就是空的。
    #[test]
    fn learning_endpoints_are_registered() {
        let router = test_router(false);
        let source = include_str!("routes.rs");
        for path in [
            "/api/v1/learning/progress",
            "/api/v1/learning/review/queue",
            "/api/v1/learning/review/stats",
            "/api/v1/learning/review",
            "/api/v1/learning/today",
            "/api/v1/learning/graph",
            "/api/v1/learning/explore",
            "/api/v1/learning/collections",
        ] {
            assert!(source.contains(path), "Learning 路由未注册：{path}");
        }
        let _ = router;
    }

    #[tokio::test]
    async fn collections_round_trip_over_http() {
        let app = test_router(false);

        let (status, before) = request(&app, "/api/v1/learning/collections").await;
        assert_eq!(status, StatusCode::OK);
        let start = before.as_array().map_or(0, Vec::len);

        let (status, created) = post_json(
            &app,
            "/api/v1/learning/collections",
            &serde_json::json!({ "title": "验收合集", "tags": ["测试"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "创建失败：{created}");
        let id = created["id"].as_str().expect("id").to_string();

        // 重新读回：验证真的落库，而不是只回显请求
        let (_, after) = request(&app, "/api/v1/learning/collections").await;
        let items = after.as_array().expect("array");
        assert_eq!(items.len(), start + 1);
        assert!(
            items.iter().any(|c| c["id"] == id),
            "新建的合集应出现在列表里"
        );

        // 删除
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/learning/collections/{id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let (_, final_state) = request(&app, "/api/v1/learning/collections").await;
        assert_eq!(
            final_state.as_array().map_or(0, Vec::len),
            start,
            "删除后应恢复原状"
        );
    }

    #[tokio::test]
    async fn empty_collection_title_is_rejected() {
        let app = test_router(false);
        let (status, body) = post_json(
            &app,
            "/api/v1/learning/collections",
            &serde_json::json!({ "title": "   " }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "invalid");
    }

    #[tokio::test]
    async fn geography_search_reads_real_database() {
        let app = test_router(false);
        // 搜一个真实库里存在的实体名：命中即证明走的是 geography.db，
        // 而不是前端那份 12 条硬编码示例。
        let (status, body) = request(
            &app,
            "/api/v1/geography/search?query=%E4%B8%8A%E6%B5%B7&limit=5",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let groups = body.as_array().expect("groups array");
        assert!(!groups.is_empty(), "上海应当能搜到");
        assert!(
            groups
                .iter()
                .any(|g| g["items"].as_array().is_some_and(|items| !items.is_empty())),
            "至少要有一个分组带条目"
        );
    }

    #[tokio::test]
    async fn geography_search_empty_query_returns_empty_not_error() {
        let app = test_router(false);
        // 空白 query 不该 400 —— 前端在用户清空输入框时会正常发请求。
        let (status, body) = request(&app, "/api/v1/geography/search?query=%20%20").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().map_or(0, Vec::len), 0);
    }

    #[tokio::test]
    async fn geography_search_unknown_type_is_rejected() {
        let app = test_router(false);
        // 静默忽略未知类型会让「筛选没生效」伪装成「搜不到」，必须 400。
        let (status, body) =
            request(&app, "/api/v1/geography/search?query=x&entity_type=banana").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "invalid");
    }

    #[tokio::test]
    async fn geography_detail_unknown_id_is_404() {
        let app = test_router(false);
        let (status, _) = request(&app, "/api/v1/geography/entities/definitely-not-a-place").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// 请求体键必须是驼峰（与前端 transport 的参数约定一致），
    /// 否则每个写操作都会 422。
    #[tokio::test]
    async fn study_accepts_camel_case_body() {
        let app = test_router(false);
        let (status, body) = post_json(
            &app,
            "/api/v1/language/study",
            &serde_json::json!({ "entityId": "jmdict:nope", "action": "study" }),
        )
        .await;
        // id 不存在 → 404（说明**成功反序列化**了请求体，否则会是 422）
        assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
        assert_eq!(body["code"], "not_found");
    }

    #[tokio::test]
    async fn study_rejects_unknown_language_code() {
        let app = test_router(false);
        let (status, _) = get_query(&app, "/api/v1/language/study-queue?language=klingon").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn language_item_unknown_id_returns_not_found() {
        let app = test_router(false);
        let (status, body) = request(&app, "/api/v1/language/items/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "not_found");
    }

    async fn post_json(app: &axum::Router, uri: &str, payload: &Value) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn get_query(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
        request(app, uri).await
    }

    async fn request(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn health_ok() {
        let (status, body) = request(&test_router(false), "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, serde_json::json!({ "status": "ok" }));
    }

    #[tokio::test]
    async fn home_returns_semantic_home_shape() {
        let (status, body) = request(&test_router(false), "/api/v1/history/home").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.get("periods").unwrap().is_array());
        assert!(body.get("stories").unwrap().is_array());
        assert!(body.get("stats").unwrap().is_object());
    }

    #[tokio::test]
    async fn search_with_query_ok() {
        let (status, body) =
            request(&test_router(false), "/api/v1/history/search?q=%E5%B8%9D").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn search_missing_q_is_400_error_contract() {
        let (status, body) = request(&test_router(false), "/api/v1/history/search").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "history_error");
        assert!(body["message"].is_string());
    }

    #[tokio::test]
    async fn detail_endpoints_404_with_error_contract() {
        for path in [
            "/api/v1/history/periods/missing",
            "/api/v1/history/events/missing",
            "/api/v1/history/people/missing",
            "/api/v1/history/works/missing",
            "/api/v1/history/stories/missing",
        ] {
            let (status, body) = request(&test_router(false), path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(body["code"], "history_error", "{path}");
            assert!(
                body["message"].as_str().unwrap().contains("not found"),
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn failing_query_returns_500_error_contract() {
        let (status, body) = request(&test_router(true), "/api/v1/history/home").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "history_error");
        assert!(body["message"].is_string());
    }

    #[tokio::test]
    async fn unknown_route_is_404() {
        let (status, _) = request(&test_router(false), "/api/v1/history/charts").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
