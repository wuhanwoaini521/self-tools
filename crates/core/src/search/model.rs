//! 全局检索数据契约（V11 §119-§122）。
//!
//! 设计约束：
//! - **无 LLM**：查询与结果都是确定性结构，不涉及任何 provider 调用；
//! - **可导航**：每条命中都携带 `action_target`（前端据此跳转），
//!   形状如 `{"module":"history","entity":{"kind":"event","id":"x"}}`；
//! - **可降级**：单源失败只进入 `degraded_sources`，不影响其它源。

use serde::{Deserialize, Serialize};

/// 每个来源的默认条数上限。
pub const DEFAULT_LIMIT_PER_SOURCE: usize = 5;

/// 命中片段（snippet）的最大字符数。
pub const MAX_SNIPPET_CHARS: usize = 300;

/// 全局检索返回的命中总条数硬上限。
pub const MAX_TOTAL_HITS: usize = 50;

/// 全局检索覆盖的来源模块。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    History,
    Travel,
    Geography,
    Language,
    Memory,
    Documents,
    Files,
    StudyBoard,
    Applications,
}

impl SearchSource {
    pub const ALL: [SearchSource; 9] = [
        SearchSource::History,
        SearchSource::Travel,
        SearchSource::Geography,
        SearchSource::Language,
        SearchSource::Memory,
        SearchSource::Documents,
        SearchSource::Files,
        SearchSource::StudyBoard,
        SearchSource::Applications,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SearchSource::History => "history",
            SearchSource::Travel => "travel",
            SearchSource::Geography => "geography",
            SearchSource::Language => "language",
            SearchSource::Memory => "memory",
            SearchSource::Documents => "documents",
            SearchSource::Files => "files",
            SearchSource::StudyBoard => "study_board",
            SearchSource::Applications => "applications",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SearchSource::History => "历史",
            SearchSource::Travel => "旅行",
            SearchSource::Geography => "地理",
            SearchSource::Language => "语言",
            SearchSource::Memory => "记忆",
            SearchSource::Documents => "文档",
            SearchSource::Files => "文件",
            SearchSource::StudyBoard => "学习看板",
            SearchSource::Applications => "应用",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 全局检索查询。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlobalSearchQuery {
    /// 用户输入的原始查询（空白会在服务层被裁剪）。
    pub query: String,
    /// 每个来源最多返回多少条命中。
    #[serde(default = "default_limit_per_source")]
    pub limit_per_source: usize,
    /// 参与的来源；空 = 全部来源。
    #[serde(default)]
    pub sources: Vec<SearchSource>,
}

impl Default for GlobalSearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit_per_source: DEFAULT_LIMIT_PER_SOURCE,
            sources: Vec::new(),
        }
    }
}

impl GlobalSearchQuery {
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    /// 裁剪后的查询词（纯空白视为空）。
    #[must_use]
    pub fn trimmed(&self) -> &str {
        self.query.trim()
    }

    /// 该来源是否参与本次检索（空列表 = 全部）。
    #[must_use]
    pub fn includes(&self, source: SearchSource) -> bool {
        self.sources.is_empty() || self.sources.contains(&source)
    }
}

/// 单条全局检索命中。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlobalSearchHit {
    pub source: SearchSource,
    /// 来源内的实体类型（如 `event` / `trip` / `memory` / `document`）。
    pub kind: String,
    /// 命中标题。
    pub title: String,
    /// 命中片段（截断到 [`MAX_SNIPPET_CHARS`] 字符以内）。
    pub snippet: String,
    /// 前端导航目标（如 `{"module":"history","entity":{"kind":"event","id":"x"}}`）。
    pub action_target: serde_json::Value,
    /// 相关性分数（越大越相关；0.0–1.0 为约定范围，实现可自行标定）。
    pub score: f32,
}

impl GlobalSearchHit {
    #[must_use]
    pub fn new(
        source: SearchSource,
        kind: impl Into<String>,
        title: impl Into<String>,
        snippet: impl Into<String>,
        action_target: serde_json::Value,
        score: f32,
    ) -> Self {
        Self {
            source,
            kind: kind.into(),
            title: title.into(),
            snippet: bound_snippet(&snippet.into()),
            action_target,
            score,
        }
    }
}

/// 全局检索结果。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlobalSearchResult {
    /// 裁剪后的查询词。
    pub query: String,
    /// 命中（按分数倒序）。
    pub hits: Vec<GlobalSearchHit>,
    /// 降级的来源（失败但被跳过，不影响其它源）。
    pub degraded_sources: Vec<SearchSource>,
    /// 命中条数（`hits.len()` 的显式化）。
    pub total: usize,
}

/// 把 snippet 收敛到 [`MAX_SNIPPET_CHARS`] 个字符以内（按字符而非字节，
/// 避免把中文字符切坏）。超出时以省略号结尾，总长度仍不超过上限。
#[must_use]
pub fn bound_snippet(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_SNIPPET_CHARS {
        return text.to_string();
    }
    if MAX_SNIPPET_CHARS <= 1 {
        return "…".to_string();
    }
    let bounded: String = text
        .chars()
        .take(MAX_SNIPPET_CHARS - 1)
        .collect::<String>()
        .trim_end()
        .to_string();
    format!("{bounded}…")
}

fn default_limit_per_source() -> usize {
    DEFAULT_LIMIT_PER_SOURCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_serializes_snake_case() {
        let raw = serde_json::to_string(&SearchSource::StudyBoard).expect("serialize source");
        assert_eq!(raw, "\"study_board\"");
        let parsed: SearchSource =
            serde_json::from_str("\"study_board\"").expect("deserialize source");
        assert_eq!(parsed, SearchSource::StudyBoard);
        assert!(SearchSource::parse("study_board").is_some());
        assert!(SearchSource::parse("nope").is_none());
    }

    #[test]
    fn query_defaults_and_source_filter() {
        let query = GlobalSearchQuery::new("  foo  ");
        assert_eq!(query.limit_per_source, DEFAULT_LIMIT_PER_SOURCE);
        assert!(query.sources.is_empty());
        assert!(query.includes(SearchSource::Memory));
        assert_eq!(query.trimmed(), "foo");

        let scoped = GlobalSearchQuery {
            sources: vec![SearchSource::Files],
            ..GlobalSearchQuery::new("x")
        };
        assert!(scoped.includes(SearchSource::Files));
        assert!(!scoped.includes(SearchSource::History));
    }

    #[test]
    fn hit_snippet_is_bounded() {
        let long = "木".repeat(MAX_SNIPPET_CHARS + 20);
        let hit = GlobalSearchHit::new(
            SearchSource::Documents,
            "document",
            "标题",
            long.clone(),
            serde_json::json!({"module": "documents"}),
            0.5,
        );
        assert!(hit.snippet.chars().count() <= MAX_SNIPPET_CHARS);
        assert!(hit.snippet.ends_with('…'));

        let short = GlobalSearchHit::new(
            SearchSource::Files,
            "file",
            "标题",
            "短",
            serde_json::Value::Null,
            0.1,
        );
        assert_eq!(short.snippet, "短");
    }

    #[test]
    fn empty_query_is_blank() {
        assert!(GlobalSearchQuery::new("   \n\t ").trimmed().is_empty());
    }
}
