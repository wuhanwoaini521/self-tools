//! 全局检索端口（V11 §119-§122）。
//!
//! 端口属于用例层（application）：各模块的检索实现在 infrastructure，
//! 组合根（`apps/desktop`）负责把已装配好的模块服务包一层适配器注册进来。
//! 用例层不感知任何存储 / HTTP / SQL 细节。
//!
//! 端口**不含 LLM**：实现方不得在 `search` 内部调用模型，只做本地检索。

use devtoolbox_core::search::{GlobalSearchHit, GlobalSearchQuery, SearchSource};

/// 单个来源的全局检索器。
pub trait GlobalSearchPort: Send + Sync {
    /// 该适配器负责的来源。
    fn source(&self) -> SearchSource;

    /// 执行本地检索（返回候选，可多于最终上限；聚合层负责裁剪）。
    ///
    /// 失败返回人类可读文本（进入 `degraded_sources`，不中断其它源）。
    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String>;
}
