//! Personal Knowledge 统一检索契约（V6 Track D，§52-§69）。
//!
//! `KnowledgeResult` 是 Memory / Documents / Files / Module Knowledge 的**统一结果形状**；
//! 四类知识源在物理上保持独立（不同表、不同端口），只在这里统一表达。
//!
//! 任何结果都必须携带 `Provenance`（§66）：AI 回答涉及个人资料时必须能回溯来源。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 知识源类型（V6 §53）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    Memory,
    Document,
    File,
    /// 业务模块知识（History / Travel / Geography / Language）。
    Module,
}

impl KnowledgeSourceKind {
    /// 已注册检索器的源（工具 schema 的 `sources` 枚举；`Module` 是预留的
    /// 业务模块知识位，P1 注册 `ModuleRetriever` 后才加入，见 ADR-005）。
    pub const ALL: [KnowledgeSourceKind; 3] = [
        KnowledgeSourceKind::Memory,
        KnowledgeSourceKind::Document,
        KnowledgeSourceKind::File,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeSourceKind::Memory => "memory",
            KnowledgeSourceKind::Document => "document",
            KnowledgeSourceKind::File => "file",
            KnowledgeSourceKind::Module => "module",
        }
    }

    /// 中文标签（UI / prompt 展示）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            KnowledgeSourceKind::Memory => "记忆",
            KnowledgeSourceKind::Document => "文档",
            KnowledgeSourceKind::File => "文件",
            KnowledgeSourceKind::Module => "模块",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 检索结果的来源信息（V6 §66）。内部必须保留，不因展示层省略而丢失。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source_kind: KnowledgeSourceKind,
    /// 稳定 id（memory id / document id / file id）。
    pub source_id: String,
    pub title: String,
    /// 位置（chunk 序号 / 页码 / 章节 / 相对路径）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// 文件系统路径（Documents / Files 才有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 归属模块（Module knowledge 才有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// 统一检索结果（V6 §54）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeResult {
    pub source_type: KnowledgeSourceKind,
    pub source_id: String,
    pub title: String,
    /// 片段（有硬截断；绝不返回整份文档）。
    pub snippet: String,
    /// 相关性分数（0.0–1.0，规则式打分，可比较但非概率）。
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    pub provenance: Provenance,
}

impl KnowledgeResult {
    #[must_use]
    pub fn new(
        source_type: KnowledgeSourceKind,
        source_id: impl Into<String>,
        title: impl Into<String>,
        snippet: impl Into<String>,
        score: f32,
    ) -> Self {
        let source_id = source_id.into();
        let title = title.into();
        Self {
            source_type,
            provenance: Provenance {
                source_kind: source_type,
                source_id: source_id.clone(),
                title: title.clone(),
                location: None,
                path: None,
                module: None,
            },
            source_id,
            title,
            snippet: snippet.into(),
            score,
            location: None,
            metadata: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        let location = location.into();
        self.provenance.location = Some(location.clone());
        self.location = Some(location);
        self
    }

    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.provenance.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// 结果身份：`(source_type, source_id, location)`（去重用，V6 §55）。
    #[must_use]
    pub fn identity(&self) -> (KnowledgeSourceKind, &str, Option<&str>) {
        (
            self.source_type,
            self.source_id.as_str(),
            self.location.as_deref(),
        )
    }

    /// 用于跨源去重的物理路径（Documents / Files 共有）。
    #[must_use]
    pub fn canonical_path(&self) -> Option<&str> {
        self.provenance.path.as_deref()
    }

    /// 进入 prompt 的字符数（预算核算）。
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.title.chars().count() + self.snippet.chars().count()
    }
}

/// 检索请求（§58：query + sources + limit）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct KnowledgeQuery {
    pub query: String,
    /// 限定知识源（空 = 全部）。
    #[serde(default)]
    pub sources: Vec<KnowledgeSourceKind>,
    /// 当前模块（用于业务模块优先级，§60）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// 本次上限（缺省取预算 `max_results`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl KnowledgeQuery {
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    /// 是否检索该源（空 sources = 全选）。
    #[must_use]
    pub fn includes(&self, kind: KnowledgeSourceKind) -> bool {
        self.sources.is_empty() || self.sources.contains(&kind)
    }
}

/// 上下文预算（V6 §64/§65）：硬限制，绝不按 query 之外的条件无界注入。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeBudget {
    /// 单次检索返回的结果上限。
    pub max_results: usize,
    /// 进入 prompt 的片段总字符上限。
    pub max_chars: usize,
    /// 单源结果上限（防止某源垄断）。
    pub max_per_source: usize,
    /// 自动注入（非工具调用）时 memory 上限（§23：5–10 条）。
    pub max_memories: usize,
    /// 自动注入时 document chunk 上限。
    pub max_document_chunks: usize,
    /// 自动注入时 file 上限（文件定位默认走工具，§22）。
    pub max_files: usize,
}

impl Default for KnowledgeBudget {
    fn default() -> Self {
        Self {
            max_results: 8,
            max_chars: 4_000,
            max_per_source: 4,
            max_memories: 5,
            max_document_chunks: 3,
            max_files: 0,
        }
    }
}

impl KnowledgeBudget {
    /// 该源的结果上限。
    #[must_use]
    pub fn limit_for(&self, kind: KnowledgeSourceKind) -> usize {
        match kind {
            KnowledgeSourceKind::Memory => self.max_memories.min(self.max_per_source),
            KnowledgeSourceKind::Document => self.max_document_chunks.min(self.max_per_source),
            KnowledgeSourceKind::File => self.max_files.min(self.max_per_source),
            KnowledgeSourceKind::Module => self.max_per_source,
        }
    }
}

/// 检索观测（§93/§94）：只计数与耗时，**不含任何正文**，可安全展示与记录。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeDiagnostics {
    pub sources_queried: Vec<KnowledgeSourceKind>,
    /// 每源候选数（去重前）。
    pub candidate_counts: BTreeMap<String, usize>,
    /// 最终结果数。
    pub result_count: usize,
    /// 每源最终结果数。
    pub selected_counts: BTreeMap<String, usize>,
    /// 因预算被丢弃的结果数。
    pub dropped_by_budget: usize,
    /// 因身份/路径重复被合并的结果数。
    pub duplicates_removed: usize,
    /// 因敏感度被过滤的条数（不含正文）。
    pub omitted_sensitive: usize,
    /// 单源失败原因（§91 精神：单源失败不拖垮整体；只记原因不记内容）。
    #[serde(default)]
    pub errors: BTreeMap<String, String>,
    pub total_chars: usize,
    pub duration_ms: u64,
    pub truncated: bool,
}

impl KnowledgeDiagnostics {
    pub fn record_candidates(&mut self, kind: KnowledgeSourceKind, count: usize) {
        if !self.sources_queried.contains(&kind) {
            self.sources_queried.push(kind);
        }
        self.candidate_counts
            .insert(kind.as_str().to_string(), count);
    }
}

/// 检索结果集（结果 + 观测）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeRetrievalOutcome {
    pub results: Vec<KnowledgeResult>,
    pub diagnostics: KnowledgeDiagnostics,
}

impl KnowledgeRetrievalOutcome {
    /// 是否什么都没找到（§67：必须如实回答「没有找到」，不得猜测）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
}

/// 稳定 id 生成（FNV-1a 64，确定性、无 crypto 依赖，跨运行稳定）。
#[must_use]
pub fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0x1f; // 分隔符，避免 ("ab","c") 与 ("a","bc") 碰撞
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}-{hash:016x}")
}

/// 片段截断（按字符，附截断标记）。
#[must_use]
pub fn snippet(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_round_trip() {
        for kind in KnowledgeSourceKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let back: KnowledgeSourceKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
            assert!(!kind.label().is_empty());
        }
        assert_eq!(
            KnowledgeSourceKind::parse("DOCUMENT"),
            Some(KnowledgeSourceKind::Document)
        );
        assert_eq!(KnowledgeSourceKind::parse("nope"), None);
    }

    #[test]
    fn stable_id_is_deterministic_and_separated() {
        let a = stable_id("doc", &["root1", "notes/a.md"]);
        let b = stable_id("doc", &["root1", "notes/a.md"]);
        assert_eq!(a, b);
        assert!(a.starts_with("doc-"));
        assert_eq!(a.len(), 20);
        assert_ne!(
            stable_id("doc", &["ab", "c"]),
            stable_id("doc", &["a", "bc"])
        );
        assert_ne!(
            stable_id("doc", &["r", "p"]),
            stable_id("file", &["r", "p"])
        );
    }

    #[test]
    fn budget_limits_are_bounded() {
        let budget = KnowledgeBudget::default();
        assert_eq!(budget.limit_for(KnowledgeSourceKind::Memory), 4);
        assert_eq!(budget.limit_for(KnowledgeSourceKind::Document), 3);
        assert_eq!(budget.limit_for(KnowledgeSourceKind::File), 0);
        assert!(budget.max_results >= budget.max_per_source);
    }

    #[test]
    fn result_carries_provenance_and_identity() {
        let result = KnowledgeResult::new(
            KnowledgeSourceKind::Document,
            "doc-1",
            "Jenkins 记录",
            "AccessDeniedException 出现在 ……",
            0.8,
        )
        .with_location("chunk#3")
        .with_path("D:/资料/Jenkins.md");
        assert_eq!(result.provenance.source_kind, KnowledgeSourceKind::Document);
        assert_eq!(result.provenance.source_id, "doc-1");
        assert_eq!(result.provenance.location.as_deref(), Some("chunk#3"));
        assert_eq!(result.identity().2, Some("chunk#3"));
        assert_eq!(result.canonical_path(), Some("D:/资料/Jenkins.md"));
        assert!(result.char_count() > 0);
    }

    #[test]
    fn query_source_filter_and_snippet_cap() {
        let query = KnowledgeQuery {
            sources: vec![KnowledgeSourceKind::Memory],
            ..KnowledgeQuery::new("docker")
        };
        assert!(query.includes(KnowledgeSourceKind::Memory));
        assert!(!query.includes(KnowledgeSourceKind::File));
        assert!(KnowledgeQuery::new("x").includes(KnowledgeSourceKind::File));

        let long = "字".repeat(50);
        let capped = snippet(&long, 10);
        assert_eq!(capped.chars().count(), 11);
        assert!(capped.ends_with('…'));
        assert_eq!(snippet(" 短 ", 10), "短");
    }

    #[test]
    fn diagnostics_record_counts_without_content() {
        let mut diagnostics = KnowledgeDiagnostics::default();
        diagnostics.record_candidates(KnowledgeSourceKind::Memory, 3);
        diagnostics.record_candidates(KnowledgeSourceKind::Memory, 4);
        diagnostics.record_candidates(KnowledgeSourceKind::File, 1);
        assert_eq!(diagnostics.sources_queried.len(), 2);
        assert_eq!(diagnostics.candidate_counts["memory"], 4);
        let json = serde_json::to_string(&diagnostics).unwrap();
        assert!(!json.contains("content"));
    }
}
