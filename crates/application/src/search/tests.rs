//! 全局检索测试（V11 §119-§122）：空注册表、单源降级、每源裁剪、总数上限、
//! 排序、空白查询、snippet 边界、来源过滤。
//!
//! 全部用内存 Fake 端口（`GlobalSearchPort`）驱动 `GlobalSearchService`：
//! 不触任何真实模块服务、SQLite、LLM 或网络 —— 服务本身也不调用 LLM，
//! 因此这些测试天然验证了「模型不可用时功能仍可用」。

use std::sync::Arc;

use devtoolbox_core::search::{
    GlobalSearchHit, GlobalSearchQuery, MAX_SNIPPET_CHARS, MAX_TOTAL_HITS,
    SearchSource,
};

use super::ports::GlobalSearchPort;
use super::service::GlobalSearchService;

// ---------- Fakes ----------

/// 内存端口：返回固定命中，可注入失败（验证单源失败不影响其它源）。
struct FakePort {
    source: SearchSource,
    hits: Vec<GlobalSearchHit>,
    failure: Option<String>,
}

impl FakePort {
    fn new(source: SearchSource) -> Self {
        Self {
            source,
            hits: Vec::new(),
            failure: None,
        }
    }

    fn with<F>(mut self, id: &str, score: f32, mutate: F) -> Self
    where
        F: FnOnce(&mut GlobalSearchHit),
    {
        let mut hit = GlobalSearchHit::new(
            self.source,
            "entity",
            format!("{}/标题", self.source.as_str()),
            format!("{id}/片段"),
            serde_json::json!({"module": self.source.as_str(), "entity": {"kind": "entity", "id": id}}),
            score,
        );
        mutate(&mut hit);
        self.hits.push(hit);
        self
    }

    fn failing(source: SearchSource, message: &str) -> Self {
        Self {
            source,
            hits: Vec::new(),
            failure: Some(message.to_string()),
        }
    }
}

impl GlobalSearchPort for FakePort {
    fn source(&self) -> SearchSource {
        self.source
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        if let Some(message) = &self.failure {
            return Err(message.clone());
        }
        assert_eq!(query.trimmed(), "建筑");
        Ok(self.hits.clone())
    }
}

fn history_port() -> Arc<FakePort> {
    Arc::new(FakePort::new(SearchSource::History).with("h1", 0.9, |hit| {
        hit.action_target = serde_json::json!({
            "module": "history",
            "entity": {"kind": "event", "id": "h1"}
        });
    }))
}

fn memory_port() -> Arc<FakePort> {
    Arc::new(
        FakePort::new(SearchSource::Memory)
            .with("m1", 0.4, |hit| {
                hit.kind = "memory".to_string();
            })
            .with("m2", 0.6, |hit| {
                hit.kind = "memory".to_string();
            }),
    )
}

fn files_port() -> Arc<FakePort> {
    Arc::new(FakePort::new(SearchSource::Files).with("f1", 0.7, |hit| {
        hit.kind = "file".to_string();
    }))
}

fn query() -> GlobalSearchQuery {
    GlobalSearchQuery::new("  建筑  ")
}

fn service(ports: Vec<Arc<dyn GlobalSearchPort>>) -> GlobalSearchService {
    GlobalSearchService::new(ports)
}

// ---------- 空注册表 ----------

#[test]
fn empty_registry_returns_empty_result() {
    let result = service(Vec::new()).search(&query());
    // 空注册表：查询词照常回显，没有任何命中，也不降级任何来源。
    assert_eq!(result.query, "建筑");
    assert!(result.hits.is_empty());
    assert!(result.degraded_sources.is_empty());
    assert_eq!(result.total, 0);
    // 关键是「空结果」而不是错误：调用面可以直接渲染空态。
}

// ---------- 单源降级 ----------

#[test]
fn one_failing_source_degrades_while_others_succeed() {
    let result = service(vec![
        Arc::new(FakePort::failing(
            SearchSource::History,
            "history index unavailable",
        )) as Arc<dyn GlobalSearchPort>,
        memory_port() as Arc<dyn GlobalSearchPort>,
        files_port() as Arc<dyn GlobalSearchPort>,
    ])
    .search(&query());

    assert_eq!(result.degraded_sources, vec![SearchSource::History]);
    assert_eq!(result.total, 3);
    let sources: Vec<SearchSource> = result.hits.iter().map(|hit| hit.source).collect();
    assert_eq!(
        sources,
        vec![SearchSource::Files, SearchSource::Memory, SearchSource::Memory]
    );
}

#[test]
fn all_sources_succeed_has_no_degradation() {
    let result = service(vec![history_port(), memory_port(), files_port()]).search(&query());
    assert!(result.degraded_sources.is_empty());
    assert_eq!(result.total, 4);
    assert_eq!(result.query, "建筑");
}

// ---------- 每源裁剪 ----------

#[test]
fn per_source_limit_is_enforced() {
    let port = Arc::new(
        FakePort::new(SearchSource::Documents)
            .with("d1", 0.1, |_| {})
            .with("d2", 0.9, |_| {})
            .with("d3", 0.5, |_| {}),
    );
    let result = service(vec![port]).search(&GlobalSearchQuery {
        limit_per_source: 2,
        ..query()
    });
    assert_eq!(result.total, 2);
    // 裁剪的是低分项，不是靠后的项。
    let ids: Vec<&str> = result
        .hits
        .iter()
        .map(|hit| hit.action_target["entity"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["d2", "d3"]);
}

#[test]
fn per_source_limit_zero_falls_back_to_default() {
    let port = Arc::new(
        FakePort::new(SearchSource::Documents)
            .with("d1", 0.1, |_| {})
            .with("d2", 0.2, |_| {})
            .with("d3", 0.3, |_| {})
            .with("d4", 0.4, |_| {})
            .with("d5", 0.5, |_| {})
            .with("d6", 0.6, |_| {}),
    );
    let result = service(vec![port]).search(&GlobalSearchQuery {
        limit_per_source: 0,
        ..query()
    });
    assert_eq!(result.total, devtoolbox_core::search::DEFAULT_LIMIT_PER_SOURCE);
}

// ---------- 总数上限 ----------

#[test]
fn total_hits_are_bounded() {
    // 每源 20 条 × 3 源 = 60 > MAX_TOTAL_HITS(50)。
    let mut ports: Vec<Arc<dyn GlobalSearchPort>> = Vec::new();
    for (index, source) in [
        SearchSource::History,
        SearchSource::Travel,
        SearchSource::Memory,
    ]
    .into_iter()
    .enumerate()
    {
        let mut port = FakePort::new(source);
        for position in 0..20 {
            // 分数全局唯一且与源顺序无关，保证被裁掉的是最低分段。
            let score = (index * 20 + position) as f32 / 100.0;
            port = port.with(&format!("{source:?}/{position}"), score, |_| {});
        }
        ports.push(Arc::new(port));
    }
    let result = service(ports).search(&GlobalSearchQuery {
        limit_per_source: 20,
        ..query()
    });
    assert_eq!(result.total, MAX_TOTAL_HITS);
    assert_eq!(result.hits.len(), MAX_TOTAL_HITS);
    assert!(result.hits.iter().all(|hit| hit.score >= 0.1));
}

// ---------- 排序 ----------

#[test]
fn hits_are_sorted_by_score_descending() {
    let result = service(vec![history_port(), memory_port(), files_port()]).search(&query());
    let scores: Vec<f32> = result.hits.iter().map(|hit| hit.score).collect();
    assert_eq!(scores, vec![0.9, 0.7, 0.6, 0.4]);
}

#[test]
fn equal_scores_keep_stable_order() {
    // 两个端口给出完全相同的分数：结果顺序必须稳定（按端口注册顺序），不抖动。
    let left = Arc::new(FakePort::new(SearchSource::Files).with("f1", 0.5, |_| {}));
    let right = Arc::new(FakePort::new(SearchSource::Memory).with("m1", 0.5, |_| {}));
    let result = service(vec![left, right]).search(&query());
    let sources: Vec<SearchSource> = result.hits.iter().map(|hit| hit.source).collect();
    assert_eq!(sources, vec![SearchSource::Files, SearchSource::Memory]);
    // 反向注册 => 反向顺序（证明是稳定排序而非碰运气）。
    let left = Arc::new(FakePort::new(SearchSource::Files).with("f1", 0.5, |_| {}));
    let right = Arc::new(FakePort::new(SearchSource::Memory).with("m1", 0.5, |_| {}));
    let result = service(vec![right, left]).search(&query());
    let sources: Vec<SearchSource> = result.hits.iter().map(|hit| hit.source).collect();
    assert_eq!(sources, vec![SearchSource::Memory, SearchSource::Files]);
}

// ---------- 空白查询 ----------

#[test]
fn blank_query_returns_empty_result() {
    for blank in ["", "   ", "\n\t "] {
        let result = service(vec![history_port(), memory_port()]).search(&GlobalSearchQuery::new(blank));
        assert_eq!(result.total, 0);
        assert!(result.hits.is_empty());
        assert!(result.degraded_sources.is_empty());
        assert_eq!(result.query, "");
    }
}

#[test]
fn query_is_trimmed_in_result() {
    let result = service(vec![history_port()]).search(&query());
    assert_eq!(result.query, "建筑");
}

// ---------- snippet 边界 ----------

#[test]
fn snippet_is_bounded_to_max_chars() {
    let long = "长".repeat(MAX_SNIPPET_CHARS + 100);
    let port = Arc::new(FakePort::new(SearchSource::Documents).with("d1", 1.0, move |hit| {
        hit.snippet = long.clone();
    }));
    let result = service(vec![port]).search(&query());
    let snippet = &result.hits[0].snippet;
    assert!(snippet.chars().count() <= MAX_SNIPPET_CHARS);
    assert!(snippet.ends_with('…'));
}

// ---------- 来源过滤 ----------

#[test]
fn sources_filter_selects_only_requested_source() {
    let result = service(vec![history_port(), memory_port(), files_port()]).search(
        &GlobalSearchQuery {
            sources: vec![SearchSource::Files],
            ..query()
        },
    );
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].source, SearchSource::Files);
}

// ---------- 导航目标 ----------

#[test]
fn hit_action_target_is_navigable() {
    let result = service(vec![history_port()]).search(&query());
    let target = &result.hits[0].action_target;
    assert_eq!(target["module"], "history");
    assert_eq!(target["entity"]["kind"], "event");
    assert_eq!(target["entity"]["id"], "h1");
}
