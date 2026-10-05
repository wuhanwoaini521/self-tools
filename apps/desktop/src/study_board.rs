//! 学习板（Study Board）的 Tauri 命令。
//!
//! ## 为什么需要这一层
//!
//! 学习板的**存储与用例早已存在**（`StudyBoardSqliteStore` + `StudyBoardService`），
//! 但只有 Personal AI 的四个工具能碰到它。前端画板因此自己往 `localStorage`
//! 里塞了一份 —— 结果是：
//! - 画的东西只活在那个浏览器里（换设备、清缓存就没了）；
//! - AI 看到的 `study-board.list` 与用户眼前的板**不是同一块**。
//!
//! 这里把同一份用例接到 IPC 上：桌面端与网页端（`apps/server`）行为一致，
//! 数据都落在 `config/study_boards.db`。
//!
//! 隐私不变（V11 §113/§114）：笔迹正文只在本机存储与本机传输，
//! 不进日志、不进个人记忆；错误文本只含稳定 reason。

use tauri::State;

use devtoolbox_application::study_board::StudyBoardError;
use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot, StudyBoardSummary};

use crate::{AppState, CommandError};

/// 用例错误 → 命令错误：`code` 稳定（前端可据此提示改输入还是报故障）。
fn study_board_error(error: StudyBoardError) -> CommandError {
    let code = if error.is_invalid_argument() {
        "study_board_invalid"
    } else if error.is_not_found() {
        "study_board_not_found"
    } else {
        "study_board_failed"
    };
    CommandError {
        code,
        message: error.to_string(),
    }
}

/// `study_board_list`：板元数据列表（不含笔迹正文）。
#[tauri::command]
pub async fn study_board_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<StudyBoardSummary>, CommandError> {
    state.study_board.list(limit).map_err(study_board_error)
}

/// `study_board_get`：读一块板（含笔迹；查不到返回 `None`，与桌面端既有习惯一致）。
#[tauri::command]
pub async fn study_board_get(
    state: State<'_, AppState>,
    board_id: String,
) -> Result<Option<StudyBoard>, CommandError> {
    state.study_board.get(&board_id).map_err(study_board_error)
}

/// `study_board_save`：幂等保存（任一字段缺省即保留原值）。
#[tauri::command]
pub async fn study_board_save(
    state: State<'_, AppState>,
    board_id: String,
    title: Option<String>,
    strokes: Option<serde_json::Value>,
    module_origin: Option<String>,
) -> Result<StudyBoard, CommandError> {
    state
        .study_board
        .save(&board_id, title, strokes, module_origin)
        .map(|saved| saved.board)
        .map_err(study_board_error)
}

/// `study_board_snapshot`：登记快照（前端渲染 PNG 后传入 base64）。
#[tauri::command]
pub async fn study_board_snapshot(
    state: State<'_, AppState>,
    board_id: String,
    png_base64: Option<String>,
) -> Result<StudyBoardSnapshot, CommandError> {
    state
        .study_board
        .save_snapshot(&board_id, png_base64)
        .map_err(study_board_error)
}
