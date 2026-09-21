//! Knowledge 模块测试（V6 §94/§113）：facade 工具面 + 统一结果形状 + 观测。
//!
//! 用内存 Fake 检索器（实现 `KnowledgeSourceRetriever`）驱动
//! `KnowledgeRetrievalService`：不触真实域服务，专注断言
//! - `knowledge.search` 合并多源结果、按 source_type 分组产出 ui_hint；
//! - 每条结果都带 provenance（§66）；
//! - 空命中如实说「没有找到」（§67），不编造；
//! - `sources` 参数限定源，未知值 → 参数错误。

use std::sync::Arc;

use devtoolbox_core::knowledge::{
    KnowledgeBudget, KnowledgeQuery, KnowledgeResult, KnowledgeSourceKind, Provenance,
};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{ToolRisk, UiBlockKind};

use crate::knowledge::ports::KnowledgeSourceRetriever;
use crate::knowledge::KnowledgeRetrievalService;
use crate::personal_ai::context::ContextBudget;
use crate::personal_ai::knowledge::{knowledge_tool_names, register_knowledge};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

/// 内存检索器：按源返回固定候选（含 provenance），可注入失败。
struct FakeRetriever {
    kind: KnowledgeSourceKind,
    results: Vec<KnowledgeResult>,
}

impl FakeRetriever {
    fn new(kind: KnowledgeSourceKind) -> Self {
        Self {
            kind,
            results: Vec::new(),
        }
    }

    fn with(mut self, id: &str, title: &str, snippet: &str, path: &str) -> Self {
        self.results.push(KnowledgeResult {
            source_type: self.kind,
            source_id: id.to_string(),
            title: title.to_string(),
            snippet: snippet.to_string(),
            location: None,
            score: 0.9,
            provenance: Provenance {
                source_kind: self.kind,
                source_id: id.to_string(),
                title: title.to_string(),
                path: Some(path.to_string()),
                location: None,
                module: None,
            },
            metadata: serde_json::json!({}),
        });
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
    ) -> Result<Vec<KnowledgeResult>, crate::error::ApplicationError> {
        let mut results = self.results.clone();
        if limit > 0 {
            results.truncate(limit);
        }
        Ok(results)
    }
}

fn hub(retrievers: Vec<Arc<dyn KnowledgeSourceRetriever>>) -> (Arc<KnowledgeRetrievalService>, ToolRegistry, ModuleRegistry) {
    let service = Arc::new(KnowledgeRetrievalService::new(
        retrievers,
        KnowledgeBudget::default(),
    ));
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_knowledge(&mut modules, &mut tools, Arc::clone(&service))
        .expect("register knowledge module");
    (service, tools, modules)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// ---------------------------------------------------------------------------
// 工具面
// ---------------------------------------------------------------------------

#[test]
fn registers_descriptor_and_single_read_tool() {
    let (_service, tools, modules) = hub(Vec::new());
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "knowledge");
    assert_eq!(descriptors[0].tools.len(), 1);
    assert!(
        modules.context_provider("knowledge").is_some(),
        "facade 也必须提供 ContextProvider"
    );

    let mut names: Vec<String> = tools.specs().iter().map(|spec| spec.name.clone()).collect();
    names.sort();
    let mut expected: Vec<String> = knowledge_tool_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    for spec in tools.specs() {
        assert_eq!(spec.risk, ToolRisk::Read);
        assert_eq!(spec.module, "knowledge");
    }
}

#[test]
fn search_merges_sources_and_groups_ui_blocks() {
    let memory = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "Docker 数据目录", "/Volumes/Data/docker", "memory://mem-1");
    let document = FakeRetriever::new(KnowledgeSourceKind::Document)
        .with("doc-1", "docker.md", "volume 挂载说明", "/data/docs/docker.md");
    let (_service, tools, _modules) = hub(vec![Arc::new(memory), Arc::new(document)]);

    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({"query": "docker"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], 2, "两源结果都应出现");

    // provenance 必须保留（§66）。
    for item in result.data["items"].as_array().unwrap() {
        assert!(item["provenance"]["source_id"].is_string());
        assert!(item["provenance"]["source_kind"].is_string());
    }

    // 按来源分组的 ui_hint：memory_list + document_list。
    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("grouped blocks");
    let kinds: Vec<&str> = blocks.iter().map(|b| b["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"memory_list"), "{kinds:?}");
    assert!(kinds.contains(&"document_list"), "{kinds:?}");
    let _ = UiBlockKind::MemoryList;
    let _ = UiBlockKind::DocumentList;
}

#[test]
fn search_without_sources_limit_is_budget_capped() {
    let retriever = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "a", "snippet a", "memory://a")
        .with("mem-2", "b", "snippet b", "memory://b")
        .with("mem-3", "c", "snippet c", "memory://c");
    let service = Arc::new(KnowledgeRetrievalService::new(
        vec![Arc::new(retriever)],
        KnowledgeBudget {
            max_results: 2,
            max_chars: 10_000,
            ..KnowledgeBudget::default()
        },
    ));
    let mut tools = ToolRegistry::new();
    let mut modules = ModuleRegistry::new();
    register_knowledge(&mut modules, &mut tools, Arc::clone(&service)).expect("register");

    let outcome = service
        .search(&KnowledgeQuery {
            query: "snippet".into(),
            sources: Vec::new(),
            module: None,
            limit: None,
        })
        .expect("search");
    assert_eq!(outcome.results.len(), 2, "预算 max_results 必须硬截断");
    assert!(outcome.diagnostics.errors.is_empty(), "无错误: {:?}", outcome.diagnostics.errors);
}

#[test]
fn search_sources_argument_filters_and_validates() {
    let memory = FakeRetriever::new(KnowledgeSourceKind::Memory)
        .with("mem-1", "a", "snippet", "memory://a");
    let document = FakeRetriever::new(KnowledgeSourceKind::Document)
        .with("doc-1", "b", "snippet", "/data/b.md");
    let (_service, tools, _modules) = hub(vec![Arc::new(memory), Arc::new(document)]);

    let memory_only = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({"query": "snippet", "sources": ["memory"]}),
    }))
    .unwrap();
    assert_eq!(memory_only.data["count"], 1);
    assert_eq!(memory_only.data["items"][0]["source_type"], "memory");

    let bad = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c2".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({"query": "snippet", "sources": ["telepathy"]}),
    }));
    assert!(bad.is_err(), "未知 source kind 必须参数错误");

    let not_array = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c3".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({"query": "snippet", "sources": "memory"}),
    }));
    assert!(not_array.is_err(), "sources 必须是数组");
}

#[test]
fn search_no_hit_states_not_found_with_diagnostics() {
    let (_service, tools, _modules) = hub(vec![Arc::new(
        FakeRetriever::new(KnowledgeSourceKind::Memory),
    )]);
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({"query": "量子计算"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["note"], "没有找到相关个人资料");
    assert_eq!(result.data["items"], serde_json::json!([]));
    // 观测（diagnostics）必须随结果返回，便于排障（§65）。
    assert!(result.data["diagnostics"].is_object());
    assert!(
        result.metadata["ui_hint"].is_null() || result.metadata["ui_hint"]["ui_blocks"].is_null(),
        "空命中不得产出 ui_hint"
    );
}

#[test]
fn missing_query_is_invalid_argument() {
    let (_service, tools, _modules) = hub(Vec::new());
    let error = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "knowledge.search".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("缺 query 必须参数错误");
    assert!(error.to_string().contains("query"), "{error}");
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

#[test]
fn context_provider_reports_overview_only() {
    let (_service, _tools, modules) = hub(vec![
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Memory)),
        Arc::new(FakeRetriever::new(KnowledgeSourceKind::Document)),
    ]);
    let provider = modules.context_provider("knowledge").expect("provider");
    let bundle = provider
        .build_context(&AppContext::default(), &ContextBudget::default())
        .expect("overview");
    assert_eq!(bundle.module, "knowledge");
    let sources = bundle.summary["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 2);
    assert!(bundle.summary["budget"].is_object());
}
