//! 场景式口语陪练 HTTP 接口（Phase 2：文本 + 可选音频回合）。

use std::sync::Arc;

use axum::Json;
use axum::extract::Path;
use axum::{Extension, extract::Query};
use devtoolbox_application::language::{
    SpeakingPracticeError, SpeakingPracticeService, SpeakingSessionView,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub type Practice = Arc<SpeakingPracticeService>;

fn clock() -> i64 {
    devtoolbox_infrastructure::now_unix()
}

fn err(code: &'static str, message: String) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError { code, message }
}

fn map_error(error: SpeakingPracticeError) -> crate::ai_api::ApiError {
    match error {
        SpeakingPracticeError::NotFound(message) => err("not_found", message),
        SpeakingPracticeError::EmptyTurn => {
            err("language_speaking_empty_turn", "empty speaking turn".into())
        }
        SpeakingPracticeError::EmptyAudio => err(
            "language_speaking_empty_audio",
            "empty speaking audio".into(),
        ),
        SpeakingPracticeError::AudioDisabled => err(
            "language_speaking_audio_disabled",
            "speaking audio is disabled".into(),
        ),
        SpeakingPracticeError::Invalid(message) => err("language_speaking_invalid", message),
        SpeakingPracticeError::Timeout(message) => err("language_speaking_timeout", message),
        SpeakingPracticeError::AsrUnavailable(message) => {
            err("language_speaking_asr_unavailable", message)
        }
        SpeakingPracticeError::Unavailable(message) => {
            err("language_speaking_llm_unavailable", message)
        }
        SpeakingPracticeError::Storage(message) => err("language_speaking_failed", message),
    }
}

fn view_json(view: SpeakingSessionView) -> Value {
    serde_json::to_value(view).unwrap_or(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionBody {
    pub scenario_id: String,
    #[serde(default)]
    pub cefr: Option<String>,
    #[serde(default)]
    pub locale_assist: Option<String>,
    pub client_request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTurnBody {
    pub text: String,
    pub client_request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAudioBody {
    #[serde(default)]
    pub transcript: Option<String>,
    #[serde(default)]
    pub audio_base64: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    pub client_request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeBody {
    pub turn_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioQuery {
    #[serde(default)]
    pub cefr: Option<String>,
}

/// `GET /api/v1/language/speaking/capabilities`
pub async fn capabilities(
    Extension(practice): Extension<Practice>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    Ok(Json(
        serde_json::to_value(practice.capabilities()).unwrap_or(Value::Null),
    ))
}

/// `GET /api/v1/language/speaking/scenarios`
pub async fn list_scenarios(
    Extension(practice): Extension<Practice>,
    Query(query): Query<ScenarioQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let mut items = practice.list_scenarios().map_err(map_error)?;
    if let Some(cefr) = query
        .cefr
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        items.retain(|item| item.cefr.as_str().eq_ignore_ascii_case(cefr));
    }
    Ok(Json(json!({ "items": items })))
}

/// `GET /api/v1/language/speaking/scenarios/{id}`
pub async fn get_scenario(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let scenario = practice.get_scenario(&id).map_err(map_error)?;
    Ok(Json(serde_json::to_value(scenario).unwrap_or(Value::Null)))
}

/// `POST /api/v1/language/speaking/sessions`
pub async fn create_session(
    Extension(practice): Extension<Practice>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice
        .create_session(
            &body.scenario_id,
            body.cefr.as_deref(),
            body.locale_assist.as_deref().unwrap_or("zh"),
            &body.client_request_id,
            clock(),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `GET /api/v1/language/speaking/sessions/{id}`
pub async fn get_session(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice.get_session(&id).map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/audio`
pub async fn submit_audio(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
    Json(body): Json<SubmitAudioBody>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice
        .submit_audio(
            &id,
            body.transcript.as_deref(),
            body.audio_base64.as_deref(),
            body.mime_type.as_deref(),
            &body.client_request_id,
            clock(),
        )
        .await
        .map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/tts`
pub async fn synthesize(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
    Json(body): Json<SynthesizeBody>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let result = practice
        .synthesize_turn(&id, &body.turn_id)
        .await
        .map_err(map_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or(Value::Null)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/turns`
pub async fn submit_turn(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
    Json(body): Json<SubmitTurnBody>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice
        .submit_turn(&id, &body.text, &body.client_request_id, clock())
        .await
        .map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/pause`
pub async fn pause(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice.pause(&id, clock()).map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/resume`
pub async fn resume(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice.resume(&id, clock()).map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/complete`
pub async fn complete(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice.complete(&id, clock()).map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/interrupt`
pub async fn interrupt(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let view = practice.interrupt(&id, clock()).map_err(map_error)?;
    Ok(Json(view_json(view)))
}

/// `POST /api/v1/language/speaking/sessions/{id}/review`
pub async fn add_review(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let result = practice
        .add_expressions_to_review(&id, clock())
        .map_err(map_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or(Value::Null)))
}

/// `GET /api/v1/language/speaking/sessions/{id}/report`
pub async fn report(
    Extension(practice): Extension<Practice>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let feedback = practice.report(&id).map_err(map_error)?;
    Ok(Json(serde_json::to_value(feedback).unwrap_or(Value::Null)))
}
