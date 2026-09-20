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
use axum::routing::get;
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

/// 组装全部路由（服务由组合根注入，测试可替换为假实现）。
#[must_use]
pub fn router(service: Arc<HistoryService>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/history/home", get(home))
        .route("/api/v1/history/search", get(search))
        .route("/api/v1/history/periods/{id}", get(period))
        .route("/api/v1/history/events/{id}", get(event))
        .route("/api/v1/history/people/{id}", get(person))
        .route("/api/v1/history/works/{id}", get(work))
        .route("/api/v1/history/stories/{id}", get(story))
        .with_state(AppState { service })
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

    fn test_router(fail_all: bool) -> axum::Router {
        router(Arc::new(HistoryService::new(Box::new(FakeHistory {
            fail_all,
        }))))
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
