//! Study Board（学习板）持久化适配（V11 §112）。
//!
//! `infrastructure` 只做实现：SQLite 读写，不持有端口类型、不含业务规则。
//! 端口在 application（`personal_ai::study_board` 的 `StudyBoardStorePort`），
//! 装配在组合根（与 memory / documents 同构）。

pub mod store;

pub use store::{STUDY_BOARD_SCHEMA_VERSION, StudyBoardSqliteStore};
