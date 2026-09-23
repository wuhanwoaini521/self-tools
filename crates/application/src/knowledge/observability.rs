//! Knowledge 观测（V6 §93/§94）。
//!
//! **隐私约束**：只累计计数与耗时，绝不记录 Memory 正文 / 文档正文 / 文件内容 /
//! 用户 prompt。快照可直接暴露给前端展示与排障。
#![allow(
    clippy::field_reassign_with_default,
    clippy::unnecessary_sort_by,
    clippy::drop_non_drop,
    clippy::uninlined_format_args
)]

use std::collections::BTreeMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use devtoolbox_core::knowledge::{KnowledgeRetrievalOutcome, KnowledgeSourceKind};

use crate::documents::service::IndexReport;
use crate::files::service::FileIndexReport;

/// 观测快照（序列化给前端 / 日志；无正文）。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeMetricsSnapshot {
    pub retrievals: u64,
    pub memory_hits: u64,
    pub document_hits: u64,
    pub file_hits: u64,
    pub results_selected: u64,
    pub dropped_by_budget: u64,
    pub omitted_sensitive: u64,
    pub retrieval_duration_ms: u64,
    pub index_runs: u64,
    pub indexed_documents: u64,
    pub indexed_files: u64,
    pub index_failures: u64,
    pub index_duration_ms: u64,
    pub tool_calls: BTreeMap<String, u64>,
    pub tool_duration_ms: u64,
    pub context_items_selected: u64,
}

#[derive(Default)]
struct Counters {
    snapshot: KnowledgeMetricsSnapshot,
}

/// 进程内观测计数器（加锁时间极短，无 await 跨锁）。
pub struct KnowledgeMetrics {
    counters: Mutex<Counters>,
}

impl Default for KnowledgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeMetrics {
    #[must_use]
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(Counters::default()),
        }
    }

    /// 记录一次检索（命中数 / 预算丢弃 / 敏感过滤 / 耗时）。
    pub fn record_retrieval(&self, outcome: &KnowledgeRetrievalOutcome) {
        let mut counters = self.counters.lock();
        counters.snapshot.retrievals += 1;
        counters.snapshot.results_selected += outcome.diagnostics.result_count as u64;
        counters.snapshot.dropped_by_budget += outcome.diagnostics.dropped_by_budget as u64;
        counters.snapshot.omitted_sensitive += outcome.diagnostics.omitted_sensitive as u64;
        counters.snapshot.retrieval_duration_ms += outcome.diagnostics.duration_ms;
        for (label, count) in &outcome.diagnostics.selected_counts {
            let slot = match KnowledgeSourceKind::parse(label) {
                Some(KnowledgeSourceKind::Memory) => &mut counters.snapshot.memory_hits,
                Some(KnowledgeSourceKind::Document) => &mut counters.snapshot.document_hits,
                Some(KnowledgeSourceKind::File) => &mut counters.snapshot.file_hits,
                _ => continue,
            };
            *slot += *count as u64;
        }
    }

    /// 记录一次自动注入（选中条数；§93 context items selected）。
    pub fn record_context(&self, items: usize) {
        let mut counters = self.counters.lock();
        counters.snapshot.context_items_selected += items as u64;
    }

    /// 记录一次索引（文档 / 文件；只计数与耗时）。
    pub fn record_index(
        &self,
        documents: &[IndexReport],
        files: &[FileIndexReport],
        duration_ms: u64,
    ) {
        let mut counters = self.counters.lock();
        counters.snapshot.index_runs += 1;
        counters.snapshot.index_duration_ms += duration_ms;
        for report in documents {
            counters.snapshot.indexed_documents += report.total_changed() as u64;
            counters.snapshot.index_failures += report.failed as u64;
        }
        for report in files {
            counters.snapshot.indexed_files += report.total_changed() as u64;
        }
    }

    /// 记录一次工具调用（名称 + 耗时；不含参数与结果）。
    pub fn record_tool(&self, name: &str, duration_ms: u64) {
        let mut counters = self.counters.lock();
        *counters
            .snapshot
            .tool_calls
            .entry(name.to_string())
            .or_insert(0) += 1;
        counters.snapshot.tool_duration_ms += duration_ms;
    }

    #[must_use]
    pub fn snapshot(&self) -> KnowledgeMetricsSnapshot {
        self.counters.lock().snapshot.clone()
    }

    pub fn reset(&self) {
        let mut counters = self.counters.lock();
        counters.snapshot = KnowledgeMetricsSnapshot::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::knowledge::{KnowledgeDiagnostics, KnowledgeResult};

    #[test]
    fn metrics_accumulate_without_content() {
        let metrics = KnowledgeMetrics::new();
        let mut diagnostics = KnowledgeDiagnostics::default();
        diagnostics.result_count = 2;
        diagnostics.dropped_by_budget = 1;
        diagnostics.omitted_sensitive = 3;
        diagnostics.duration_ms = 5;
        diagnostics.selected_counts.insert("memory".into(), 2);
        let outcome = KnowledgeRetrievalOutcome {
            results: vec![KnowledgeResult::new(
                KnowledgeSourceKind::Memory,
                "mem-1",
                "偏好",
                "喜欢历史旅行",
                0.9,
            )],
            diagnostics,
        };
        metrics.record_retrieval(&outcome);
        metrics.record_tool("memory.search", 7);
        metrics.record_context(2);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.retrievals, 1);
        assert_eq!(snapshot.memory_hits, 2);
        assert_eq!(snapshot.dropped_by_budget, 1);
        assert_eq!(snapshot.omitted_sensitive, 3);
        assert_eq!(snapshot.tool_calls["memory.search"], 1);
        assert_eq!(snapshot.tool_duration_ms, 7);
        assert_eq!(snapshot.context_items_selected, 2);

        // 快照不得包含任何正文。
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("历史旅行"));
        assert!(!json.contains("snippet"));

        metrics.reset();
        assert_eq!(metrics.snapshot(), KnowledgeMetricsSnapshot::default());
    }
}
