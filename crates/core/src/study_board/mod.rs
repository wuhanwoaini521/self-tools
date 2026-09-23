//! Study Board（学习板）—— 纯领域契约（V11 §107-§114）。
//!
//! 边界：
//! - 后端**不解释笔迹**：`strokes` 是不透明的 JSON（矢量笔画数据，由前端渲染）；
//!   backend 只负责原样存储、有界摘要与快照登记（V11 §109-§111）。
//! - **隐私铁律（V11 §113/§114）**：`strokes` 与快照是用户私有数据 ——
//!   不得自动提升为 Personal Memory，不得进入任何日志 / 遥测 / 错误文本；
//!   工具结果只暴露有界摘要（≤ 2000 字符）与引用，不回传整份笔迹。
//! - 本模块无 IO：持久化在 `infrastructure::study_board`（SQLite），
//!   工具与 ContextProvider 在 `application::personal_ai::study_board`。

pub mod model;

pub use model::{
    STROKE_SUMMARY_MAX_CHARS, STUDY_BOARD_MAX_TITLE_CHARS, STUDY_BOARD_MODULE_ID, StudyBoard,
    StudyBoardSnapshot, StudyBoardSummary, bounded_strokes_summary, is_valid_board_id,
    strokes_summary,
};
