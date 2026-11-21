//! History AI 解读的 HTTP 接口（网页端 / pad）。
//!
//! 桌面端 `history_enrichment_*` 5 个命令此前没有 HTTP 等价，导致 Web 上的
//! 「AI 解读」永远停在 MISSING。这里直接复用共享的 `EnrichmentRunnerPort`，
//! 与桌面端读写同一个 `config/history_enrichment.db`。

use std::sync::Arc;

use axum::extract::Query;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{Value, json};

use devtoolbox_application::history::enrichment::ports::EnrichmentRunnerPort;
use devtoolbox_core::history_enrichment::{EnrichmentKey, EnrichmentSection};

use crate::ai_api::ApiError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityQuery {
    pub entity_type: String,
    pub entity_id: String,
    pub locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionQuery {
    pub entity_type: String,
    pub entity_id: String,
    pub section: String,
    pub locale: String,
}

type SectionBody = SectionQuery;

fn bad(message: impl Into<String>) -> ApiError {
    ApiError {
        code: "invalid",
        message: message.into(),
    }
}

fn failure(message: String) -> ApiError {
    ApiError {
        code: "history_enrichment_error",
        message,
    }
}

fn parse_section(value: &str) -> Result<EnrichmentSection, ApiError> {
    match value {
        "overview" => Ok(EnrichmentSection::Overview),
        "background" => Ok(EnrichmentSection::Background),
        "impact" => Ok(EnrichmentSection::Impact),
        other => Err(bad(format!("unknown enrichment section: {other}"))),
    }
}

fn parse_key(body: &SectionBody) -> Result<EnrichmentKey, ApiError> {
    if body.entity_type != "event" || body.entity_id.trim().is_empty() {
        return Err(bad("enrichment currently supports only non-empty event ids"));
    }
    Ok(EnrichmentKey::new(
        "event",
        &body.entity_id,
        parse_section(&body.section)?,
        &body.locale,
    ))
}

/// `GET /api/v1/history/enrichment/state?entityType=&entityId=&locale=`
pub async fn state(
    Extension(runner): Extension<Arc<dyn EnrichmentRunnerPort>>,
    Query(query): Query<EntityQuery>,
) -> Result<Json<Value>, ApiError> {
    let sections = runner
        .sections(&query.entity_type, &query.entity_id, &query.locale)
        .map_err(failure)?;
    let items: Vec<Value> = sections
        .into_iter()
        .map(|(section, state)| {
            json!({
                "section": section.as_str(),
                "state": match state {
                    devtoolbox_core::history_enrichment::EnrichmentState::Missing => "MISSING",
                    devtoolbox_core::history_enrichment::EnrichmentState::Generating => "GENERATING",
                    devtoolbox_core::history_enrichment::EnrichmentState::Ready => "READY",
                    devtoolbox_core::history_enrichment::EnrichmentState::Stale => "STALE",
                    devtoolbox_core::history_enrichment::EnrichmentState::Failed => "FAILED",
                    devtoolbox_core::history_enrichment::EnrichmentState::Reviewed => "REVIEWED",
                },
            })
        })
        .collect();
    Ok(Json(json!(items)))
}

/// `GET /api/v1/history/enrichment/view?...`
pub async fn view(
    Extension(runner): Extension<Arc<dyn EnrichmentRunnerPort>>,
    Query(query): Query<SectionQuery>,
) -> Result<Json<Value>, ApiError> {
    let key = parse_key(&query)?;
    let view = runner.get(&key).map_err(failure)?;
    Ok(Json(json!(view)))
}

/// `POST /api/v1/history/enrichment/ensure`
pub async fn ensure(
    Extension(runner): Extension<Arc<dyn EnrichmentRunnerPort>>,
    Json(body): Json<SectionBody>,
) -> Result<Json<Value>, ApiError> {
    let key = parse_key(&body)?;
    let view = runner.ensure(&key).await.map_err(failure)?;
    Ok(Json(json!(view)))
}

/// `POST /api/v1/history/enrichment/refresh`
pub async fn refresh(
    Extension(runner): Extension<Arc<dyn EnrichmentRunnerPort>>,
    Json(body): Json<SectionBody>,
) -> Result<Json<Value>, ApiError> {
    let key = parse_key(&body)?;
    let view = runner.refresh(&key).await.map_err(failure)?;
    Ok(Json(json!(view)))
}

/// `POST /api/v1/history/enrichment/review`
pub async fn review(
    Extension(runner): Extension<Arc<dyn EnrichmentRunnerPort>>,
    Json(body): Json<SectionBody>,
) -> Result<Json<Value>, ApiError> {
    let key = parse_key(&body)?;
    runner.mark_reviewed(&key).map_err(failure)?;
    Ok(Json(json!({ "ok": true })))
}
