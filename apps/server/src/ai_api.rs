//! AI 与设置的 HTTP 接口（网页端）。
//!
//! 网页端没有 Tauri IPC，Ask AI / Settings 都拿不到数据。这里补上：
//! - `GET/POST /api/v1/settings` —— 读写 `settings.json`（与桌面端**同一个文件**，
//!   所以网页端配好的 provider，桌面端立刻可用，反之亦然）；
//! - `GET  /api/v1/ai/status` —— provider 是否就绪 + 已注册模块/工具；
//! - `POST /api/v1/ai/chat`  —— 发起一次 agent 对话。
//!
//! 诚实边界：**未配置 provider 时 chat 不会假装成功**，而是返回明确的
//! 「未配置模型」错误码。宁可报未配置，也不要返回一句编造的答复。

use std::sync::Arc;

// Rust 2024 起 `Future` 在 prelude 里，无需限定路径。

use devtoolbox_core::personal_ai::types::AgentRequest;
use devtoolbox_core::settings::AppSettings;
use serde::{Deserialize, Serialize};

/// 统一错误契约（与 history / language 侧一致）。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code {
            "not_found" => axum::http::StatusCode::NOT_FOUND,
            "invalid" | "ai_not_configured" => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}

/// 设置读写所需的最小存储能力（由组合根注入，测试可替换）。
pub trait SettingsAccess: Send + Sync {
    fn load(&self) -> AppSettings;
    fn save(&self, settings: &AppSettings) -> Result<(), String>;
}

/// `GET /api/v1/settings`
pub async fn get_settings(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
) -> Result<axum::Json<AppSettings>, ApiError> {
    Ok(axum::Json(store.load()))
}

/// `POST /api/v1/settings`
pub async fn save_settings(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
    axum::Json(settings): axum::Json<AppSettings>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    store.save(&settings).map_err(|message| ApiError {
        code: "settings_save_failed",
        message,
    })?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
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
) -> Result<axum::Json<AiStatus>, ApiError> {
    let settings = store.load();
    Ok(axum::Json(AiStatus {
        configured: settings.ai.is_configured(),
        provider: settings.ai.provider.clone(),
        model: settings.ai.model.clone(),
        modules: Vec::new(),
        tools: Vec::new(),
    }))
}

/// `POST /api/v1/ai/chat`
///
/// `/api/v1/ai/chat` 的请求体。
///
/// 直接用 core 的 [`AgentRequest`]：前端 `aiClient` 发来的就是它（外层包一个
/// `request` 键）。此前 server 自造了一个只有 `{message, session_id}` 的
/// `ChatRequest`，于是前端一发
/// `{request:{message, app_context, capabilities, ...}}` 就 422——
/// **网页端的 AI 从来没通过过**，只是错误信息被前端包装成
/// 「无法连接本地数据服务（422）」，看起来像服务没启动。
#[derive(Debug, Deserialize)]
pub struct ChatBody {
    pub request: AgentRequest,
}

/// 发起一次 agent 对话。
///
/// 只在 provider 就绪时委派给真正的 agent 运行时；未配置时**明确拒绝**，
/// 不返回任何编造内容。
pub async fn ai_chat(
    axum::Extension(store): axum::Extension<Arc<dyn SettingsAccess>>,
    axum::Extension(agent): axum::Extension<Option<Arc<dyn AiChatRunner>>>,
    axum::Json(body): axum::Json<ChatBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let request = body.request;
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
    let response = agent
        .chat(&request.message, request.session_id.as_deref())
        .await
        .map_err(|message| ApiError {
            code: "ai_failed",
            message,
        })?;
    Ok(axum::Json(response))
}

/// 真正的 agent 调用（由组合根实现；未配置 provider 时不构造）。
///
/// 用 `async fn` 而非 `-> impl Future`：后者不是 dyn 兼容的，
/// 而这里需要 `Arc<dyn AiChatRunner>`（provider 可能缺席 → 用 Option 表达）。
pub trait AiChatRunner: Send + Sync {
    fn chat<'a>(
        &'a self,
        message: &'a str,
        session_id: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + 'a>>;
}

/// 基于 `SettingsStore` 的生产实现（读写 `settings.json`，与桌面端同一文件）。
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

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        self.store.save(settings).map_err(|error| error.to_string())
    }
}
