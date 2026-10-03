//! 共享端口适配器。
//!
//! 把 infrastructure 的 SQLite store 包装成 application 的 port trait，
//! 供 `apps/desktop` 与 `apps/server` 共用——避免两端各写一份、语义漂移。

pub mod language;
pub mod learning;

pub use language::LanguageStoreAdapter;
pub use learning::LearningStoreAdapter;
