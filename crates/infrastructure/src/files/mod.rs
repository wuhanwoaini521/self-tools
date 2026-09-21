//! Files 持久化与文件系统访问（V6 Track C，只读）。

pub mod fs;
pub mod store;

pub use fs::LocalFileSystem;
pub use store::{FILES_SCHEMA_VERSION, FileIndexError, FileIndexSqliteStore};
