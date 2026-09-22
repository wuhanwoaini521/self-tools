//! 全局检索服务（V11 §119-§122）：聚合各模块命中，不经过任何 LLM。
//!
//! ```text
//! query
//!   → trim（空白查询直接返回空结果）
//!   → 每个端口 search（单源失败 → degraded_sources，其余源继续）
//!   → 端口返回的命中再收紧：来源纠正 + snippet 截断（双重保险）
//!   → 每源裁剪 limit_per_source
//!   → 全局按 score 倒序排序
//!   → 总数硬截断 MAX_TOTAL_HITS
//! ```

use std::sync::Arc;

use devtoolbox_core::search::{
    DEFAULT_LIMIT_PER_SOURCE, GlobalSearchHit, GlobalSearchQuery, GlobalSearchResult,
    MAX_TOTAL_HITS, bound_snippet,
};

use crate::search::ports::GlobalSearchPort;

/// 统一全局检索服务。
pub struct GlobalSearchService {
    ports: Vec<Arc<dyn GlobalSearchPort>>,
}

impl GlobalSearchService {
    #[must_use]
    pub fn new(ports: Vec<Arc<dyn GlobalSearchPort>>) -> Self {
        Self { ports }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ports.is_empty()
    }

    /// 已注册的来源数（观测 / 诊断用）。
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.ports.len()
    }

    /// 执行全局检索。
    ///
    /// - 空白查询返回空结果（`hits` 与 `degraded_sources` 均为空）；
    /// - 单源失败只降级该源，其它源的结果照常返回；
    /// - 命中按分数倒序（同分保持来源注册顺序 => 稳定排序）。
    #[must_use]
    pub fn search(&self, query: &GlobalSearchQuery) -> GlobalSearchResult {
        let trimmed = query.trimmed();
        if trimmed.is_empty() {
            // 空白查询：空结果而非错误（前端渲染空态）。
            return GlobalSearchResult::default();
        }

        let limit_per_source = if query.limit_per_source == 0 {
            DEFAULT_LIMIT_PER_SOURCE
        } else {
            query.limit_per_source
        };

        let mut hits: Vec<GlobalSearchHit> = Vec::new();
        let mut degraded_sources = Vec::new();

        for port in &self.ports {
            if !query.includes(port.source()) {
                continue;
            }
            match port.search(query) {
                Ok(mut source_hits) => {
                    let source = port.source();
                    // 收紧端口返回的载荷：来源归属以端口为准（防止上游混入别的来源），
                    // snippet 兜底截断（端口可用 `GlobalSearchHit::new` 已收敛，此处双保险）。
                    for hit in &mut source_hits {
                        hit.source = source;
                        hit.snippet = bound_snippet(&hit.snippet);
                    }
                    source_hits.sort_by(|left, right| {
                        right
                            .score
                            .partial_cmp(&left.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    source_hits.truncate(limit_per_source);
                    hits.extend(source_hits);
                }
                Err(_message) => {
                    // 单源降级：只记录来源，不影响其它源的结果。
                    degraded_sources.push(port.source());
                }
            }
        }

        // 稳定排序：同分时保持「按来源、按端口提交顺序」的原有次序。
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(MAX_TOTAL_HITS);

        GlobalSearchResult {
            query: trimmed.to_string(),
            total: hits.len(),
            hits,
            degraded_sources,
        }
    }
}
