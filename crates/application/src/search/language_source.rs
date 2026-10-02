//! Language 检索源：把语言词典接到全局搜索。
//!
//! `SearchSource::Language` 早已在 core 里定义，但此前**没有任何端口注册它**，
//! 因此全局搜索搜不到任何词条 / 句子，只能在 Language 页面里用模块私有的
//! `language_search` 命令搜——那是第二套搜索入口。
//!
//! 本适配器复用 Language 已有的 `LanguageService::search`（真实 SQL + FTS5，
//! 0.0 依赖），把命中映射成统一的 `GlobalSearchHit`，前端 ⌘K 即可搜到语言内容。

use std::sync::Arc;

use devtoolbox_core::search::{GlobalSearchHit, GlobalSearchQuery, SearchSource};

use crate::language::LanguageService;

/// snippet 上限（与 core 的 `MAX_SNIPPET_CHARS` 对齐）。
const SNIPPET_MAX: usize = 300;

fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= SNIPPET_MAX {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(SNIPPET_MAX).collect();
    out.push('…');
    out
}

/// Language 检索源。
pub struct LanguageSearchPort {
    service: Arc<LanguageService>,
}

impl LanguageSearchPort {
    #[must_use]
    pub fn new(service: Arc<LanguageService>) -> Self {
        Self { service }
    }
}

impl crate::search::GlobalSearchPort for LanguageSearchPort {
    fn source(&self) -> SearchSource {
        SearchSource::Language
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        let trimmed = query.trimmed();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        // `GlobalSearchService` 把 `limit_per_source == 0` 视为「用默认值」，
        // 端口拿到的是**原始** query，所以这里必须做同样的回落，否则 0 会被
        // 原样透传给模块 search，返回空结果（聚合层再截断也救不回来）。
        let limit = if query.limit_per_source == 0 {
            devtoolbox_core::search::DEFAULT_LIMIT_PER_SOURCE
        } else {
            query.limit_per_source
        };
        let hits = self
            .service
            .search(None, trimmed, limit)
            .map_err(|error| error.to_string())?;
        Ok(hits
            .into_iter()
            .map(|hit| {
                let item = hit.item;
                // snippet 用命中字段 + 首个释义（`LanguageService::search` 不返回释义，
                // 因此这里只用条目自身的读音 / 转写，不编造释义文本）。
                let mut context = item.text.clone();
                if let Some(reading) = item.reading.as_ref() {
                    context.push_str(&format!("  {reading}"));
                }
                if let Some(romanization) = item.romanization.as_ref() {
                    context.push_str(&format!("  {romanization}"));
                }
                let kind = item.item_type.label().to_string();
                GlobalSearchHit::new(
                    SearchSource::Language,
                    item.item_type.label(),
                    item.text.clone(),
                    snippet(&context),
                    serde_json::json!({
                        "module": "language",
                        "tab": "explore",
                        "itemId": item.id,
                        "itemType": kind,
                    }),
                    0.8,
                )
            })
            .collect())
    }
}
