//! Travel（城市研究与攻略）的 HTTP 接口。
//!
//! ## 为什么必须有
//!
//! Travel 的能力在 Rust 侧一直完整（研究服务、provider 装配、会话注册表、SQLite 缓存），
//! 但只装配在桌面组合根里。网页端一进去就是
//! `「travel_research_start」尚无网页端接口`——整页功能不可用。
//!
//! 现在 `travel_providers` 与 `TravelStoreAdapter` 已下沉到 infrastructure，
//! 桌面与网页用**同一套** provider 与**同一份** travel.db。
//!
//! 交互形态与桌面端一致：`start` 立即返回 session_id，`progress` 轮询。

use std::sync::Arc;

use parking_lot::Mutex;

use axum::extract::Query;
use axum::{Extension, Json};
use devtoolbox_application::travel::TravelResearchRequest;
use devtoolbox_application::travel::session::TravelSessionRegistry;
use devtoolbox_core::personal_ai::ChatModelProvider;
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::travel::TravelDataProvider;
use devtoolbox_infrastructure::TravelStore;
use devtoolbox_infrastructure::travel::providers::travel_research_service;
use serde::Deserialize;
use serde_json::Value;

fn err(code: &'static str, message: impl Into<String>) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError {
        code,
        message: message.into(),
    }
}

#[derive(Debug, Deserialize)]
pub struct StartBody {
    pub request: TravelResearchRequest,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub session_id: String,
}

#[derive(Deserialize)]
pub struct GuideQuery {
    pub city: String,
    pub days: Option<u8>,
}

/// 运行时装配依赖。
#[derive(Clone)]
pub struct TravelDeps {
    pub client: reqwest::Client,
    pub store: Arc<Mutex<TravelStore>>,
    pub registry: Arc<TravelSessionRegistry>,
    pub settings_loader: Arc<dyn Fn() -> Result<AppSettings, String> + Send + Sync>,
}

/// 注册一次会话（与桌面端 `travel_research_start` 同构）。
pub fn research_start(
    deps: TravelDeps,
    request: TravelResearchRequest,
) -> Result<String, crate::ai_api::ApiError> {
    if request.city.trim().is_empty() {
        return Err(err("travel_empty_city", "请先填写目的地城市"));
    }
    let (session_id, session) = deps.registry.register();
    let client = deps.client.clone();
    let store = Arc::clone(&deps.store);
    let loader = Arc::clone(&deps.settings_loader);

    tokio::spawn(async move {
        let travel = loader().map(|settings| settings.travel).unwrap_or_default();
        let service = travel_research_service(&client, &travel, store);
        let handle = Arc::clone(&session);
        let progress = move |event| {
            handle
                .lock()
                .expect("travel session poisoned")
                .push_event(event);
        };
        match service.research_city(&request, &progress).await {
            Ok(outcome) => session
                .lock()
                .expect("travel session poisoned")
                .finish(outcome.guide, outcome.from_cache),
            Err(error) => session
                .lock()
                .expect("travel session poisoned")
                .fail(error.to_string()),
        }
    });
    Ok(session_id)
}

pub async fn travel_research_start(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Json(body): Json<StartBody>,
) -> Result<Json<String>, crate::ai_api::ApiError> {
    research_start((*deps).clone(), body.request).map(Json)
}

pub async fn travel_research_progress(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Query(params): Query<SessionQuery>,
) -> Result<Json<Option<Value>>, crate::ai_api::ApiError> {
    let Some(session) = deps.registry.get(&params.session_id) else {
        return Ok(Json(None));
    };
    // 直接序列化应用层的 view（字段与桌面端 `TravelResearchSnapshot` 一致）。
    let view = session.lock().expect("travel session poisoned").view();
    Ok(Json(Some(serde_json::to_value(view).map_err(|error| {
        err("travel_encode_failed", error.to_string())
    })?)))
}

pub async fn travel_recent_guides(
    Extension(deps): Extension<Arc<TravelDeps>>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let store = deps.store.lock();
    let summaries = store
        .list_guides(20)
        .map_err(|error| err("travel_error", error.to_string()))?;
    Ok(Json(serde_json::to_value(summaries).unwrap_or(Value::Null)))
}

pub async fn travel_load_guide(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Query(params): Query<GuideQuery>,
) -> Result<Json<Option<Value>>, crate::ai_api::ApiError> {
    let store = deps.store.lock();
    let guide = store
        .get_guide(&params.city, params.days.unwrap_or(3), 0)
        .map_err(|error| err("travel_error", error.to_string()))?;
    Ok(Json(guide.map(|value| {
        serde_json::to_value(value).unwrap_or(Value::Null)
    })))
}

// ---- 连通性测试（与桌面端 test_travel_* 同语义）----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestBody {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Deserialize)]
pub struct AmapTestBody {
    pub api_key: String,
}

#[derive(Deserialize)]
pub struct QweatherTestBody {
    pub api_key: String,
    #[serde(alias = "host", alias = "apiHost")]
    pub api_host: String,
}

/// 连通性测试：与桌面端 `test_travel_*` 用**同一套 provider 与同一句判词**，
/// 避免「桌面说通、网页说不通」这种漂移。
fn travel_test_error(code: &'static str, message: String) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError { code, message }
}

pub async fn test_travel_llm(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Json(body): Json<LlmTestBody>,
) -> Result<Json<String>, crate::ai_api::ApiError> {
    let provider = devtoolbox_infrastructure::travel::providers::llm_test_provider(
        &deps.client,
        body.base_url,
        body.api_key,
        body.model,
    );
    let answer = provider
        .chat(devtoolbox_core::ChatRequest {
            messages: vec![
                devtoolbox_core::ChatMessage::system("You are a connectivity test."),
                devtoolbox_core::ChatMessage::user("Reply with OK."),
            ],
            session_id: None,
            tools: Vec::new(),
            temperature: Some(0.2),
            max_tokens: None,
        })
        .await
        .map_err(|error| travel_test_error("travel_llm_test_failed", error.message))?
        .content
        .unwrap_or_default();
    Ok(Json(format!(
        "LLM 连接成功（收到 {} 个字符的响应）",
        answer.trim().chars().count()
    )))
}

pub async fn test_travel_amap(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Json(body): Json<AmapTestBody>,
) -> Result<Json<String>, crate::ai_api::ApiError> {
    let provider = devtoolbox_infrastructure::travel::providers::amap_test_provider(
        &deps.client,
        body.api_key,
    );
    let facts = provider
        .fetch(devtoolbox_core::travel::TravelDataRequest {
            city: "北京".to_string(),
            kind: "poi",
        })
        .await
        .map_err(|error| travel_test_error("travel_amap_test_failed", error.message))?;
    Ok(Json(format!(
        "高德连接成功（北京 POI 返回 {} 条）",
        facts.len()
    )))
}

pub async fn test_travel_qweather(
    Extension(deps): Extension<Arc<TravelDeps>>,
    Json(body): Json<QweatherTestBody>,
) -> Result<Json<String>, crate::ai_api::ApiError> {
    if body.api_host.trim().is_empty() {
        return Err(travel_test_error(
            "travel_qweather_test_failed",
            "请先填写和风天气 API Host".to_string(),
        ));
    }
    let provider = devtoolbox_infrastructure::travel::providers::qweather_test_provider(
        &deps.client,
        body.api_key,
        body.api_host,
    );
    let facts = provider
        .fetch(devtoolbox_core::travel::TravelDataRequest {
            city: "北京".to_string(),
            kind: "weather",
        })
        .await
        .map_err(|error| travel_test_error("travel_qweather_test_failed", error.message))?;
    Ok(Json(format!(
        "和风天气连接成功（{}）",
        facts
            .first()
            .map_or("已返回天气数据", |fact| fact.value.as_str())
    )))
}

/// 测试用装配（路由测试不需要真实网络：provider 全部按设置降级）。
#[cfg(test)]
#[must_use]
pub fn test_deps(dir: &std::path::Path) -> TravelDeps {
    TravelDeps {
        client: reqwest::Client::new(),
        store: Arc::new(Mutex::new(
            devtoolbox_infrastructure::TravelStore::open(dir.join("travel-test.db"))
                .expect("travel store"),
        )),
        registry: Arc::new(devtoolbox_application::travel::session::TravelSessionRegistry::new()),
        settings_loader: Arc::new(|| Ok(devtoolbox_core::settings::AppSettings::default())),
    }
}
