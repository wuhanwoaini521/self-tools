//! Files 领域契约（V6 Track C，§40-§48）。
//!
//! 只读边界（§42）：V6 只支持 search / metadata / safe read / open，
//! 不存在 delete / move / rename / write / chmod / execute 的任何 API。
//!
//! 依赖方向：`core::files` 无内部依赖（策略为纯函数）。

pub mod model;

pub use model::{
    DEFAULT_DENY_PATTERNS, FileAccessDenied, FileAccessPolicy, FileContentKind, FileFingerprint,
    FileIndexStats, FileMetadata, FileQuery, FileReadOutcome, KnowledgeRoot, RawFile, display_path,
    extension_of, file_id, file_name_of,
};
