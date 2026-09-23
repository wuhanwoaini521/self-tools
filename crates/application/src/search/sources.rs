//! Global Search 模块适配器（V11 §119-§122）。
//!
//! 把两个知识层模块（Memory / Documents / Files）的真实检索接到
//! `GlobalSearchPort`。**零 LLM 依赖**：全部走后端本地索引（§121）。
//!
//! 每个适配器只做三件事：调模块 `search` → 映射成 `GlobalSearchHit`
//! （source/kind/title/snippet/action_target/score）→ 错误转可读文本
//! （聚合层记为 degraded，不中断其它源）。

use std::sync::Arc;

use devtoolbox_core::files::{FileMetadata, FileQuery};
use devtoolbox_core::memory::MemoryItem;
use devtoolbox_core::search::{GlobalSearchHit, GlobalSearchQuery, SearchSource};

/// snippet 上限（与 core 的 `MAX_SNIPPET_CHARS` 对齐；这里只做兜底截断）。
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

/// 取第一行当标题（Memory content 可能是多行）。
fn first_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    if out.is_empty() {
        out.push_str("(无标题)");
    }
    out
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

/// Personal Memory 检索源（用户显式确认过的内容 → 权重最高）。
pub struct MemorySearchPort {
    service: Arc<crate::memory::MemoryService>,
}

impl MemorySearchPort {
    #[must_use]
    pub fn new(service: Arc<crate::memory::MemoryService>) -> Self {
        Self { service }
    }
}

impl crate::search::GlobalSearchPort for MemorySearchPort {
    fn source(&self) -> SearchSource {
        SearchSource::Memory
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        let items = self
            .service
            .search(&query.query, None, query.limit_per_source)
            .map_err(|error| error.to_string())?;
        Ok(items
            .iter()
            .map(|item: &MemoryItem| {
                GlobalSearchHit::new(
                    SearchSource::Memory,
                    "memory",
                    first_line(&item.content, 80),
                    snippet(&item.content),
                    serde_json::json!({
                        "module": "knowledge",
                        "tab": "memory",
                        "memory_id": item.id,
                    }),
                    1.0,
                )
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// 文档索引检索源（标题 + chunk 正文）。
pub struct DocumentSearchPort {
    service: Arc<crate::documents::DocumentService>,
}

impl DocumentSearchPort {
    #[must_use]
    pub fn new(service: Arc<crate::documents::DocumentService>) -> Self {
        Self { service }
    }
}

impl crate::search::GlobalSearchPort for DocumentSearchPort {
    fn source(&self) -> SearchSource {
        SearchSource::Documents
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        let hits = self
            .service
            .search(&query.query, None, query.limit_per_source)
            .map_err(|error| error.to_string())?;
        Ok(hits
            .iter()
            .map(|hit| {
                let document_id = hit.meta.document_id.clone();
                GlobalSearchHit::new(
                    SearchSource::Documents,
                    "document",
                    hit.meta.title.clone(),
                    snippet(&hit.snippet),
                    serde_json::json!({
                        "module": "knowledge",
                        "tab": "documents",
                        "document_id": document_id,
                    }),
                    // 正文命中（有 chunk）比纯标题命中更相关。
                    if hit.chunk_id.is_some() { 0.9 } else { 0.7 },
                )
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// 文件索引检索源（只搜元数据：文件名 / 路径）。
///
/// `settings` 由组合根注入（读一次当前 `KnowledgeSettings`）；文件根未配置
/// 时返回受控降级原因，而不是错误崩溃（§74：如实报告）。
pub struct FileSearchPort {
    service: Arc<crate::files::FileService>,
    settings: Arc<dyn Fn() -> devtoolbox_core::settings::KnowledgeSettings + Send + Sync>,
}

impl FileSearchPort {
    #[must_use]
    pub fn new(
        service: Arc<crate::files::FileService>,
        settings: Arc<dyn Fn() -> devtoolbox_core::settings::KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self { service, settings }
    }
}

impl crate::search::GlobalSearchPort for FileSearchPort {
    fn source(&self) -> SearchSource {
        SearchSource::Files
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        let settings = (self.settings)();
        if !settings.is_configured() {
            return Err("file_roots_not_configured".to_string());
        }
        let spec = FileQuery {
            query: query.query.clone(),
            limit: query.limit_per_source,
            ..FileQuery::default()
        };
        let files = self
            .service
            .search(&settings, &spec)
            .map_err(|error| error.to_string())?;
        Ok(files
            .iter()
            .map(|file: &FileMetadata| {
                GlobalSearchHit::new(
                    SearchSource::Files,
                    "file",
                    file.file_name.clone(),
                    snippet(&file.path),
                    serde_json::json!({
                        "module": "knowledge",
                        "tab": "files",
                        "file_id": file.file_id,
                    }),
                    // 文件按名字/路径匹配 → 固定中等权重。
                    0.6,
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_truncates_on_char_boundary() {
        assert_eq!(snippet("abc"), "abc");
        let long = "深".repeat(400);
        let out = snippet(&long);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= SNIPPET_MAX + 1);
    }

    #[test]
    fn first_line_handles_multiline_and_empty() {
        assert_eq!(first_line("第一行\n第二行", 80), "第一行");
        assert_eq!(first_line("", 80), "(无标题)");
        let long = "x".repeat(120);
        assert!(first_line(&long, 80).ends_with('…'));
    }
}
