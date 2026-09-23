//! Knowledge 检索测试（V6 §98）—— 合并 / 排序 / 去重 / 预算 / 降级 / 观测。
//!
//! 全部用内存 Fake 检索器（`KnowledgeSourceRetriever`）驱动
//! `KnowledgeRetrievalService`：不触真实域服务与 SQLite。

use std::sync::Arc;

use devtoolbox_core::knowledge::{
    KnowledgeBudget, KnowledgeQuery, KnowledgeResult, KnowledgeSourceKind, Provenance,
};
use devtoolbox_core::personal_ai::AppContext;

use crate::error::ApplicationError;
use crate::knowledge::KnowledgeRetrievalService;
use crate::knowledge::context::KnowledgeContext;
use crate::knowledge::ports::{KnowledgeSourceRetriever, RetrievalMode};
use crate::personal_ai::retrieval::RetrievalAugmenter;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 内存检索器：返回固定候选，可注入失败（验证单源失败不影响其它源）。
struct FakeRetriever {
    kind: KnowledgeSourceKind,
    results: Vec<KnowledgeResult>,
    failure: Option<String>,
}

impl FakeRetriever {
    fn new(kind: KnowledgeSourceKind) -> Self {
        Self {
            kind,
            results: Vec::new(),
            failure: None,
        }
    }

    fn with(
        mut self,
        id: &str,
        title: &str,
        snippet: &str,
        path: Option<&str>,
        score: f32,
    ) -> Self {
        self.results.push(KnowledgeResult {
            source_type: self.kind,
            source_id: id.to_string(),
            title: title.to_string(),
            snippet: snippet.to_string(),
            location: None,
            score,
            provenance: Provenance {
                source_kind: self.kind,
                source_id: id.to_string(),
                title: title.to_string(),
                path: path.map(str::to_string),
                location: None,
                module: None,
            },
            metadata: serde_json::json!({}),
        });
        self
    }

    fn failing(mut self, reason: &str) -> Self {
        self.failure = Some(reason.to_string());
        self
    }
}

impl KnowledgeSourceRetriever for FakeRetriever {
    fn kind(&self) -> KnowledgeSourceKind {
        self.kind
    }

    fn retrieve(
        &self,
        _query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeResult>, ApplicationError> {
        if let Some(reason) = &self.failure {
            return Err(ApplicationError::Knowledge {
                message: reason.clone(),
            });
        }
        let mut results = self.results.clone();
        if limit > 0 {
            results.truncate(limit);
        }
        Ok(results)
    }
}

fn service(retrievers: Vec<Arc<dyn KnowledgeSourceRetriever>>) -> KnowledgeRetrievalService {
    KnowledgeRetrievalService::new(retrievers, all_sources_budget())
}

/// 全部源都参与的预算（默认 `max_files = 0`：文件定位默认走工具，§22）。
fn all_sources_budget() -> KnowledgeBudget {
    KnowledgeBudget {
        max_files: 4,
        ..KnowledgeBudget::default()
    }
}

fn query(text: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        query: text.to_string(),
        sources: Vec::new(),
        module: None,
        limit: None,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// ---------------------------------------------------------------------------
// 合并与排序
// ---------------------------------------------------------------------------

#[test]
fn merges_all_sources_sorted_by_score() {
    let service = service(vec![
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Memory).with(
            "mem-1",
            "Docker 数据目录",
            "/Volumes/Data/docker",
            None,
            0.9,
        )),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
            "doc-1",
            "docker.md",
            "volume 说明",
            Some("/data/docs/docker.md"),
            0.6,
        )),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::File).with(
            "file-1",
            "docker-compose.yml",
            "",
            Some("/data/docker-compose.yml"),
            0.4,
        )),
    ]);

    let outcome = service.search(&query("docker")).expect("search");
    assert_eq!(outcome.results.len(), 3);
    assert_eq!(outcome.results[0].source_type, KnowledgeSourceKind::Memory);
    assert_eq!(outcome.results[2].source_type, KnowledgeSourceKind::File);
    // 每条结果都带 provenance（§66）。
    for result in &outcome.results {
        assert_eq!(result.provenance.source_id, result.source_id);
        assert_eq!(result.provenance.source_kind, result.source_type);
    }
    assert_eq!(outcome.diagnostics.sources_queried.len(), 3);
    assert_eq!(outcome.diagnostics.result_count, 3);
    assert!(outcome.diagnostics.errors.is_empty());
}

#[test]
fn same_score_is_deterministic() {
    let build = || {
        service(vec![
            Arc::new(FakeRetriever::new(KnowledgeSourceKind::File).with(
                "file-b",
                "b.md",
                "s",
                Some("/data/b.md"),
                0.8,
            )),
            Arc::new(
                FakeRetriever::new(KnowledgeSourceKind::Memory).with("mem-a", "a", "s", None, 0.8),
            ),
        ])
    };
    let first = build().search(&query("s")).expect("first").results;
    let second = build().search(&query("s")).expect("second").results;
    let ids: Vec<&str> = first.iter().map(|r| r.source_id.as_str()).collect();
    let ids_again: Vec<&str> = second.iter().map(|r| r.source_id.as_str()).collect();
    assert_eq!(ids, ids_again, "同分必须确定性排序");
}

#[test]
fn empty_query_returns_empty_without_querying() {
    let service = service(vec![Arc::new(
        FakeRetriever::new(KnowledgeSourceKind::Memory).with("mem-1", "a", "s", None, 1.0),
    )]);
    let outcome = service.search(&query("   ")).expect("search");
    assert!(outcome.results.is_empty());
    assert!(
        outcome.diagnostics.sources_queried.is_empty(),
        "空查询不应触发任何源"
    );
}

// ---------------------------------------------------------------------------
// 去重（§5.3：Document 优先于同路径 File）
// ---------------------------------------------------------------------------

#[test]
fn same_path_document_wins_over_file() {
    let service = service(vec![
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::File).with(
            "file-1",
            "docker.md",
            "文件元数据命中",
            Some("/data/docs/docker.md"),
            0.9,
        )),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
            "doc-1",
            "docker.md",
            "文档正文命中",
            Some("/data/docs/docker.md"),
            0.6,
        )),
    ]);

    let outcome = service.search(&query("docker")).expect("search");
    assert_eq!(outcome.results.len(), 1, "同路径必须去重");
    assert_eq!(
        outcome.results[0].source_type,
        KnowledgeSourceKind::Document,
        "Document 优先于 File"
    );
    assert_eq!(outcome.diagnostics.duplicates_removed, 1);
}

#[test]
fn same_identity_deduplicates() {
    let service = service(vec![Arc::new(
        FakeRetriever::new(KnowledgeSourceKind::Memory)
            .with("mem-1", "a", "s", None, 0.9)
            .with("mem-1", "a", "s", None, 0.9),
    )]);
    let outcome = service.search(&query("s")).expect("search");
    assert_eq!(outcome.results.len(), 1);
    assert!(outcome.diagnostics.duplicates_removed >= 1);
}

// ---------------------------------------------------------------------------
// 预算
// ---------------------------------------------------------------------------

#[test]
fn budget_caps_results_per_source_and_chars() {
    let retriever = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "a", &"x".repeat(50), None, 0.9)
        .with("mem-2", "b", &"y".repeat(50), None, 0.8)
        .with("mem-3", "c", &"z".repeat(50), None, 0.7);
    let service = KnowledgeRetrievalService::new(
        vec![Arc::new(retriever)],
        KnowledgeBudget {
            max_results: 2,
            max_chars: 10_000,
            max_per_source: 5,
            max_memories: 5,
            max_document_chunks: 3,
            max_files: 0,
        },
    );
    let outcome = service.search(&query("s")).expect("search");
    assert_eq!(outcome.results.len(), 2, "max_results 硬截断");
    assert!(outcome.diagnostics.dropped_by_budget >= 1);
}

#[test]
fn per_source_budget_prevents_monopoly() {
    let service = KnowledgeRetrievalService::new(
        vec![
            Arc::new(
                FakeRetriever::new(KnowledgeSourceKind::Memory)
                    .with("mem-1", "a", "s", None, 0.9)
                    .with("mem-2", "b", "s", None, 0.8),
            ),
            Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
                "doc-1",
                "c",
                "s",
                Some("/data/c.md"),
                0.7,
            )),
        ],
        KnowledgeBudget {
            max_results: 10,
            max_chars: 10_000,
            max_per_source: 1,
            max_memories: 1,
            max_document_chunks: 1,
            max_files: 0,
        },
    );
    let outcome = service.search(&query("s")).expect("search");
    assert_eq!(outcome.results.len(), 2, "每源一条，两源都保留");
    assert!(
        outcome.diagnostics.dropped_by_budget >= 1,
        "第二条 memory 应被预算丢弃"
    );
}

#[test]
fn explicit_limit_overrides_per_source_budget() {
    // 计划 §5.4：显式检索的 limit 是请求，只受 max_results / max_chars 约束。
    let retriever = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "a", "s", None, 0.9)
        .with("mem-2", "b", "s", None, 0.8)
        .with("mem-3", "c", "s", None, 0.7);
    let service = KnowledgeRetrievalService::new(
        vec![Arc::new(retriever)],
        KnowledgeBudget {
            max_results: 10,
            max_chars: 10_000,
            max_per_source: 1,
            max_memories: 1,
            max_document_chunks: 1,
            max_files: 0,
        },
    );
    let outcome = service
        .search(&KnowledgeQuery {
            query: "s".into(),
            sources: Vec::new(),
            module: None,
            limit: Some(3),
        })
        .expect("search");
    assert_eq!(outcome.results.len(), 3, "显式 limit 必须能超过每源预算");
    assert_eq!(outcome.diagnostics.dropped_by_budget, 0);
}

#[test]
fn char_budget_truncates_and_marks() {
    let retriever = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "a", &"x".repeat(100), None, 0.9)
        .with("mem-2", "b", &"y".repeat(100), None, 0.8);
    let service = KnowledgeRetrievalService::new(
        vec![Arc::new(retriever)],
        KnowledgeBudget {
            max_results: 10,
            max_chars: 150,
            max_per_source: 10,
            max_memories: 10,
            max_document_chunks: 3,
            max_files: 0,
        },
    );
    let outcome = service.search(&query("s")).expect("search");
    assert_eq!(outcome.results.len(), 1, "第二条超出字符预算");
    assert!(outcome.diagnostics.truncated);
    assert_eq!(outcome.diagnostics.total_chars, 101);
}

#[test]
fn sources_filter_limits_retrievers() {
    let service = service(vec![
        Arc::new(
            FakeRetriever::new(KnowledgeSourceKind::Memory).with("mem-1", "a", "s", None, 0.9),
        ),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
            "doc-1",
            "b",
            "s",
            Some("/data/b.md"),
            0.8,
        )),
    ]);
    let outcome = service
        .search(&KnowledgeQuery {
            query: "s".into(),
            sources: vec![KnowledgeSourceKind::Memory],
            module: None,
            limit: None,
        })
        .expect("search");
    assert_eq!(outcome.results.len(), 1);
    assert_eq!(outcome.results[0].source_type, KnowledgeSourceKind::Memory);
    assert_eq!(
        outcome.diagnostics.sources_queried,
        vec![KnowledgeSourceKind::Memory]
    );
}

// ---------------------------------------------------------------------------
// 降级（§91）
// ---------------------------------------------------------------------------

#[test]
fn one_source_failure_does_not_block_others() {
    let service = service(vec![
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Memory).failing("索引库被占用")),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
            "doc-1",
            "b",
            "s",
            Some("/data/b.md"),
            0.8,
        )),
    ]);
    let outcome = service.search(&query("s")).expect("search");
    assert_eq!(outcome.results.len(), 1, "其它源结果必须保留");
    assert_eq!(outcome.diagnostics.errors.len(), 1);
    assert!(outcome.diagnostics.errors.contains_key("memory"));
}

// ---------------------------------------------------------------------------
// 自动注入（§22/§68）
// ---------------------------------------------------------------------------

#[test]
fn augment_skips_short_queries_and_weak_hits() {
    let service = service(vec![Arc::new(
        FakeRetriever::new(KnowledgeSourceKind::Memory).with("mem-1", "a", "s", None, 0.1),
    )]);
    let context = AppContext::default();
    // 查询太短。
    assert!(block_on(service.augment("a", &context)).is_none());
    // 相关度低于阈值（0.5）。
    assert!(block_on(service.augment("足够长的查询", &context)).is_none());
}

#[test]
fn augment_renders_prompt_block_for_strong_hits() {
    let service = service(vec![Arc::new(
        FakeRetriever::new(KnowledgeSourceKind::Memory).with(
            "mem-1",
            "Docker 数据目录",
            "/Volumes/Data/docker",
            None,
            0.95,
        ),
    )]);
    let rendered = block_on(service.augment("docker 数据目录在哪", &AppContext::default()))
        .expect("强命中必须注入");
    assert!(rendered.contains("[个人知识检索]"), "{rendered}");
    assert!(rendered.contains("/Volumes/Data/docker"));
    // 观测：注入也计数（retrievals + 选中的 context 条目）。
    let snapshot = service.metrics().snapshot();
    assert_eq!(snapshot.retrievals, 1);
    assert_eq!(snapshot.context_items_selected, 1);
}

// ---------------------------------------------------------------------------
// 上下文聚合（§63）
// ---------------------------------------------------------------------------

#[test]
fn build_context_groups_and_records_omitted() {
    let service = KnowledgeRetrievalService::new(
        vec![
            Arc::new(
                FakeRetriever::new(KnowledgeSourceKind::Memory)
                    .with("mem-1", "a", "s", None, 0.9)
                    .with("mem-2", "b", "s", None, 0.8),
            ),
            Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document).with(
                "doc-1",
                "c",
                "s",
                Some("/data/c.md"),
                0.7,
            )),
        ],
        KnowledgeBudget {
            max_results: 10,
            max_chars: 10_000,
            max_per_source: 1,
            max_memories: 1,
            max_document_chunks: 1,
            max_files: 0,
        },
    );
    let context: KnowledgeContext = service
        .build_context("s", &AppContext::default(), None, RetrievalMode::Explicit)
        .expect("context");
    assert_eq!(context.memories.len(), 1);
    assert_eq!(context.document_refs.len(), 1);
    assert_eq!(context.omitted.memories, 1, "被预算省略的条数必须记录");
    let rendered = context.render_for_prompt(1_000);
    assert!(rendered.contains("[个人知识检索]"));
}
