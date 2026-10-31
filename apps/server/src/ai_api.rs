//! AI 与设置的 HTTP 接口（网页端）。
//!
//! 网页端通过同一组合根 PersonalAgent 访问数据：
//! - `GET /api/v1/settings` 只返回脱敏状态；
//! - `POST /api/v1/settings` 拒绝写入，直到管理 API 有独立认证边界；
//! - AI chat、运行取消、Travel proposal 确认/取消均委派给运行时。
//!
//! 未配置 provider 时明确报错，不返回模型伪造的成功结果。

use std::future::Future;
use std::sync::Arc;

use devtoolbox_core::personal_ai::types::AgentRequest;
use devtoolbox_core::settings::{AppSettings, SecretSettingsStatus};
use devtoolbox_core::{AgentError, AgentResponse, ModuleDescriptor, ToolSpec};
use serde::{Deserialize, Serialize};

/// 统一错误契约（与其他 HTTP 接口一致）。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        const BAD_REQUEST_CODES: [&str; 8] = [
            "invalid",
            "ai_not_configured",
            "language_invalid_mark",
            "language_shadow_empty_transcript",
            "language_shadow_no_sentence",
            "study_board_invalid",
            "travel_plan_invalid",
            "travel_plan_proposal_invalid",
        ];
        let status = match self.code {
            "not_found" => axum::http::StatusCode::NOT_FOUND,
            "settings_write_requires_desktop" => axum::http::StatusCode::FORBIDDEN,
            "travel_plan_conflict" | "travel_plan_proposal_expired" | "personal_ai_cancelled" => {
                axum::http::StatusCode::CONFLICT
            }
            "ai_unavailable" => axum::http::StatusCode::SERVICE_UNAVAILABLE,
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

/// Public HTTP view: configuration status only; no credentials, paths, or admin settings.
#[derive(Debug, Serialize)]
pub struct SettingsStatus {
    pub ai_configured: bool,
    pub secrets: SecretSettingsStatus,
}

/// `GET /api/v1/settings`
pub async fn get_settings(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
) -> Result<axum::Json<SettingsStatus>, ApiError> {
    let settings = store.load();
    let public = settings.public_view();
    Ok(axum::Json(SettingsStatus {
        ai_configured: settings.ai.is_configured(),
        secrets: public.secrets,
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
    if trace_id.len() > 128 || trace_id.is_empty() || trace_id.chars().any(char::is_control) {
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
    let Some(agent) = agent else {
        return Err(ApiError {
            code: "ai_unavailable",
            message: "AI 运行时未就绪。".to_string(),
        });
    };
    Ok(axum::Json(agent.cancel_run(&trace_id)))
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
