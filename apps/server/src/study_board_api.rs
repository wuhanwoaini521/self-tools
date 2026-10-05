//! 学习板（Study Board）的 HTTP 接口。
//!
//! ## 为什么需要这一层
//!
//! 画板是唯一一个**存储与用例都存在、却只有 AI 能碰到**的模块：
//! `StudyBoardSqliteStore` + `StudyBoardService` 早就在 Rust 侧，
//! agent 工具 `study-board.save` 也能存，但前端画板一直只往 `localStorage` 写。
//! 于是网页端画的板：换浏览器就没、跟 AI 看到的那块也不是同一块。
//!
//! 这里把**同一份用例**接出来（与桌面端命令一对一），数据仍落在
//! `<data_dir>/study_boards.db` —— 网页端与桌面端看到的是同一批板。
//!
//! 隐私边界不变（V11 §113/§114）：笔迹只在本机存储与本机回环传输，
//! 不进日志、不进个人记忆；错误体只含稳定 `code` 与受控文本。

use std::sync::Arc;

use axum::extract::Path;
use axum::{Extension, Json};
use devtoolbox_application::study_board::{StudyBoardError, StudyBoardService};
use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot};
use serde::Deserialize;
use serde_json::Value;

/// 用例服务（由组合根 `AppCore::study_board` 注入）。
pub type SharedStudyBoard = Extension<Arc<StudyBoardService>>;

/// `POST /api/v1/study-boards` 请求体。
///
/// 前端（transport）统一发 camelCase；snake_case 别名一并接受，
/// 免得换个调用方（脚本 / curl）就 422。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBoardRequest {
    #[serde(alias = "board_id")]
    pub board_id: String,
    /// 缺省 = 保留原标题（新板则报错：没有标题的板在列表里只是空白）。
    #[serde(default)]
    pub title: Option<String>,
    /// 矢量笔迹；**后端不透明**（原样存取，不解析、不解释）。
    #[serde(default)]
    pub strokes: Option<Value>,
    /// 来源模块（如 `history` / `language`）。
    #[serde(default, alias = "module_origin")]
    pub module_origin: Option<String>,
}

/// `POST /api/v1/study-boards/{id}/snapshots` 请求体。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRequest {
    /// 前端渲染的 PNG（base64）；缺省则只登记有界摘要。
    #[serde(default, alias = "png_base64")]
    pub png_base64: Option<String>,
}

/// `GET /api/v1/study-boards?limit=…`：板元数据列表（不含笔迹正文）。
pub async fn list_boards(
    Extension(service): SharedStudyBoard,
    axum::extract::Query(params): axum::extract::Query<ListQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let items = service.list(params.limit).map_err(api_error)?;
    Ok(Json(
        serde_json::to_value(items).map_err(serialization_error)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
}

/// `GET /api/v1/study-boards/{id}`：读一块板（含笔迹；查不到 → 404）。
pub async fn get_board(
    Extension(service): SharedStudyBoard,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    match service.get(&id).map_err(api_error)? {
        Some(board) => Ok(Json(board_json(&board)?)),
        None => Err(crate::ai_api::ApiError {
            code: "not_found",
            message: format!("学习板「{id}」不存在"),
        }),
    }
}

/// `POST /api/v1/study-boards`：幂等保存（按 `board_id` upsert）。
pub async fn save_board(
    Extension(service): SharedStudyBoard,
    Json(request): Json<SaveBoardRequest>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let saved = service
        .save(
            &request.board_id,
            request.title,
            request.strokes,
            request.module_origin,
        )
        .map_err(api_error)?;
    Ok(Json(serde_json::json!({
        "board": board_json(&saved.board)?,
        "created": saved.created,
    })))
}

/// `POST /api/v1/study-boards/{id}/snapshots`：登记快照（后端不生成图像）。
pub async fn save_snapshot(
    Extension(service): SharedStudyBoard,
    Path(id): Path<String>,
    Json(request): Json<SnapshotRequest>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let snapshot: StudyBoardSnapshot = service
        .save_snapshot(&id, request.png_base64)
        .map_err(api_error)?;
    Ok(Json(serde_json::json!({
        "snapshot_id": snapshot.id,
        "board_id": snapshot.board_id,
        "title": snapshot.title,
        "strokes_summary": snapshot.strokes_summary,
        "created_at": snapshot.created_at,
        "has_png": snapshot.png_base64.is_some(),
    })))
}

/// 板的 JSON 形状（与 `core::study_board::StudyBoard` 一致，前端直接用）。
fn board_json(board: &StudyBoard) -> Result<Value, crate::ai_api::ApiError> {
    serde_json::to_value(board).map_err(serialization_error)
}

fn serialization_error(error: serde_json::Error) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError {
        code: "study_board_failed",
        message: error.to_string(),
    }
}

/// 用例错误 → HTTP 错误契约。
///
/// `code` 稳定：参数问题 400、找不到 404、存储故障 500 —— 前端可据此区分
/// 「你写错了」与「服务坏了」，而不是一律显示「保存失败」。
fn api_error(error: StudyBoardError) -> crate::ai_api::ApiError {
    let code = if error.is_invalid_argument() {
        "invalid"
    } else if error.is_not_found() {
        "not_found"
    } else {
        "study_board_failed"
    };
    crate::ai_api::ApiError {
        code,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_application::study_board::StudyBoardStorePort;
    use devtoolbox_core::study_board::StudyBoardSummary;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeStore {
        boards: Mutex<HashMap<String, StudyBoard>>,
    }

    impl StudyBoardStorePort for FakeStore {
        fn upsert_board(
            &self,
            board: &StudyBoard,
        ) -> Result<(), devtoolbox_application::study_board::StudyBoardStoreError> {
            self.boards.lock().insert(board.id.clone(), board.clone());
            Ok(())
        }
        fn get_board(
            &self,
            id: &str,
        ) -> Result<Option<StudyBoard>, devtoolbox_application::study_board::StudyBoardStoreError>
        {
            Ok(self.boards.lock().get(id).cloned())
        }
        fn list_boards(
            &self,
            limit: usize,
        ) -> Result<Vec<StudyBoardSummary>, devtoolbox_application::study_board::StudyBoardStoreError>
        {
            Ok(self
                .boards
                .lock()
                .values()
                .take(limit)
                .map(StudyBoard::summary)
                .collect())
        }
        fn upsert_snapshot(
            &self,
            _snapshot: &StudyBoardSnapshot,
        ) -> Result<(), devtoolbox_application::study_board::StudyBoardStoreError> {
            Ok(())
        }
        fn get_snapshot(
            &self,
            _id: &str,
        ) -> Result<
            Option<StudyBoardSnapshot>,
            devtoolbox_application::study_board::StudyBoardStoreError,
        > {
            Ok(None)
        }
        fn latest_snapshot(
            &self,
            _board_id: &str,
        ) -> Result<
            Option<StudyBoardSnapshot>,
            devtoolbox_application::study_board::StudyBoardStoreError,
        > {
            Ok(None)
        }
    }

    fn service() -> StudyBoardService {
        StudyBoardService::new(Arc::new(FakeStore::default()))
    }

    #[test]
    fn invalid_argument_is_400_and_missing_board_is_404() {
        assert_eq!(api_error(StudyBoardError::MissingContent).code, "invalid");
        assert_eq!(
            api_error(StudyBoardError::InvalidBoardId("../x".into())).code,
            "invalid"
        );
        assert_eq!(
            api_error(StudyBoardError::NotFound("b-1".into())).code,
            "not_found"
        );
        assert_eq!(
            api_error(StudyBoardError::Store {
                reason: "board_save_failed",
                message: "disk".into(),
            })
            .code,
            "study_board_failed"
        );
    }

    #[test]
    fn error_message_never_contains_stroke_payload() {
        let error = api_error(StudyBoardError::Store {
            reason: "board_save_failed",
            message: "disk full".into(),
        });
        assert!(!error.message.contains("points"));
        assert!(error.message.starts_with("board_save_failed"));
    }

    #[test]
    fn board_json_round_trips_strokes_untouched() {
        let service = service();
        let strokes = serde_json::json!({"strokes": [{"points": [[1, 2]], "color": "#000"}]});
        let saved = service
            .save("b-1", Some("板".into()), Some(strokes.clone()), None)
            .expect("save");
        let json = board_json(&saved.board).expect("json");
        assert_eq!(json["strokes"], strokes, "笔迹原样存取，后端不解释");
        assert_eq!(json["title"], "板");
    }
}
