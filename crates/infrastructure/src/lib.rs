//! 本地文件系统与设置持久化适配器。

pub mod document_store;
pub mod error;
pub mod settings_store;
pub mod workspace_scanner;

pub use document_store::{read_utf8, write_utf8_atomic};
pub use error::InfrastructureError;
pub use settings_store::{AppSettings, SettingsStore};
pub use workspace_scanner::{WorkspaceFile, scan_markdown_files};
