//! Documents 持久化（V6 Track B）。

pub mod extract;
pub mod store;

pub use extract::{LocalDocumentSource, is_binary, modified_timestamp};
pub use store::{DOCUMENTS_SCHEMA_VERSION, DocumentIndexError, DocumentIndexSqliteStore};
