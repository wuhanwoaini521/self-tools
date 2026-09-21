//! Documents 领域契约（V6 Track B，§27-§39）。
//!
//! 文档是**可检索知识源**，不是记忆（§3 Principle 3）：整份文档不进 Memory，
//! 只以 chunk 形式按需检索与读取。
//!
//! 依赖方向：`core::documents` 无内部依赖（分块为纯函数）。

pub mod chunk;
pub mod model;

pub use chunk::{chunk_id, chunk_text};
pub use model::{
    ChunkConfig, DocumentChunk, DocumentFingerprint, DocumentHit, DocumentIndexStats,
    DocumentLocation, DocumentMeta, DocumentReadRequest, DocumentReadResult, DocumentType,
    DocumentVisibility, ExtractedContent, ScannedDocument, detect_document_type, document_id,
    is_indexable_document,
};
