//! Stable-ID Geography / History integration endpoints.

use std::sync::Arc;

use axum::{Extension, Json, extract::Path};
use serde::Serialize;

use crate::ai_api::ApiError;
use devtoolbox_application::exploration::ExplorationService;

fn ok<T: Serialize>(value: T) -> Result<Json<serde_json::Value>, ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| ApiError {
            code: "exploration_encode_failed",
            message: error.to_string(),
        })
}

fn backend(error: devtoolbox_application::ApplicationError) -> ApiError {
    ApiError {
        code: "exploration_failed",
        message: error.to_string(),
    }
}

/// `GET /api/v1/exploration/geography/{id}/history`
pub async fn location_history(
    Extension(service): Extension<Arc<ExplorationService>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match service.location_history(&id).map_err(backend)? {
        Some(context) => ok(context),
        None => Err(ApiError {
            code: "not_found",
            message: format!("地理实体不存在：{id}"),
        }),
    }
}

/// `GET /api/v1/exploration/history-places/{id}/geography`
pub async fn geography_for_history_place(
    Extension(service): Extension<Arc<ExplorationService>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match service.geography_for_history_place(&id).map_err(backend)? {
        Some(detail) => ok(Some(detail)),
        None => ok(None::<serde_json::Value>),
    }
}
