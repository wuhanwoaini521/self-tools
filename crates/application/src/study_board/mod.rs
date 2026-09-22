//! Study Board 用例层（V11 §110-§114）。
//!
//! 端口（`StudyBoardStorePort`）由组合根装配 SQLite 实现；本模块只做
//! 板/快照的编排（读、有界摘要、幂等保存），不含 SQL 与 UI 细节。

pub mod ports;

pub use ports::{StudyBoardStoreError, StudyBoardStorePort, store_error_text};
