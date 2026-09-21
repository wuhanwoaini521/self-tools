//! Personal Memory — 纯领域契约（V6 Track A，§8-§23）。
//!
//! 只包含数据模型（`MemoryItem` 等）与确定性写入 gate（secret 检测 / 显式意图 /
//! 状态决策）。持久化与 IO 在 infrastructure，检索编排在 application。
//!
//! 依赖方向：`core::memory` 无内部依赖（仅 serde / regex）。

pub mod gate;
pub mod model;

pub use gate::{
    MEMORY_MAX_CONTENT_CHARS, MEMORY_MIN_CONFIDENCE, SecretKind, WriteDecision, WriteIntent,
    detect_explicit_save_intent, detect_secret, extract_save_content, resolve_status,
    source_type_for, validate_draft,
};
pub use model::{
    MemoryCategory, MemoryDraft, MemoryItem, MemoryQuery, MemorySensitivity, MemorySourceType,
    MemoryStatus,
};
