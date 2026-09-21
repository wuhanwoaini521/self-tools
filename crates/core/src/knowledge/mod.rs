//! Personal Knowledge 统一契约（V6 Track D，§52-§69）。
//!
//! 依赖方向：`core::knowledge` 无内部依赖。它只表达「统一结果形状」，
//! 不持有任何知识源的实现（Memory / Documents / Files 各有自己的模块与表）。

pub mod model;

pub use model::{
    KnowledgeBudget, KnowledgeDiagnostics, KnowledgeQuery, KnowledgeResult,
    KnowledgeRetrievalOutcome, KnowledgeSourceKind, Provenance, snippet, stable_id,
};
