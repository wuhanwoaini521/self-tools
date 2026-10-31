//! Versioned HTTP contract for application-level backup and recovery.
//!
//! These routes run only on the loopback server and inherit its Host/Origin guard.
//! The runtime manager owns snapshot and startup-recovery behavior; handlers expose
//! stable DTOs and do not return database paths or provider settings.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use devtoolbox_runtime::backup::{
    ApplicationBackupManager, ApplicationBackupSummary, RestoreReceipt, RestoreStatus,
    StageRestoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const RESTORE_CONFIRMATION: &str = "apply_on_next_clean_startup";

#[derive(Debug, Deserialize)]
struct CreateBackupRequest {
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
struct RestoreRequest {
    confirmation: String,
}

#[derive(Debug, Serialize)]
struct BackupResponse {
    id: String,
    created_at: i64,
    app_version: String,
    format_version: u32,
    complete: bool,
    entry_count: usize,
    total_bytes: u64,
    has_sensitive_entries: bool,
    note: String,
}

impl From<ApplicationBackupSummary> for BackupResponse {
    fn from(summary: ApplicationBackupSummary) -> Self {
        Self {
            id: summary.id,
            created_at: summary.created_at,
            app_version: summary.app_version,
            format_version: summary.format_version,
            complete: summary.complete,
            entry_count: summary.entry_count,
            total_bytes: summary.total_bytes,
            has_sensitive_entries: summary.has_sensitive_entries,
            note: summary.note,
        }
    }
}

#[derive(Debug, Serialize)]
struct BackupListResponse {
    backups: Vec<BackupResponse>,
}

#[derive(Debug, Serialize)]
struct BackupCreatedResponse {
    backup: BackupResponse,
}

#[derive(Debug, Serialize)]
struct RestoreStatusResponse {
    state: String,
    restore_id: Option<String>,
    backup_id: Option<String>,
    safety_backup_id: Option<String>,
    message: Option<String>,
}

impl From<RestoreStatus> for RestoreStatusResponse {
    fn from(status: RestoreStatus) -> Self {
        Self {
            state: status.state,
            restore_id: status.restore_id,
            backup_id: status.backup_id,
            safety_backup_id: status.safety_backup_id,
            message: status.message,
        }
    }
}

#[derive(Debug, Serialize)]
struct RestoreAcceptedResponse {
    restore_id: String,
    backup_id: String,
    state: &'static str,
    requires_restart: bool,
}

impl From<RestoreReceipt> for RestoreAcceptedResponse {
    fn from(receipt: RestoreReceipt) -> Self {
        Self {
            restore_id: receipt.restore_id,
            backup_id: receipt.backup_id,
            state: "pending_restart",
            requires_restart: receipt.requires_restart,
        }
    }
}

/// Build the `/api/v1/backups` contract for one application data directory.
pub fn router<S>(data_directory: PathBuf) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let manager = Arc::new(ApplicationBackupManager::new(data_directory));
    Router::new()
        .route("/api/v1/backups", get(list_backups).post(create_backup))
        .route("/api/v1/backups/restore-status", get(restore_status))
        .route("/api/v1/backups/{backup_id}/restore", post(stage_restore))
        .layer(Extension(manager))
}

async fn list_backups(Extension(manager): Extension<Arc<ApplicationBackupManager>>) -> Response {
    match tokio::task::spawn_blocking(move || manager.list_backups()).await {
        Ok(Ok(backups)) => Json(BackupListResponse {
            backups: backups.into_iter().map(BackupResponse::from).collect(),
        })
        .into_response(),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "backup_list_failed",
            "Backup list is unavailable.",
        ),
    }
}

async fn create_backup(
    Extension(manager): Extension<Arc<ApplicationBackupManager>>,
    request: Result<Json<CreateBackupRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.note.chars().count() > 240 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "backup_note_too_long",
            "Backup note must be 240 characters or fewer.",
        );
    }
    match tokio::task::spawn_blocking(move || manager.create_backup(&request.note)).await {
        Ok(Ok(backup)) => (
            StatusCode::CREATED,
            Json(BackupCreatedResponse {
                backup: backup.into(),
            }),
        )
            .into_response(),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "backup_create_failed",
            "Application backup could not be created.",
        ),
    }
}

async fn restore_status(Extension(manager): Extension<Arc<ApplicationBackupManager>>) -> Response {
    match tokio::task::spawn_blocking(move || manager.restore_status()).await {
        Ok(Ok(status)) => Json(RestoreStatusResponse::from(status)).into_response(),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "restore_status_failed",
            "Restore status is unavailable.",
        ),
    }
}

async fn stage_restore(
    Extension(manager): Extension<Arc<ApplicationBackupManager>>,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<RestoreRequest>, JsonRejection>,
) -> Response {
    let Path(backup_id) = match path {
        Ok(path) => path,
        Err(_) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "invalid_backup_id",
                "Backup identifier is invalid.",
            );
        }
    };
    let Json(request) = match request {
        Ok(request) => request,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.confirmation != RESTORE_CONFIRMATION {
        return api_error(
            StatusCode::BAD_REQUEST,
            "restore_confirmation_required",
            "Set confirmation to apply_on_next_clean_startup to stage a restore.",
        );
    }
    let manager_for_lookup = Arc::clone(&manager);
    let backups = match tokio::task::spawn_blocking(move || manager_for_lookup.list_backups()).await
    {
        Ok(Ok(backups)) => backups,
        _ => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "backup_list_failed",
                "Backup list is unavailable.",
            );
        }
    };
    let Some(backup) = backups.iter().find(|backup| backup.id == backup_id) else {
        return api_error(
            StatusCode::NOT_FOUND,
            "backup_not_found",
            "Application backup was not found.",
        );
    };
    if !backup.complete || backup.format_version != 1 {
        return api_error(
            StatusCode::CONFLICT,
            "backup_not_restorable",
            "Only complete backups using a supported format can be restored.",
        );
    }

    match tokio::task::spawn_blocking(move || manager.stage_restore(&backup_id, true)).await {
        Ok(Ok(receipt)) => (
            StatusCode::ACCEPTED,
            Json(RestoreAcceptedResponse::from(receipt)),
        )
            .into_response(),
        Ok(Err(StageRestoreError::AlreadyPending)) => api_error(
            StatusCode::CONFLICT,
            "restore_already_pending",
            "Another restore is already pending.",
        ),
        Ok(Err(StageRestoreError::InvalidBackup(_))) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "restore_could_not_be_staged",
            "Application backup failed verification and was not staged.",
        ),
        Ok(Err(StageRestoreError::ConfirmationRequired | StageRestoreError::InvalidId)) => {
            api_error(
                StatusCode::BAD_REQUEST,
                "invalid_restore_request",
                "Restore request is invalid.",
            )
        }
        Ok(Err(StageRestoreError::Storage(_))) | Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "restore_staging_failed",
            "The restore request could not be staged because of a server error.",
        ),
    }
}

fn json_rejection(rejection: JsonRejection) -> Response {
    let status = rejection.status();
    let (code, message) = match status {
        StatusCode::PAYLOAD_TOO_LARGE => (
            "request_body_too_large",
            "Request body exceeds the server limit.",
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            "unsupported_content_type",
            "Request must use application/json.",
        ),
        _ => ("invalid_json", "Request body is not valid JSON."),
    };
    api_error(status, code, message)
}

fn api_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use rusqlite::Connection;
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::router;
    use crate::security::local_http_guard;

    fn fixture_database(directory: &Path, value: &str) {
        let connection = Connection::open(directory.join("learning.db")).expect("fixture database");
        connection
            .execute_batch("CREATE TABLE state (value TEXT NOT NULL);")
            .expect("fixture schema");
        connection
            .execute("INSERT INTO state (value) VALUES (?1)", [value])
            .expect("fixture value");
    }

    fn set_fixture_value(directory: &Path, value: &str) {
        Connection::open(directory.join("learning.db"))
            .expect("fixture database")
            .execute("UPDATE state SET value = ?1", [value])
            .expect("update fixture");
    }

    fn fixture_value(directory: &Path) -> String {
        Connection::open(directory.join("learning.db"))
            .expect("fixture database")
            .query_row("SELECT value FROM state", [], |row| row.get(0))
            .expect("fixture value")
    }

    async fn send(app: &axum::Router, request: Request<Body>) -> axum::response::Response {
        app.clone().oneshot(request).await.expect("route response")
    }

    fn local_request(method: &str, uri: &str, body: Body) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "127.0.0.1:8080")
            .header("origin", "http://127.0.0.1:5173")
            .header("content-type", "application/json")
            .body(body)
            .expect("request")
    }

    #[tokio::test]
    async fn versioned_api_creates_lists_and_stages_restore_without_touching_live_data() {
        let home = tempdir().expect("temporary data directory");
        fixture_database(home.path(), "from-backup");
        let app =
            router(home.path().to_path_buf()).layer(axum::middleware::from_fn(local_http_guard));

        let created = send(
            &app,
            local_request(
                "POST",
                "/api/v1/backups",
                Body::from(r#"{"note":"contract test"}"#),
            ),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let created_body = to_bytes(created.into_body(), 1024 * 1024)
            .await
            .expect("created response body");
        let created_json: serde_json::Value =
            serde_json::from_slice(&created_body).expect("created response JSON");
        let backup_id = created_json["backup"]["id"]
            .as_str()
            .expect("backup ID")
            .to_string();
        assert_eq!(created_json["backup"]["format_version"], 1);
        assert!(
            created_json["backup"]["complete"]
                .as_bool()
                .expect("complete field")
        );

        let listed = send(&app, local_request("GET", "/api/v1/backups", Body::empty())).await;
        assert_eq!(listed.status(), StatusCode::OK);
        let listed_body = to_bytes(listed.into_body(), 1024 * 1024)
            .await
            .expect("backup list body");
        let listed_json: serde_json::Value =
            serde_json::from_slice(&listed_body).expect("backup list JSON");
        assert_eq!(listed_json["backups"][0]["id"], backup_id);

        set_fixture_value(home.path(), "current-live-value");
        let restore = send(
            &app,
            local_request(
                "POST",
                &format!("/api/v1/backups/{backup_id}/restore"),
                Body::from(r#"{"confirmation":"apply_on_next_clean_startup"}"#),
            ),
        )
        .await;
        assert_eq!(restore.status(), StatusCode::ACCEPTED);
        let restore_body = to_bytes(restore.into_body(), 1024 * 1024)
            .await
            .expect("restore response body");
        let restore_json: serde_json::Value =
            serde_json::from_slice(&restore_body).expect("restore response JSON");
        assert_eq!(restore_json["state"], "pending_restart");
        assert!(
            restore_json["requires_restart"]
                .as_bool()
                .expect("restart field")
        );
        assert_eq!(fixture_value(home.path()), "current-live-value");

        let status = send(
            &app,
            local_request("GET", "/api/v1/backups/restore-status", Body::empty()),
        )
        .await;
        assert_eq!(status.status(), StatusCode::OK);
        let status_body = to_bytes(status.into_body(), 1024 * 1024)
            .await
            .expect("status response body");
        let status_json: serde_json::Value =
            serde_json::from_slice(&status_body).expect("status response JSON");
        assert_eq!(status_json["state"], "pending_restart");

        let _data_lock = devtoolbox_runtime::backup::AppDataDirectoryLock::acquire(home.path())
            .expect("clean startup applies restore");
        assert_eq!(fixture_value(home.path()), "from-backup");
    }

    #[tokio::test]
    async fn versioned_restore_rejects_external_browser_origin() {
        let home = tempdir().expect("temporary data directory");
        let app =
            router(home.path().to_path_buf()).layer(axum::middleware::from_fn(local_http_guard));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/backups/00000000-0000-4000-8000-000000000000/restore")
                    .header("host", "127.0.0.1:8080")
                    .header("origin", "https://attacker.example")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"confirmation":"apply_on_next_clean_startup"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("route response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn malformed_json_uses_the_versioned_error_envelope() {
        let home = tempdir().expect("temporary data directory");
        let app = router(home.path().to_path_buf());
        let response = send(
            &app,
            local_request("POST", "/api/v1/backups", Body::from("{")),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("error response body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("error response JSON");
        assert_eq!(value["code"], "invalid_json");
        assert!(value["message"].is_string());
    }

    #[tokio::test]
    async fn restore_requires_explicit_confirmation() {
        let home = tempdir().expect("temporary data directory");
        fixture_database(home.path(), "value");
        let app = router(home.path().to_path_buf());
        let created = send(
            &app,
            local_request("POST", "/api/v1/backups", Body::from("{}")),
        )
        .await;
        let body = to_bytes(created.into_body(), 1024 * 1024)
            .await
            .expect("response body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("response JSON");
        let backup_id = value["backup"]["id"].as_str().expect("backup ID");
        let response = send(
            &app,
            local_request(
                "POST",
                &format!("/api/v1/backups/{backup_id}/restore"),
                Body::from(r#"{"confirmation":"no"}"#),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
