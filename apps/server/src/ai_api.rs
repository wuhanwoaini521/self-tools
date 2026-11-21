//! AI 与设置的 HTTP 接口（网页端）。
//!
//! 网页端通过同一组合根 PersonalAgent 访问数据：
//! - `GET /api/v1/settings` 返回 `PublicAppSettings`（无密钥值）；
//! - `POST /api/v1/settings` 拒绝写入，直到管理 API 有独立认证边界；
//! - AI chat、运行取消、Travel proposal 确认/取消均委派给运行时。
//!
//! 未配置 provider 时明确报错，不返回模型伪造的成功结果。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use devtoolbox_core::personal_ai::types::AgentRequest;
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::{
    AgentError, AgentResponse, JobProgressEvent, JobRunSnapshot, ModuleDescriptor, ToolSpec,
};
use devtoolbox_runtime::job_events::{AiJobEvents, ReplayError};
use serde::{Deserialize, Serialize};

/// 统一错误契约（与其他 HTTP 接口一致）。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        const BAD_REQUEST_CODES: [&str; 18] = [
            "invalid",
            "conversation_invalid_role",
            "ai_not_configured",
            "language_invalid_mark",
            "language_shadow_empty_transcript",
            "language_shadow_no_sentence",
            "language_speaking_empty_turn",
            "language_speaking_empty_audio",
            "language_speaking_audio_disabled",
            "language_speaking_invalid",
            "language_speaking_timeout",
            "study_board_invalid",
            "travel_plan_invalid",
            "travel_plan_proposal_invalid",
            "travel_amap_not_configured",
            "travel_qweather_not_configured",
            "travel_llm_test_failed",
            "travel_qweather_test_failed",
        ];
        let status = match self.code {
            "not_found" | "job_not_found" => axum::http::StatusCode::NOT_FOUND,
            "settings_write_requires_desktop" => axum::http::StatusCode::FORBIDDEN,
            "travel_plan_conflict"
            | "travel_plan_proposal_expired"
            | "personal_ai_cancelled"
            | "job_cursor_expired" => axum::http::StatusCode::CONFLICT,
            "ai_unavailable"
            | "language_speaking_llm_unavailable"
            | "language_speaking_asr_unavailable" => axum::http::StatusCode::SERVICE_UNAVAILABLE,
            code if BAD_REQUEST_CODES.contains(&code) => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}

/// 设置读取所需的最小能力（由组合根注入）。
pub trait SettingsAccess: Send + Sync {
    fn load(&self) -> AppSettings;
}

/// Public HTTP view: `PublicAppSettings` plus `ai_configured`. No credentials.
#[derive(Debug, Serialize)]
pub struct SettingsStatus {
    pub ai_configured: bool,
    #[serde(flatten)]
    pub public: devtoolbox_core::settings::PublicAppSettings,
}

/// `GET /api/v1/settings`
pub async fn get_settings(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
) -> Result<axum::Json<SettingsStatus>, ApiError> {
    let settings = store.load();
    Ok(axum::Json(SettingsStatus {
        ai_configured: settings.ai.is_configured(),
        public: settings.public_view(),
    }))
}

/// `POST /api/v1/settings` remains disabled until an authenticated administration boundary exists.
pub async fn save_settings() -> Result<axum::Json<serde_json::Value>, ApiError> {
    Err(ApiError {
        code: "settings_write_requires_desktop",
        message: "网页端配置写入已禁用；请在本机桌面应用中修改设置。".to_string(),
    })
}

/// AI 状态（与前端 `AiStatus` 对齐）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub configured: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub modules: Vec<serde_json::Value>,
    pub tools: Vec<serde_json::Value>,
}

/// `GET /api/v1/ai/status`
pub async fn ai_status(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
) -> Result<axum::Json<AiStatus>, ApiError> {
    let settings = store.load();
    let (modules, tools) =
        agent.map_or_else(|| (Vec::new(), Vec::new()), |runner| runner.catalog());
    Ok(axum::Json(AiStatus {
        configured: settings.ai.is_configured(),
        provider: settings.ai.provider.clone(),
        model: settings.ai.model.clone(),
        modules: modules
            .into_iter()
            .filter_map(|module| serde_json::to_value(module).ok())
            .collect(),
        tools: tools
            .into_iter()
            .filter_map(|tool| serde_json::to_value(tool).ok())
            .collect(),
    }))
}

/// Browser request body: the canonical core AgentRequest plus a per-run correlation ID.
#[derive(Debug, Deserialize)]
pub struct ChatBody {
    pub request: AgentRequest,
    #[serde(default)]
    pub trace_id: Option<String>,
}

/// `POST /api/v1/ai/chat`
pub async fn ai_chat(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::Json(body): axum::Json<ChatBody>,
) -> Result<axum::Json<AgentResponse>, ApiError> {
    let settings = store.load();
    if !settings.ai.is_configured() {
        return Err(ApiError {
            code: "ai_not_configured",
            message:
                "未配置 AI 模型：请在「设置」里填写 OpenAI 兼容的 base_url 与 model（本地 Ollama 可留空 key）。"
                    .to_string(),
        });
    }
    let Some(agent) = agent else {
        return Err(ApiError {
            code: "ai_unavailable",
            message: "AI 运行时未就绪。".to_string(),
        });
    };
    let trace_id = body.trace_id.unwrap_or_else(next_trace_id);
    if !valid_trace_id(&trace_id) {
        return Err(ApiError {
            code: "invalid",
            message: "AI trace ID 无效。".to_string(),
        });
    }
    let response = agent
        .chat(body.request, trace_id)
        .await
        .map_err(|error| ApiError {
            code: error.code(),
            message: error.message,
        })?;
    Ok(axum::Json(response))
}

fn valid_trace_id(trace_id: &str) -> bool {
    !trace_id.is_empty() && trace_id.len() <= 128 && !trace_id.chars().any(char::is_control)
}

fn next_trace_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "http-{}-{}-{}",
        std::process::id(),
        devtoolbox_infrastructure::now_unix(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

pub async fn ai_cancel_run(
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> Result<axum::Json<bool>, ApiError> {
    let agent = require_runner(agent)?;
    Ok(axum::Json(agent.cancel_run(&trace_id)))
}

fn require_runner(agent: Option<Arc<dyn AiChatRunner>>) -> Result<Arc<dyn AiChatRunner>, ApiError> {
    agent.ok_or_else(|| ApiError {
        code: "ai_unavailable",
        message: "AI 运行时未就绪。".to_string(),
    })
}

fn run_events(agent: Option<Arc<dyn AiChatRunner>>) -> Result<Arc<AiJobEvents>, ApiError> {
    require_runner(agent)?.job_events().ok_or_else(|| ApiError {
        code: "ai_unavailable",
        message: "AI 进度事件源未就绪。".to_string(),
    })
}

fn require_trace_id(trace_id: &str) -> Result<(), ApiError> {
    if valid_trace_id(trace_id) {
        Ok(())
    } else {
        Err(ApiError {
            code: "invalid",
            message: "AI trace ID 无效。".to_string(),
        })
    }
}

/// `GET /api/v1/ai/runs/{trace_id}`：最近状态与重放游标。
///
/// 未知 trace（例如服务重启后）返回 404 `job_not_found`，客户端据此明确显示
/// “状态已丢失”，而不是假装任务还在运行。
pub async fn ai_run_status(
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> Result<axum::Json<JobRunSnapshot>, ApiError> {
    require_trace_id(&trace_id)?;
    let events = run_events(agent)?;
    events
        .snapshot(&trace_id)
        .map(axum::Json)
        .ok_or_else(|| ApiError {
            code: "job_not_found",
            message: "运行状态已丢失（服务可能已重启）。".to_string(),
        })
}

/// `GET /api/v1/ai/runs/{trace_id}/events`（SSE）。
#[derive(Debug, Deserialize)]
pub struct RunEventsQuery {
    /// 客户端显式游标；与 SSE `Last-Event-ID` 取更靠后的一个。
    #[serde(default)]
    pub after: Option<u64>,
}

/// `GET /api/v1/ai/runs/{trace_id}/events`
///
/// 先重放环形缓冲，再转发实时事件；终态事件之后关闭流。游标早于缓冲起点时
/// 返回 409 `job_cursor_expired`，客户端改用状态端点恢复。
pub async fn ai_run_events(
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<RunEventsQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    require_trace_id(&trace_id)?;
    let events = run_events(agent)?;
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let after = match (last_event_id, query.after) {
        (Some(header), Some(query)) => Some(header.max(query)),
        (header, query) => header.or(query),
    };

    // 先订阅再重放：两步之间产生的事件不会丢，重复的由 seq 过滤。
    let mut receiver = events.subscribe(&trace_id);
    let replayed = events.replay(&trace_id, after).map_err(replay_error)?;
    // 带游标却没有任何事件：进程重启过，状态已丢失（首次连接没有游标，允许等待运行开始）。
    if after.is_some() && replayed.is_empty() && !events.has_events(&trace_id) {
        return Err(ApiError {
            code: "job_not_found",
            message: "运行状态已丢失（服务可能已重启）。".to_string(),
        });
    }
    let mut last_seq = replayed
        .last()
        .map_or(after.unwrap_or(0), |event| event.seq);
    let replay_finished = replayed.last().is_some_and(JobProgressEvent::is_terminal);

    let stream = async_stream::stream! {
        for event in replayed {
            yield Ok::<_, std::convert::Infallible>(sse_event(&event));
        }
        if replay_finished {
            return;
        }
        loop {
            match receiver.recv().await {
                Ok(event) if event.seq > last_seq => {
                    last_seq = event.seq;
                    let terminal = event.is_terminal();
                    yield Ok(sse_event(&event));
                    if terminal {
                        break;
                    }
                }
                Ok(_) => {}
                // 订阅者落后超过缓冲：断开，让客户端用 Last-Event-ID 从环形缓冲重放。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let mut response = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response();
    response.headers_mut().insert(
        axum::http::header::HeaderName::from_static("x-accel-buffering"),
        axum::http::HeaderValue::from_static("no"),
    );
    Ok(response)
}

fn sse_event(event: &JobProgressEvent) -> Event {
    Event::default()
        .id(event.seq.to_string())
        .event("progress")
        .json_data(event)
        // 结构体序列化不会失败；失败也返回一个空事件而不是 panic。
        .unwrap_or_else(|_| Event::default().event("progress").data("{}"))
}

fn replay_error(error: ReplayError) -> ApiError {
    match error {
        ReplayError::Unknown => ApiError {
            code: "job_not_found",
            message: "运行状态已丢失（服务可能已重启）。".to_string(),
        },
        ReplayError::CursorExpired { earliest } => ApiError {
            code: "job_cursor_expired",
            message: format!("进度游标已过期；请从最近状态（seq {earliest}）重新同步。"),
        },
    }
}

pub async fn ai_confirm_travel_change(
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Result<axum::Json<devtoolbox_core::travel::TravelPlan>, ApiError> {
    let Some(agent) = agent else {
        return Err(ApiError {
            code: "ai_unavailable",
            message: "AI 运行时未就绪。".to_string(),
        });
    };
    agent
        .confirm_travel_change(&proposal_id)
        .map(axum::Json)
        .map_err(travel_change_error)
}

pub async fn ai_cancel_travel_change(
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let Some(agent) = agent else {
        return Err(ApiError {
            code: "ai_unavailable",
            message: "AI 运行时未就绪。".to_string(),
        });
    };
    agent
        .cancel_travel_change(&proposal_id)
        .map(|()| axum::Json(serde_json::json!({ "cancelled": true })))
        .map_err(travel_change_error)
}

fn travel_change_error(error: devtoolbox_application::travel::TravelPlanChangeError) -> ApiError {
    let code = match error {
        devtoolbox_application::travel::TravelPlanChangeError::Conflict => "travel_plan_conflict",
        devtoolbox_application::travel::TravelPlanChangeError::Expired => {
            "travel_plan_proposal_expired"
        }
        _ => "travel_plan_proposal_invalid",
    };
    ApiError {
        code,
        message: error.to_string(),
    }
}

/// The runtime delegates this to the shared PersonalAgent, its session store, and Travel service.
pub trait AiChatRunner: Send + Sync {
    fn chat<'a>(
        &'a self,
        request: AgentRequest,
        trace_id: String,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<AgentResponse, AgentError>> + Send + 'a>>;

    fn catalog(&self) -> (Vec<ModuleDescriptor>, Vec<ToolSpec>) {
        (Vec::new(), Vec::new())
    }

    fn cancel_run(&self, _trace_id: &str) -> bool {
        false
    }

    /// 进度事件源（网页 SSE 用）。默认没有：只读实现无需提供。
    fn job_events(&self) -> Option<Arc<AiJobEvents>> {
        None
    }

    fn confirm_travel_change(
        &self,
        _proposal_id: &str,
    ) -> Result<
        devtoolbox_core::travel::TravelPlan,
        devtoolbox_application::travel::TravelPlanChangeError,
    > {
        Err(devtoolbox_application::travel::TravelPlanChangeError::InvalidProposal)
    }

    fn cancel_travel_change(
        &self,
        _proposal_id: &str,
    ) -> Result<(), devtoolbox_application::travel::TravelPlanChangeError> {
        Err(devtoolbox_application::travel::TravelPlanChangeError::InvalidProposal)
    }
}

/// Read-only SettingsStore access shared with the desktop app.
pub struct FileSettingsAccess {
    store: devtoolbox_infrastructure::SettingsStore,
}

impl FileSettingsAccess {
    #[must_use]
    pub fn new(config_directory: impl AsRef<std::path::Path>) -> Self {
        Self {
            store: devtoolbox_infrastructure::SettingsStore::new(config_directory),
        }
    }
}

impl SettingsAccess for FileSettingsAccess {
    fn load(&self) -> AppSettings {
        self.store.load().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::settings::AiSettings;
    use std::sync::Mutex;

    struct TestSettings(AppSettings);

    impl SettingsAccess for TestSettings {
        fn load(&self) -> AppSettings {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(String, String)>>,
    }

    impl AiChatRunner for FakeRunner {
        fn chat<'a>(
            &'a self,
            request: AgentRequest,
            trace_id: String,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<AgentResponse, AgentError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("calls lock")
                    .push((request.message, trace_id.clone()));
                Ok(AgentResponse {
                    session_id: "session-1".into(),
                    trace_id: Some(trace_id),
                    citations: Vec::new(),
                    proposed_changes: Vec::new(),
                    message: "contract response".into(),
                    actions: Vec::new(),
                    ui_blocks: Vec::new(),
                    tool_trace: Vec::new(),
                    usage: None,
                    messages: Vec::new(),
                    provider: Some("fake-contract-test".into()),
                    model: Some("not-a-real-provider".into()),
                    orchestration: None,
                })
            })
        }
    }

    fn configured_settings() -> AppSettings {
        AppSettings {
            ai: AiSettings {
                base_url: Some("http://provider.invalid/v1".into()),
                model: Some("contract-test".into()),
                ..AiSettings::default()
            },
            ..AppSettings::default()
        }
    }

    fn request() -> AgentRequest {
        serde_json::from_value(serde_json::json!({
            "message": "add a place",
            "session_id": "session-1",
            "app_context": { "module": "travel", "page": "guide", "entity": { "kind": "destination", "id": "Hangzhou" }, "view_state": { "plan_revision": 3, "plan_persisted": true } },
            "capabilities": ["travel"],
            "locale": "en"
        }))
        .expect("valid AgentRequest")
    }

    #[tokio::test]
    async fn configured_http_chat_delegates_full_request_and_trace_id_to_runtime() {
        let store: Arc<dyn SettingsAccess> = Arc::new(TestSettings(configured_settings()));
        let runner = Arc::new(FakeRunner::default());
        let agent: Option<Arc<dyn AiChatRunner>> = Some(runner.clone());
        let response = ai_chat(
            axum::Extension(store),
            axum::Extension(agent),
            axum::Json(ChatBody {
                request: request(),
                trace_id: Some("browser-run-1".into()),
            }),
        )
        .await
        .expect("configured runner succeeds")
        .0;

        assert_eq!(response.message, "contract response");
        assert_eq!(response.trace_id.as_deref(), Some("browser-run-1"));
        assert_eq!(
            runner.calls.lock().expect("calls lock").as_slice(),
            &[("add a place".into(), "browser-run-1".into())]
        );
    }

    struct EventRunner {
        events: Arc<AiJobEvents>,
    }

    impl AiChatRunner for EventRunner {
        fn chat<'a>(
            &'a self,
            _request: AgentRequest,
            _trace_id: String,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<AgentResponse, AgentError>> + Send + 'a>>
        {
            Box::pin(async { Err(AgentError::model_unavailable("unused in this test")) })
        }

        fn job_events(&self) -> Option<Arc<AiJobEvents>> {
            Some(Arc::clone(&self.events))
        }
    }

    fn event_runner(events: &Arc<AiJobEvents>) -> Arc<dyn AiChatRunner> {
        Arc::new(EventRunner {
            events: Arc::clone(events),
        })
    }

    fn seed_run(events: &Arc<AiJobEvents>) {
        let sink = events.sink("run-sse");
        let _ = sink.record(&devtoolbox_core::AgentProgress::new(
            devtoolbox_core::AgentStage::Preparing,
        ));
        let _ = sink.record(&devtoolbox_core::AgentProgress::with_detail(
            devtoolbox_core::AgentStage::Thinking,
            "round 1",
        ));
        sink.finish_done();
    }

    async fn sse_text(response: Response) -> (String, axum::http::HeaderMap) {
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("sse body");
        (String::from_utf8(body.to_vec()).expect("utf8 sse"), headers)
    }

    fn sse_payloads(body: &str) -> Vec<serde_json::Value> {
        body.lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).expect("event json"))
            .collect()
    }

    #[tokio::test]
    async fn run_status_reports_the_latest_state_and_unknown_runs_fail_closed() {
        let events = Arc::new(AiJobEvents::default());
        seed_run(&events);
        let agent = event_runner(&events);

        let snapshot = ai_run_status(
            axum::Extension(Some(agent.clone())),
            axum::extract::Path("run-sse".to_string()),
        )
        .await
        .expect("known run")
        .0;
        assert_eq!(snapshot.state, devtoolbox_core::JobState::Done);
        assert_eq!(snapshot.cursor, 3);
        assert_eq!(
            snapshot.latest.as_ref().map(|event| event.stage),
            Some(devtoolbox_core::AgentStage::Done)
        );

        let error = ai_run_status(
            axum::Extension(Some(agent)),
            axum::extract::Path("after-restart".to_string()),
        )
        .await
        .expect_err("unknown run must be reported as lost");
        assert_eq!(error.code, "job_not_found");
        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn run_events_replays_the_buffer_as_an_sse_stream_and_closes_on_terminal() {
        let events = Arc::new(AiJobEvents::default());
        seed_run(&events);
        let response = ai_run_events(
            axum::Extension(Some(event_runner(&events))),
            axum::extract::Path("run-sse".to_string()),
            axum::extract::Query(RunEventsQuery { after: None }),
            axum::http::HeaderMap::new(),
        )
        .await
        .expect("sse response");

        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        assert_eq!(
            response
                .headers()
                .get("x-accel-buffering")
                .and_then(|value| value.to_str().ok()),
            Some("no")
        );

        let (body, _) = sse_text(response).await;
        assert_eq!(body.matches("event: progress").count(), 3);
        let payloads = sse_payloads(&body);
        assert_eq!(
            payloads
                .iter()
                .map(|value| value["seq"].as_u64().expect("seq"))
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(payloads[1]["detail"], "round 1");
        assert_eq!(payloads[2]["state"], "done");
        // 终态之后流结束（不会挂住 replay）。
        assert!(body.trim_end().ends_with('}'));
    }

    #[tokio::test]
    async fn concurrent_sse_subscribers_receive_the_same_live_sequence() {
        let events = Arc::new(AiJobEvents::default());
        let agent = event_runner(&events);
        let open = || {
            ai_run_events(
                axum::Extension(Some(agent.clone())),
                axum::extract::Path("run-live".to_string()),
                axum::extract::Query(RunEventsQuery { after: None }),
                axum::http::HeaderMap::new(),
            )
        };
        // 两路都先订阅（返回时已挂到广播上），再产生实时事件。
        let first = open().await.expect("first subscriber");
        let second = open().await.expect("second subscriber");

        let sink = events.sink("run-live");
        let _ = sink.record(&devtoolbox_core::AgentProgress::new(
            devtoolbox_core::AgentStage::Preparing,
        ));
        sink.finish_done();

        let (first, _) = sse_text(first).await;
        let (second, _) = sse_text(second).await;
        assert_eq!(sse_payloads(&first), sse_payloads(&second));
        assert_eq!(sse_payloads(&first).len(), 2);
        assert_eq!(sse_payloads(&first)[1]["state"], "done");
    }

    #[tokio::test]
    async fn run_events_with_a_cursor_after_restart_reports_state_lost() {
        let events = Arc::new(AiJobEvents::default());
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("last-event-id", axum::http::HeaderValue::from_static("7"));
        let error = ai_run_events(
            axum::Extension(Some(event_runner(&events))),
            axum::extract::Path("pre-restart".to_string()),
            axum::extract::Query(RunEventsQuery { after: None }),
            headers,
        )
        .await
        .expect_err("a reconnecting client must learn the run state was lost");
        assert_eq!(error.code, "job_not_found");
    }

    #[tokio::test]
    async fn run_events_rejects_a_cursor_older_than_the_ring_buffer() {
        let events = Arc::new(AiJobEvents::new(2));
        seed_run(&events);
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("last-event-id", axum::http::HeaderValue::from_static("0"));
        let error = ai_run_events(
            axum::Extension(Some(event_runner(&events))),
            axum::extract::Path("run-sse".to_string()),
            axum::extract::Query(RunEventsQuery { after: None }),
            headers,
        )
        .await
        .expect_err("cursor before the ring must be rejected");
        assert_eq!(error.code, "job_cursor_expired");
        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn run_endpoints_fail_closed_without_a_runtime() {
        let status = ai_run_status(
            axum::Extension(None),
            axum::extract::Path("run-sse".to_string()),
        )
        .await
        .expect_err("no runner");
        assert_eq!(status.code, "ai_unavailable");
        let events = ai_run_events(
            axum::Extension(None),
            axum::extract::Path("run-sse".to_string()),
            axum::extract::Query(RunEventsQuery { after: None }),
            axum::http::HeaderMap::new(),
        )
        .await
        .expect_err("no runner");
        assert_eq!(events.code, "ai_unavailable");
    }

    #[tokio::test]
    async fn unconfigured_http_chat_fails_instead_of_fabricating_a_response() {
        let store: Arc<dyn SettingsAccess> = Arc::new(TestSettings(AppSettings::default()));
        let runner: Option<Arc<dyn AiChatRunner>> = Some(Arc::new(FakeRunner::default()));
        let error = ai_chat(
            axum::Extension(store),
            axum::Extension(runner),
            axum::Json(ChatBody {
                request: request(),
                trace_id: None,
            }),
        )
        .await
        .expect_err("unconfigured provider must be rejected");
        assert_eq!(error.code, "ai_not_configured");
    }
}
