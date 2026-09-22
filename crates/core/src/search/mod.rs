//! 全局检索域（V11 §119-§122）。
//!
//! 只包含**纯契约**：查询、命中、结果与来源枚举。聚合编排（无 LLM、单源降级、
//! 排序与预算）在 application 层（`devtoolbox_application::search`）；
//! 各模块的检索实现经 `GlobalSearchPort` 端口接入，实现在 infrastructure，
//! 组合根在 `apps/desktop`。
//!
//! 依赖方向：`core::search` 无内部依赖（仅 serde / serde_json）。

pub mod model;

pub use model::{
    DEFAULT_LIMIT_PER_SOURCE, MAX_SNIPPET_CHARS, MAX_TOTAL_HITS, GlobalSearchHit,
    GlobalSearchQuery, GlobalSearchResult, SearchSource, bound_snippet,
};
