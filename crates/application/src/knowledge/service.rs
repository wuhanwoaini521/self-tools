//! Knowledge 检索编排（V6 Track D，§52-§68）。
//!
//! 确定性管线（无 Agent、无 LLM 规划，§56）：
//!
//! ```text
//! query
//!   → RetrievalPlanner（规则式：按 query 命中情况决定参与源与配额）
//!   → 各源 retrieve（候选）
//!   → merge（按分数排序）
//!   → dedupe（身份去重 + 同路径跨源去重：Document 优先于 File）
//!   → budget（每源上限 + 总条数 + 总字符，硬截断）
//!   → KnowledgeRetrievalOutcome{results, diagnostics}
//! ```
//!
//! 同时实现 `RetrievalAugmenter`：为 PersonalAgent 提供**可选**的自动注入段
//! （预算受限、敏感项永不注入、无命中不注入）。

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use devtoolbox_core::knowledge::{
    KnowledgeBudget, KnowledgeDiagnostics, KnowledgeQuery, KnowledgeResult,
    KnowledgeRetrievalOutcome, KnowledgeSourceKind,
};
use devtoolbox_core::personal_ai::AppContext;

use crate::error::ApplicationError;
use crate::knowledge::context::{KnowledgeContext, OmittedSummary};
use crate::knowledge::observability::KnowledgeMetrics;
use crate::knowledge::ports::{KnowledgeSourceRetriever, RetrievalMode};
use crate::personal_ai::retrieval::RetrievalAugmenter;

/// 自动注入的最低相关性阈值（低于此值不注入，§65/§68）。
const AUGMENT_MIN_SCORE: f32 = 0.5;
/// 查询过短（「嗯」「ok」）不触发检索。
const AUGMENT_MIN_QUERY_CHARS: usize = 2;

/// 统一知识检索服务。
pub struct KnowledgeRetrievalService {
    retrievers: Vec<Arc<dyn KnowledgeSourceRetriever>>,
    budget: KnowledgeBudget,
    metrics: Arc<KnowledgeMetrics>,
}

impl KnowledgeRetrievalService {
    #[must_use]
    pub fn new(
        retrievers: Vec<Arc<dyn KnowledgeSourceRetriever>>,
        budget: KnowledgeBudget,
    ) -> Self {
        Self {
            retrievers,
            budget,
            metrics: Arc::new(KnowledgeMetrics::new()),
        }
    }

    #[must_use]
    pub fn with_metrics(
        retrievers: Vec<Arc<dyn KnowledgeSourceRetriever>>,
        budget: KnowledgeBudget,
        metrics: Arc<KnowledgeMetrics>,
    ) -> Self {
        Self {
            retrievers,
            budget,
            metrics,
        }
    }

    #[must_use]
    pub fn budget(&self) -> KnowledgeBudget {
        self.budget
    }

    #[must_use]
    pub fn metrics(&self) -> Arc<KnowledgeMetrics> {
        self.metrics.clone()
    }

    /// 已注册的知识源（顺序固定，便于测试与展示）。
    #[must_use]
    pub fn kinds(&self) -> Vec<KnowledgeSourceKind> {
        let mut kinds: Vec<KnowledgeSourceKind> = self
            .retrievers
            .iter()
            .map(|retriever| retriever.kind())
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// 显式检索（工具路径）：四类源都参与，受 `max_results`/`max_chars` 硬截断。
    pub fn search(
        &self,
        query: &KnowledgeQuery,
    ) -> Result<KnowledgeRetrievalOutcome, ApplicationError> {
        self.retrieve(
            &query.query,
            RetrievalMode::Explicit,
            &query.sources,
            query.limit,
        )
    }

    /// 统一检索入口。
    pub fn retrieve(
        &self,
        query: &str,
        mode: RetrievalMode,
        sources: &[KnowledgeSourceKind],
        limit_override: Option<usize>,
    ) -> Result<KnowledgeRetrievalOutcome, ApplicationError> {
        let started = Instant::now();
        let mut diagnostics = KnowledgeDiagnostics::default();
        let trimmed = query.trim();
        // 本次每源上限（与候选阶段同一函数，避免两处语义分叉）。
        let per_source_limit = |kind: KnowledgeSourceKind| self.limit_for(kind, mode, limit_override);
        let max_results = limit_override
            .unwrap_or(self.budget.max_results)
            .min(self.budget.max_results);
        if trimmed.is_empty() {
            diagnostics.duration_ms = started.elapsed().as_millis() as u64;
            return Ok(KnowledgeRetrievalOutcome {
                results: Vec::new(),
                diagnostics,
            });
        }

        // 1) 各源候选（单源失败只记原因，不影响其它源，§91 精神）。
        let mut merged: Vec<KnowledgeResult> = Vec::new();
        for retriever in &self.retrievers {
            let kind = retriever.kind();
            if !sources.is_empty() && !sources.contains(&kind) {
                continue;
            }
            let limit = per_source_limit(kind);
            if limit == 0 {
                continue;
            }
            match retriever.retrieve(trimmed, limit.saturating_mul(2)) {
                Ok(results) => {
                    diagnostics.record_candidates(kind, results.len());
                    merged.extend(results);
                }
                Err(error) => {
                    diagnostics.record_candidates(kind, 0);
                    diagnostics
                        .errors
                        .insert(kind.as_str().to_string(), error.to_string());
                }
            }
        }

        // 2) 排序（分数降序；同分按源类型 + id，保证确定性）。
        merged.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(kind_priority(left.source_type).cmp(&kind_priority(right.source_type)))
                .then(left.source_id.cmp(&right.source_id))
                .then(left.location.cmp(&right.location))
        });

        // 3) 同路径跨源去重：Document / Module 优先于 File（§5.3）。
        let mut path_winners: BTreeMap<String, usize> = BTreeMap::new();
        let mut deduped: Vec<KnowledgeResult> = Vec::with_capacity(merged.len());
        for result in merged {
            match result.canonical_path().map(str::to_string) {
                Some(path) => match path_winners.get(&path).copied() {
                    Some(index) => {
                        if kind_priority(result.source_type) < kind_priority(deduped[index].source_type)
                        {
                            deduped[index] = result;
                        }
                        diagnostics.duplicates_removed += 1;
                    }
                    None => {
                        path_winners.insert(path, deduped.len());
                        deduped.push(result);
                    }
                },
                None => deduped.push(result),
            }
        }
        deduped.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(kind_priority(left.source_type).cmp(&kind_priority(right.source_type)))
                .then(left.source_id.cmp(&right.source_id))
                .then(left.location.cmp(&right.location))
        });

        // 4) 身份去重 + 预算截断（每源上限 / 总条数 / 总字符）。
        let mut selected: Vec<KnowledgeResult> = Vec::new();
        let mut identities: Vec<(KnowledgeSourceKind, String, Option<String>)> = Vec::new();
        let mut per_source: BTreeMap<KnowledgeSourceKind, usize> = BTreeMap::new();
        let mut total_chars = 0usize;
        for result in deduped {
            if selected.len() >= max_results {
                diagnostics.dropped_by_budget += 1;
                continue;
            }
            let identity = (
                result.source_type,
                result.source_id.clone(),
                result.location.clone(),
            );
            if identities.contains(&identity) {
                diagnostics.duplicates_removed += 1;
                continue;
            }
            let used = per_source.get(&result.source_type).copied().unwrap_or(0);
            if used >= per_source_limit(result.source_type) {
                diagnostics.dropped_by_budget += 1;
                continue;
            }
            let chars = result.char_count();
            if total_chars + chars > self.budget.max_chars {
                diagnostics.dropped_by_budget += 1;
                diagnostics.truncated = true;
                continue;
            }
            total_chars += chars;
            *per_source.entry(result.source_type).or_insert(0) += 1;
            identities.push(identity);
            selected.push(result);
        }
        diagnostics.result_count = selected.len();
        diagnostics.total_chars = total_chars;
        for (kind, count) in &per_source {
            diagnostics
                .selected_counts
                .insert(kind.as_str().to_string(), *count);
        }
        diagnostics.duration_ms = started.elapsed().as_millis() as u64;

        let outcome = KnowledgeRetrievalOutcome {
            results: selected,
            diagnostics,
        };
        self.metrics.record_retrieval(&outcome);
        Ok(outcome)
    }

    /// 聚合为 `KnowledgeContext`（§63），并记录被省略的条数。
    pub fn build_context(
        &self,
        query: &str,
        app_context: &AppContext,
        module_context: Option<crate::personal_ai::context::ContextBundle>,
        mode: RetrievalMode,
    ) -> Result<KnowledgeContext, ApplicationError> {
        let outcome = self.retrieve(query, mode, &[], None)?;
        let mut context = KnowledgeContext::new(app_context.clone());
        context.module_context = module_context;
        let mut omitted = OmittedSummary::default();
        for (label, candidates) in &outcome.diagnostics.candidate_counts {
            let selected = outcome
                .diagnostics
                .selected_counts
                .get(label)
                .copied()
                .unwrap_or(0);
            let missing = candidates.saturating_sub(selected);
            match KnowledgeSourceKind::parse(label) {
                Some(KnowledgeSourceKind::Memory) => omitted.memories = missing,
                Some(KnowledgeSourceKind::Document) => omitted.documents = missing,
                Some(KnowledgeSourceKind::File) => omitted.files = missing,
                _ => {}
            }
        }
        for result in outcome.results {
            context.push(result);
        }
        context.omitted = omitted;
        context.diagnostics = outcome.diagnostics;
        Ok(context)
    }

    fn limit_for(
        &self,
        kind: KnowledgeSourceKind,
        mode: RetrievalMode,
        limit_override: Option<usize>,
    ) -> usize {
        let base = match mode {
            // 显式检索（工具调用）：调用方的 limit 是请求而非上限 —— 计划 §5.4
            // 只要求受 `max_results` / `max_chars` 硬截断，每源预算不再二次钳制；
            // 否则 `knowledge.search(limit=20)` 会被静默压到 `max_per_source`。
            RetrievalMode::Explicit => limit_override.unwrap_or(self.budget.max_per_source),
            RetrievalMode::Augment => self.budget.limit_for(kind),
        };
        match (mode, limit_override) {
            (RetrievalMode::Augment, Some(limit)) => base.min(limit.max(1)),
            _ => base.max(1),
        }
    }
}

#[async_trait]
impl RetrievalAugmenter for KnowledgeRetrievalService {
    /// 自动注入：无命中 / 低相关 / 检索失败 → `None`（绝不编造、绝不无条件注入，§67/§68）。
    async fn augment(&self, query: &str, app_context: &AppContext) -> Option<String> {
        if query.trim().chars().count() < AUGMENT_MIN_QUERY_CHARS {
            return None;
        }
        let outcome = self
            .retrieve(query, RetrievalMode::Augment, &[], None)
            .ok()?;
        let strong: Vec<KnowledgeResult> = outcome
            .results
            .into_iter()
            .filter(|result| result.score >= AUGMENT_MIN_SCORE)
            .collect();
        if strong.is_empty() {
            return None;
        }
        let mut context = KnowledgeContext::new(app_context.clone());
        for result in strong {
            context.push(result);
        }
        self.metrics.record_context(context.total_items());
        let rendered = context.render_for_prompt(self.budget.max_chars);
        if rendered.is_empty() { None } else { Some(rendered) }
    }
}

/// 跨源去重优先级（小 = 优先保留）。
fn kind_priority(kind: KnowledgeSourceKind) -> u8 {
    match kind {
        KnowledgeSourceKind::Memory => 0,
        KnowledgeSourceKind::Document => 1,
        KnowledgeSourceKind::Module => 2,
        KnowledgeSourceKind::File => 3,
    }
}
