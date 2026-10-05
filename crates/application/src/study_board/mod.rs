//! Study Board 用例层（V11 §110-§114）。
//!
//! 端口（`StudyBoardStorePort`）由组合根装配 SQLite 实现；本模块只做
//! 板/快照的编排（读、有界摘要、幂等保存），不含 SQL 与 UI 细节。
//!
//! 用例在 [`StudyBoardService`]：**agent 工具与前端命令共用同一份编排**，
//! 因此「AI 保存的那块板」和「用户画的那块板」永远是同一块。

pub mod ports;
pub mod service;

pub use ports::{StudyBoardStoreError, StudyBoardStorePort, store_error_text};
pub use service::{
    STUDY_BOARD_LIST_DEFAULT_LIMIT, STUDY_BOARD_LIST_MAX_LIMIT, SavedBoard, StudyBoardError,
    StudyBoardService,
};
