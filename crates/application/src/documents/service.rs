//! Documents 用例（V6 Track B，§27-§39）。
//!
//! - 索引：按 `(path, size, mtime)` 增量；单文件失败隔离；体积门 → 仅元数据。
//! - 检索：词法粗筛（索引库 LIKE）+ 服务层精排（标题命中 > 正文命中）。
//! - 读取：chunk / section / range 三选一，**默认不整份读取**（§31）。
//! - 检索结果一律携带 provenance（§37/§66）。

use std::sync::Arc;

use devtoolbox_core::documents::{
    ChunkConfig, DocumentChunk, DocumentMeta, DocumentReadRequest, DocumentReadResult,
    DocumentType, document_id,
};
use devtoolbox_core::files::{FileAccessPolicy, KnowledgeRoot};
use devtoolbox_core::knowledge::{KnowledgeResult, KnowledgeSourceKind, snippet};
use devtoolbox_core::settings::KnowledgeSettings;

use crate::documents::ports::{
    DocumentHit, DocumentIndexPort, DocumentIndexStats, DocumentSourcePort, ExtractedContent,
};
use crate::error::ApplicationError;
use crate::text::{coverage, keywords, normalize, snippet_around};
use crate::time::now_unix;

/// 服务配置（上限集中）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentConfig {
    pub chunk_config: ChunkConfig,
    /// 检索默认上限。
    pub search_limit: usize,
    /// 候选放大倍数（粗筛 → 精排）。
    pub candidate_factor: usize,
    /// 返回片段字符上限。
    pub snippet_chars: usize,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self {
            chunk_config: ChunkConfig::default(),
            search_limit: 10,
            candidate_factor: 4,
            snippet_chars: 500,
        }
    }
}

/// 一轮索引报告（观测用；只计数与耗时，不含正文，§93/§94）。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IndexReport {
    pub root_id: String,
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub metadata_only: usize,
    pub failed: usize,
    pub removed: usize,
    pub truncated: bool,
    pub duration_ms: u64,
}

impl IndexReport {
    #[must_use]
    pub fn total_changed(&self) -> usize {
        self.indexed + self.metadata_only + self.failed + self.removed
    }
}

/// Documents 服务。
pub struct DocumentService {
    index: Arc<dyn DocumentIndexPort>,
    source: Arc<dyn DocumentSourcePort>,
    config: DocumentConfig,
    /// §4.3 deny 规则（与 Files 侧同源；检索阶段过滤凭据类文档）。
    deny_policy: FileAccessPolicy,
}

impl DocumentService {
    #[must_use]
    pub fn new(index: Arc<dyn DocumentIndexPort>, source: Arc<dyn DocumentSourcePort>) -> Self {
        Self {
            index,
            source,
            config: DocumentConfig::default(),
            deny_policy: FileAccessPolicy::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn with_config(
        index: Arc<dyn DocumentIndexPort>,
        source: Arc<dyn DocumentSourcePort>,
        config: DocumentConfig,
    ) -> Self {
        Self {
            index,
            source,
            config,
            deny_policy: FileAccessPolicy::new(Vec::new()),
        }
    }

    /// 按设置索引全部文档根（空配置 → 无根，返回空报告，不报错）。
    pub fn index_configured_roots(
        &self,
        settings: &KnowledgeSettings,
    ) -> Result<Vec<IndexReport>, ApplicationError> {
        let roots = settings.effective_document_roots();
        let mut reports = Vec::with_capacity(roots.len());
        for root in &roots {
            if !root.enabled {
                continue;
            }
            reports.push(self.index_root(root, settings)?);
        }
        Ok(reports)
    }

    /// 索引单个根（幂等、增量、失败隔离）。
    pub fn index_root(
        &self,
        root: &KnowledgeRoot,
        settings: &KnowledgeSettings,
    ) -> Result<IndexReport, ApplicationError> {
        let started = std::time::Instant::now();
        let mut report = IndexReport {
            root_id: root.id.clone(),
            ..IndexReport::default()
        };
        let (scanned, truncated) = self.source.scan_root(root, settings.max_indexed_files)?;
        report.scanned = scanned.len();
        report.truncated = truncated;

        let chunk_config = ChunkConfig {
            max_document_bytes: settings.max_document_bytes,
            ..self.config.chunk_config
        };
        let existing: std::collections::HashMap<String, (u64, i64)> = self
            .index
            .fingerprints(&root.id)?
            .into_iter()
            .map(|fingerprint| {
                (
                    fingerprint.relative_path.clone(),
                    (fingerprint.size_bytes, fingerprint.modified_at),
                )
            })
            .collect();

        // §4.3：deny 规则与 Files 侧同源（同一个 `FileAccessPolicy` 构造）。
        let deny_policy = FileAccessPolicy::new(settings.file_policy().roots().to_vec());
        let mut seen: Vec<String> = Vec::with_capacity(scanned.len());
        let now = now_unix();
        for file in &scanned {
            seen.push(file.relative_path.clone());
            if existing
                .get(&file.relative_path)
                .is_some_and(|(size, modified)| {
                    *size == file.size_bytes && *modified == file.modified_at
                })
            {
                report.unchanged += 1;
                continue;
            }
            // §91：单文件（含索引库写入）失败都只计数，继续其它文件。
            match self.index_file(
                root,
                &file.path,
                &file.relative_path,
                file.size_bytes,
                file.modified_at,
                &chunk_config,
                now,
                &deny_policy,
            ) {
                Ok(IndexOutcome::Indexed {
                    metadata_only: true,
                }) => report.metadata_only += 1,
                Ok(IndexOutcome::Indexed {
                    metadata_only: false,
                }) => report.indexed += 1,
                Ok(IndexOutcome::Failed) => report.failed += 1,
                Err(_) => report.failed += 1,
            }
        }

        // 清理已消失的文件（只清索引，不动文件系统，§88）。
        for (relative, _) in existing {
            if !seen.contains(&relative) {
                self.index.remove(&document_id(&root.id, &relative))?;
                report.removed += 1;
            }
        }
        report.duration_ms = started.elapsed().as_millis() as u64;
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn index_file(
        &self,
        root: &KnowledgeRoot,
        path: &std::path::Path,
        relative_path: &str,
        size_bytes: u64,
        modified_at: i64,
        chunk_config: &ChunkConfig,
        now: i64,
        deny_policy: &FileAccessPolicy,
    ) -> Result<IndexOutcome, ApplicationError> {
        let document_type = devtoolbox_core::documents::detect_document_type(path);
        let id = document_id(&root.id, relative_path);
        let title = devtoolbox_core::files::file_name_of(relative_path);
        // §4.3 deny 规则与 Files 侧**同源**（`DEFAULT_DENY_PATTERNS`）：
        // 用户把同一目录配成 document_roots 时，凭据类文件不得被抽取正文
        // 落入 documents.db（否则 Files 说受限、Documents 却把全文读走）。
        let denied = deny_policy.is_denied(relative_path, &title);
        let meta_base = DocumentMeta {
            document_id: id.clone(),
            root_id: root.id.clone(),
            title,
            document_type,
            path: path.to_string_lossy().to_string(),
            relative_path: relative_path.to_string(),
            size_bytes,
            modified_at,
            indexed_at: now,
            chunk_count: 0,
            content_available: false,
            visibility: Default::default(),
            index_error: None,
        };

        if denied {
            // 只写元数据（content_available=false），不抽取、不切块、不落正文。
            let mut meta = meta_base;
            meta.index_error = Some("命中 deny 规则，未索引正文".to_string());
            self.index.upsert(&meta)?;
            self.index.replace_chunks(&id, &[])?;
            return Ok(IndexOutcome::Indexed {
                metadata_only: true,
            });
        }

        let extracted =
            match self
                .source
                .extract(path, document_type, chunk_config.max_document_bytes)
            {
                Ok(extracted) => extracted,
                Err(error) => {
                    // §91：单文件失败不拖垮整轮索引，但如实记录原因。
                    let mut meta = meta_base;
                    meta.index_error = Some(error.0);
                    self.index.upsert(&meta)?;
                    self.index.replace_chunks(&id, &[])?;
                    return Ok(IndexOutcome::Failed);
                }
            };

        match extracted {
            ExtractedContent::Text(text) => {
                let chunks = devtoolbox_core::documents::chunk_text(&id, &text, chunk_config);
                let mut meta = meta_base;
                meta.chunk_count = chunks.len();
                meta.content_available = !chunks.is_empty();
                self.index.upsert(&meta)?;
                self.index.replace_chunks(&id, &chunks)?;
                Ok(IndexOutcome::Indexed {
                    metadata_only: !meta.content_available,
                })
            }
            ExtractedContent::MetadataOnly(reason) => {
                let mut meta = meta_base;
                meta.index_error = Some(reason);
                self.index.upsert(&meta)?;
                self.index.replace_chunks(&id, &[])?;
                Ok(IndexOutcome::Indexed {
                    metadata_only: true,
                })
            }
        }
    }

    /// 检索（§30）：词法粗筛 + 精排；空 query → 返回空（不返回全部）。
    pub fn search(
        &self,
        query: &str,
        document_type: Option<DocumentType>,
        limit: usize,
    ) -> Result<Vec<DocumentHit>, ApplicationError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let limit = if limit == 0 {
            self.config.search_limit
        } else {
            limit
        };
        let tokens = keywords(query);
        let candidates = self.index.search_candidates(
            &tokens,
            document_type,
            limit
                .saturating_mul(self.config.candidate_factor)
                .max(limit),
        )?;
        let mut hits: Vec<DocumentHit> = candidates
            .into_iter()
            // §4.3：deny 命中的文档不进模型检索结果（与 Files `include_restricted=false` 对齐）。
            .filter(|hit| {
                !self
                    .deny_policy
                    .is_denied(&hit.meta.relative_path, &hit.meta.title)
            })
            .map(|mut hit| {
                hit.snippet = snippet_around(&hit.snippet, &tokens, self.config.snippet_chars);
                hit
            })
            .collect();
        hits.sort_by(|left, right| {
            document_score(right, &tokens)
                .partial_cmp(&document_score(left, &tokens))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.meta.document_id.cmp(&right.meta.document_id))
                .then(left.chunk_id.cmp(&right.chunk_id))
        });
        hits.dedup_by(|left, right| {
            left.meta.document_id == right.meta.document_id && left.chunk_id == right.chunk_id
        });
        hits.truncate(limit);
        Ok(hits)
    }

    pub fn get(&self, document_id: &str) -> Result<DocumentMeta, ApplicationError> {
        self.index
            .get(document_id)?
            .ok_or_else(|| documents_error(format!("文档 `{document_id}` 不存在")))
    }

    /// 某文档的全部分块（`get_context` / UI 章节列表用；不读取文件系统）。
    pub fn chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>, ApplicationError> {
        Ok(self.index.chunks(document_id)?)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<DocumentMeta>, ApplicationError> {
        let limit = if limit == 0 { 20 } else { limit };
        Ok(self.index.recent(limit)?)
    }

    pub fn stats(&self) -> Result<DocumentIndexStats, ApplicationError> {
        Ok(self.index.stats()?)
    }

    /// 读取（§31）：chunk / section / range。元数据-only 文档 → 受控错误。
    pub fn read(
        &self,
        document_id: &str,
        request: &DocumentReadRequest,
    ) -> Result<DocumentReadResult, ApplicationError> {
        let meta = self.get(document_id)?;
        if !meta.is_indexed_content() {
            return Err(documents_error(format!(
                "文档《{}》仅索引了元数据（{}），无法读取正文",
                meta.title,
                meta.index_error
                    .clone()
                    .unwrap_or_else(|| "内容不可用".to_string())
            )));
        }
        let chunks = self.index.chunks(document_id)?;
        if chunks.is_empty() {
            return Err(documents_error(format!(
                "文档《{}》没有可读内容",
                meta.title
            )));
        }
        let max_chars = if request.max_chars == 0 {
            2_000
        } else {
            request.max_chars
        };
        let total_end = chunks
            .last()
            .map(|chunk| chunk.location.char_end)
            .unwrap_or(0);
        if request.offset >= total_end && request.chunk_id.is_none() && request.section.is_none() {
            return Err(documents_error(format!(
                "offset {} 超出文档《{}》长度（{total_end} 字符）",
                request.offset, meta.title
            )));
        }

        if let Some(chunk_id) = request.chunk_id.as_deref() {
            let chunk = chunks
                .iter()
                .find(|chunk| chunk.chunk_id == chunk_id)
                .ok_or_else(|| documents_error(format!("chunk `{chunk_id}` 不存在")))?;
            return Ok(single_chunk_result(&meta, chunk, max_chars));
        }

        if let Some(section) = request
            .section
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let needle = normalize(section);
            // 按 chunk 归属章节筛选（一个 chunk 只归属一个章节标题，§37）。
            let matched: Vec<&DocumentChunk> = chunks
                .iter()
                .filter(|chunk| {
                    chunk
                        .location
                        .section
                        .as_deref()
                        .is_some_and(|value| normalize(value).contains(&needle))
                })
                .collect();
            if matched.is_empty() {
                return Err(documents_error(format!(
                    "文档《{}》没有匹配章节 `{section}`",
                    meta.title
                )));
            }
            // 章节起点 = 标题在原文中的位置（§37）。标题可能落在前一个 chunk 的
            // 尾部（overlap 区间），此时起点早于首个匹配 chunk —— 正确：
            // 读取区间应包含标题本身，而不是从章节中段开始（旧行为会把上一章节
            // 的 overlap 尾巴带进来，`location.section` 报错章）。
            let first = matched[0];
            let offset = self
                .heading_position(&chunks, &needle, first)
                .unwrap_or(first.location.char_start);
            return Ok(range_result(&meta, &chunks, offset, max_chars));
        }

        Ok(range_result(&meta, &chunks, request.offset, max_chars))
    }

    /// 在 chunk 序列里定位章节标题的原文位置（`normalize` 后的子串匹配）。
    ///
    /// chunk 之间有 overlap：标题可能属于前一个 chunk 的尾部，因此只在
    /// `[chunk.char_start, chunk.char_end)` 区间内查找，避免跨 chunk 误定位。
    fn heading_position(
        &self,
        chunks: &[DocumentChunk],
        needle: &str,
        first: &DocumentChunk,
    ) -> Option<usize> {
        chunks
            .iter()
            .filter(|chunk| chunk.ordinal <= first.ordinal)
            .find_map(|chunk| {
                let local = chunk.text.find(needle)?;
                let title_start = chunk.location.char_start + chunk.text[..local].chars().count();
                // 回退到标题行首（`## ` 的 `#`），让读取区间包含标题本身。
                let line_start = title_start.saturating_sub(2);
                Some(line_start)
            })
    }

    /// 检索命中 → 统一结果（provenance 保留文档 id + 位置）。
    pub fn to_knowledge_results(&self, hits: &[DocumentHit], query: &str) -> Vec<KnowledgeResult> {
        let tokens = keywords(query);
        hits.iter()
            .map(|hit| {
                let score = document_score(hit, &tokens);
                let location = hit.location.clone().unwrap_or_else(|| {
                    format!("chunk#{}", hit.chunk_id.clone().unwrap_or_default())
                });
                KnowledgeResult::new(
                    KnowledgeSourceKind::Document,
                    hit.meta.document_id.clone(),
                    hit.meta.title.clone(),
                    snippet(&hit.snippet, self.config.snippet_chars),
                    score,
                )
                .with_location(location)
                .with_path(hit.meta.path.clone())
                .with_metadata(serde_json::json!({
                    "document_type": hit.meta.document_type.as_str(),
                    "relative_path": hit.meta.relative_path,
                    "modified_at": hit.meta.modified_at,
                    "chunk_id": hit.chunk_id,
                    "content_available": hit.meta.content_available,
                    "index_error": hit.meta.index_error,
                }))
            })
            .collect()
    }
}

enum IndexOutcome {
    Indexed { metadata_only: bool },
    Failed,
}

fn single_chunk_result(
    meta: &DocumentMeta,
    chunk: &DocumentChunk,
    max_chars: usize,
) -> DocumentReadResult {
    let truncated = chunk.text.chars().count() > max_chars;
    let text: String = chunk.text.chars().take(max_chars).collect();
    DocumentReadResult {
        document_id: meta.document_id.clone(),
        title: meta.title.clone(),
        text,
        location: chunk.location.clone(),
        chunk_ids: vec![chunk.chunk_id.clone()],
        total_chunks: meta.chunk_count,
        truncated,
    }
}

/// 从 `offset` 开始按 chunk 顺序拼接至多 `max_chars`（不整份读取）。
fn range_result(
    meta: &DocumentMeta,
    chunks: &[DocumentChunk],
    offset: usize,
    max_chars: usize,
) -> DocumentReadResult {
    let mut text = String::new();
    let mut chunk_ids: Vec<String> = Vec::new();
    let mut location = devtoolbox_core::documents::DocumentLocation::default();
    let mut first = true;
    let mut pending_section = false;
    let mut just_started = false;
    let start = offset;
    let end = offset.saturating_add(max_chars);
    for chunk in chunks {
        if chunk.location.char_end <= start {
            continue;
        }
        if chunk.location.char_start >= end {
            break;
        }
        let local_start = start.saturating_sub(chunk.location.char_start);
        let local_end = (end - chunk.location.char_start).min(chunk.text.chars().count());
        if local_end <= local_start {
            continue;
        }
        let piece: String = chunk
            .text
            .chars()
            .skip(local_start)
            .take(local_end - local_start)
            .collect();
        // `location.section` 取「读取区间实际命中的章节」：起始 chunk 因 overlap
        // 可能仍归属上一章节，需要向前找到区间内第一个章节归属发生变化的 chunk。
        let in_range_section = if local_start == 0 {
            chunk.location.section.clone()
        } else {
            None
        };
        if first {
            location = devtoolbox_core::documents::DocumentLocation {
                section: chunk.location.section.clone(),
                page: chunk.location.page,
                char_start: chunk.location.char_start + local_start,
                char_end: chunk.location.char_start + local_end,
            };
            first = false;
            // 起始 chunk 的正文中段开始读时，其 section 属于前文；若后续 chunk
            // 提供了更接近的归属，用第一个后续 chunk 的 section 覆盖。
            pending_section = in_range_section.is_none();
            just_started = true;
        } else {
            location.char_end = chunk.location.char_start + local_end;
        }
        // 起始 chunk 的 section 属于前文（读取点在其中段），用**后续** chunk 的
        // 第一个 section 覆盖 —— 那才是读取区间真正落入的章节。
        if pending_section
            && !just_started
            && let Some(section) = chunk.location.section.clone()
        {
            location.section = Some(section);
            pending_section = false;
        }
        just_started = false;
        chunk_ids.push(chunk.chunk_id.clone());
        text.push_str(&piece);
    }
    let total_end = chunks
        .last()
        .map(|chunk| chunk.location.char_end)
        .unwrap_or(0);
    DocumentReadResult {
        document_id: meta.document_id.clone(),
        title: meta.title.clone(),
        truncated: location.char_end < total_end,
        text,
        location,
        chunk_ids,
        total_chunks: meta.chunk_count,
    }
}

/// 文档打分（确定性）：标题命中 1.0 基准，正文命中 0.6 基准，叠加关键词覆盖率。
#[must_use]
pub fn document_score(hit: &DocumentHit, tokens: &[String]) -> f32 {
    let base = if hit.matched_in_title { 0.7 } else { 0.4 };
    let title_normalized = normalize(&hit.meta.title);
    let body_normalized = normalize(&hit.snippet);
    let title_coverage = coverage(&title_normalized, tokens);
    let body_coverage = coverage(&body_normalized, tokens);
    let length_bonus = if hit.meta.content_available {
        0.05
    } else {
        0.0
    };
    (base + 0.2 * title_coverage + 0.1 * body_coverage + length_bonus).clamp(0.0, 1.0)
}

fn documents_error(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Documents {
        message: message.into(),
    }
}

impl From<crate::documents::ports::DocumentStoreError> for ApplicationError {
    fn from(error: crate::documents::ports::DocumentStoreError) -> Self {
        documents_error(error.0)
    }
}
