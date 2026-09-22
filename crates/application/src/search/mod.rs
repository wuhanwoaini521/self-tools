//! 全局检索用例（V11 §119-§122）。
//!
//! 一个入口聚合全部模块的检索结果，**完全不经过 LLM**：查询是确定性的，
//! 不调用任何 `ChatModelProvider`，因此模型不可用时功能照常可用。
//!
//! 编排规则：
//! - 各模块经 [`GlobalSearchPort`] 接入，实现在 infrastructure，组合根在 `apps/desktop`；
//! - 单源失败只进 `degraded_sources`，其余源继续返回；
//! - 命中按分数倒序，按来源裁剪（`limit_per_source`），总数硬截断（`MAX_TOTAL_HITS`）；
//! - 查询为空白时返回空结果（不是错误）。

pub mod ports;
pub mod service;

pub use ports::GlobalSearchPort;
pub use service::GlobalSearchService;

#[cfg(test)]
mod tests;
