//! 口语陪练 Tauri 命令（与 HTTP 同一套 `SpeakingPracticeService`）。

use std::sync::Arc;

use tauri::{AppHandle, State};

use devtoolbox_application::language::{
    ReviewImportResult, SpeakingAudioCapabilities, SpeakingPracticeError, SpeakingPracticeService,
    SpeakingSessionView, TtsResult,
};
use devtoolbox_application::learning::LearningService;
use devtoolbox_core::language::{SpeakingFeedback, SpeakingScenario};
use devtoolbox_runtime::composition::CourseStoreAdapter;

use crate::{AppState, CommandError, load_ai_settings};

fn now() -> i64 {
    devtoolbox_infrastructure::now_unix()
}

fn practice_service(
    app: &AppHandle,
    state: &State<'_, AppState>,
) -> Result<SpeakingPracticeService, CommandError> {
    let store = Arc::new(
        devtoolbox_runtime::composition::SpeakingPracticeStoreAdapter::new(Arc::clone(
            &state.language_store,
        )),
    );
    let llm = load_ai_settings(app)
        .ok()
        .filter(|ai| ai.is_configured())
        .map(|ai| devtoolbox_runtime::personal_ai::build_provider(state.client.clone(), &ai));
    let dictionary = Arc::new(CourseStoreAdapter::new(Arc::clone(&state.language_store)));
    let learning = Arc::new(LearningService::new(Arc::clone(&state.learning_store)));
    Ok(SpeakingPracticeService::new(store, llm)
        .with_dictionary(dictionary)
        .with_learning(learning))
}

fn map_error(error: SpeakingPracticeError) -> CommandError {
    CommandError {
        code: match &error {
            SpeakingPracticeError::NotFound(_) => "not_found",
            SpeakingPracticeError::EmptyTurn => "language_speaking_empty_turn",
            SpeakingPracticeError::EmptyAudio => "language_speaking_empty_audio",
            SpeakingPracticeError::AudioDisabled => "language_speaking_audio_disabled",
            SpeakingPracticeError::Invalid(_) => "language_speaking_invalid",
            SpeakingPracticeError::Timeout(_) => "language_speaking_timeout",
            SpeakingPracticeError::AsrUnavailable(_) => "language_speaking_asr_unavailable",
            SpeakingPracticeError::Unavailable(_) => "language_speaking_llm_unavailable",
            SpeakingPracticeError::Storage(_) => "language_speaking_failed",
        },
        message: error.to_string(),
    }
}

#[tauri::command]
pub fn language_speaking_capabilities(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SpeakingAudioCapabilities, CommandError> {
    Ok(practice_service(&app, &state)?.capabilities())
}

#[tauri::command]
pub fn language_speaking_scenarios(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SpeakingScenario>, CommandError> {
    practice_service(&app, &state)?
        .list_scenarios()
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_scenario(
    app: AppHandle,
    state: State<'_, AppState>,
    scenario_id: String,
) -> Result<Option<SpeakingScenario>, CommandError> {
    match practice_service(&app, &state)?.get_scenario(&scenario_id) {
        Ok(scenario) => Ok(Some(scenario)),
        Err(SpeakingPracticeError::NotFound(_)) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

#[tauri::command]
pub async fn language_speaking_create_session(
    app: AppHandle,
    state: State<'_, AppState>,
    scenario_id: String,
    cefr: Option<String>,
    locale_assist: Option<String>,
    client_request_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .create_session(
            &scenario_id,
            cefr.as_deref(),
            locale_assist.as_deref().unwrap_or("zh"),
            &client_request_id,
            now(),
        )
        .await
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<SpeakingSessionView>, CommandError> {
    match practice_service(&app, &state)?.get_session(&session_id) {
        Ok(view) => Ok(Some(view)),
        Err(SpeakingPracticeError::NotFound(_)) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

#[tauri::command]
pub async fn language_speaking_submit_audio(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    transcript: Option<String>,
    audio_base64: Option<String>,
    mime_type: Option<String>,
    client_request_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .submit_audio(
            &session_id,
            transcript.as_deref(),
            audio_base64.as_deref(),
            mime_type.as_deref(),
            &client_request_id,
            now(),
        )
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn language_speaking_tts(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    turn_id: String,
) -> Result<TtsResult, CommandError> {
    practice_service(&app, &state)?
        .synthesize_turn(&session_id, &turn_id)
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn language_speaking_submit_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    client_request_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .submit_turn(&session_id, &text, &client_request_id, now())
        .await
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_pause(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .pause(&session_id, now())
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_resume(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .resume(&session_id, now())
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_complete(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .complete(&session_id, now())
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_report(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<SpeakingFeedback>, CommandError> {
    match practice_service(&app, &state)?.report(&session_id) {
        Ok(report) => Ok(Some(report)),
        Err(SpeakingPracticeError::NotFound(_)) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

#[tauri::command]
pub fn language_speaking_interrupt(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SpeakingSessionView, CommandError> {
    practice_service(&app, &state)?
        .interrupt(&session_id, now())
        .map_err(map_error)
}

#[tauri::command]
pub fn language_speaking_add_review(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ReviewImportResult, CommandError> {
    practice_service(&app, &state)?
        .add_expressions_to_review(&session_id, now())
        .map_err(map_error)
}
