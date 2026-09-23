//! Documents 域测试（V6 §96）：增量索引 / 体积门 / 失败隔离 / 检索精排 / 读取边界。
//!
//! 全部用内存 Fake 端口（`DocumentIndexPort` / `DocumentSourcePort`）驱动，不触真实
//! 文件系统与 SQLite：SQLite 语义由 infrastructure 的 store 测试覆盖。
//! `search_candidates` 与 SQLite 实现同语义（标题命中 → 每文档一条、snippet 为空；
//! 正文命中 → 逐 chunk），保证服务层的精排 / 去重逻辑被真实驱动。

use super::*;
use crate::text::keywords;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::documents::{
    DocumentChunk, DocumentMeta, DocumentReadRequest, DocumentType, document_id,
};
use devtoolbox_core::files::KnowledgeRoot;
use devtoolbox_core::knowledge::KnowledgeSourceKind;
use devtoolbox_core::settings::KnowledgeSettings;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 内存索引：语义与 `DocumentIndexSqliteStore` 一致（LIKE 粗筛 + 指纹 + 统计）。
#[derive(Default)]
struct FakeDocumentIndex {
    metas: Mutex<HashMap<String, DocumentMeta>>,
    chunks: Mutex<HashMap<String, Vec<DocumentChunk>>>,
    /// `replace_chunks` 调用记录（断言增量索引不重复写 chunk）。
    chunk_writes: Mutex<Vec<(String, usize)>>,
    /// `remove` 调用记录（断言消失的文件只清索引）。
    removed: Mutex<Vec<String>>,
}

impl DocumentIndexPort for FakeDocumentIndex {
    fn upsert(&self, meta: &DocumentMeta) -> Result<(), DocumentStoreError> {
        self.metas
            .lock()
            .insert(meta.document_id.clone(), meta.clone());
        Ok(())
    }

    fn replace_chunks(
        &self,
        document_id: &str,
        chunks: &[DocumentChunk],
    ) -> Result<(), DocumentStoreError> {
        self.chunks
            .lock()
            .insert(document_id.to_string(), chunks.to_vec());
        self.chunk_writes
            .lock()
            .push((document_id.to_string(), chunks.len()));
        Ok(())
    }

    fn get(&self, document_id: &str) -> Result<Option<DocumentMeta>, DocumentStoreError> {
        Ok(self.metas.lock().get(document_id).cloned())
    }

    fn chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>, DocumentStoreError> {
        let mut chunks = self
            .chunks
            .lock()
            .get(document_id)
            .cloned()
            .unwrap_or_default();
        chunks.sort_by_key(|chunk| chunk.ordinal);
        Ok(chunks)
    }

    fn search_candidates(
        &self,
        keywords: &[String],
        document_type: Option<DocumentType>,
        limit: usize,
    ) -> Result<Vec<DocumentHit>, DocumentStoreError> {
        if keywords.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let needles: Vec<String> = keywords
            .iter()
            .map(|keyword| keyword.to_lowercase())
            .collect();
        let matches = |text: &str| {
            let lowered = text.to_lowercase();
            needles.iter().any(|needle| lowered.contains(needle))
        };
        let mut metas: Vec<DocumentMeta> = self.metas.lock().values().cloned().collect();
        metas.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then(left.document_id.cmp(&right.document_id))
        });
        metas.retain(|meta| document_type.is_none_or(|kind| meta.document_type == kind));

        // 标题命中（每文档一条，与 SQLite 的 `lower(title) LIKE` 分支一致）。
        let mut hits: Vec<DocumentHit> = metas
            .iter()
            .filter(|meta| matches(&meta.title))
            .take(limit)
            .map(|meta| DocumentHit {
                meta: meta.clone(),
                chunk_id: None,
                location: None,
                snippet: String::new(),
                matched_in_title: true,
            })
            .collect();

        // 正文命中（逐 chunk，snippet = chunk 原文，location = 位置描述）。
        let mut body: Vec<(DocumentMeta, DocumentChunk)> = Vec::new();
        for meta in &metas {
            for chunk in self.chunks(&meta.document_id)? {
                if matches(&chunk.text) {
                    body.push((meta.clone(), chunk));
                }
            }
        }
        for (meta, chunk) in body.into_iter().take(limit) {
            hits.push(DocumentHit {
                location: Some(chunk.location.describe()),
                meta,
                chunk_id: Some(chunk.chunk_id),
                snippet: chunk.text,
                matched_in_title: false,
            });
        }
        Ok(hits)
    }

    fn recent(&self, limit: usize) -> Result<Vec<DocumentMeta>, DocumentStoreError> {
        let mut metas: Vec<DocumentMeta> = self.metas.lock().values().cloned().collect();
        metas.sort_by(|left, right| {
            right
                .indexed_at
                .cmp(&left.indexed_at)
                .then(left.document_id.cmp(&right.document_id))
        });
        metas.truncate(limit);
        Ok(metas)
    }

    fn fingerprints(&self, root_id: &str) -> Result<Vec<DocumentFingerprint>, DocumentStoreError> {
        Ok(self
            .metas
            .lock()
            .values()
            .filter(|meta| meta.root_id == root_id)
            .map(|meta| DocumentFingerprint {
                document_id: meta.document_id.clone(),
                root_id: meta.root_id.clone(),
                relative_path: meta.relative_path.clone(),
                size_bytes: meta.size_bytes,
                modified_at: meta.modified_at,
            })
            .collect())
    }

    fn remove(&self, document_id: &str) -> Result<(), DocumentStoreError> {
        self.metas.lock().remove(document_id);
        self.chunks.lock().remove(document_id);
        self.removed.lock().push(document_id.to_string());
        Ok(())
    }

    fn stats(&self) -> Result<DocumentIndexStats, DocumentStoreError> {
        let metas = self.metas.lock();
        Ok(DocumentIndexStats {
            documents: metas.len(),
            chunks: self.chunks.lock().values().map(Vec::len).sum(),
            content_available: metas.values().filter(|meta| meta.content_available).count(),
            metadata_only: metas
                .values()
                .filter(|meta| !meta.content_available && meta.index_error.is_some())
                .count(),
            failed: metas
                .values()
                .filter(|meta| meta.index_error.is_some())
                .count(),
        })
    }
}

/// 内存来源：路径 → 正文（+ 可注入的失败 / 仅元数据 / 体积 / mtime）。
#[derive(Default)]
struct FakeDocumentSource {
    files: HashMap<String, String>,
    /// 强制 `extract` 报错（§91 失败隔离）。
    failures: HashMap<String, String>,
    /// 强制 `extract` 返回 `MetadataOnly`（如 PDF 无抽取能力）。
    metadata_only: HashMap<String, String>,
    /// 覆盖扫描出的体积（无需真的分配大字符串）。
    sizes: HashMap<String, u64>,
    modified: HashMap<String, i64>,
}

const DEFAULT_MODIFIED: i64 = 1_700_000_000;

impl FakeDocumentSource {
    fn with_file(mut self, path: &str, content: &str) -> Self {
        self.files.insert(path.to_string(), content.to_string());
        self
    }

    fn failing(mut self, path: &str, reason: &str) -> Self {
        self.failures.insert(path.to_string(), reason.to_string());
        self
    }

    fn metadata_only(mut self, path: &str, reason: &str) -> Self {
        self.metadata_only
            .insert(path.to_string(), reason.to_string());
        self
    }

    fn with_modified(mut self, path: &str, modified_at: i64) -> Self {
        self.modified.insert(path.to_string(), modified_at);
        self
    }

    fn size_of(&self, path: &str) -> u64 {
        self.sizes.get(path).copied().unwrap_or_else(|| {
            self.files
                .get(path)
                .map_or(0, |content| content.len() as u64)
        })
    }

    fn modified_of(&self, path: &str) -> i64 {
        self.modified.get(path).copied().unwrap_or(DEFAULT_MODIFIED)
    }
}

impl DocumentSourcePort for FakeDocumentSource {
    fn scan_root(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<ScannedDocument>, bool), DocumentStoreError> {
        let prefix = format!("{}/", root.path.trim_end_matches('/'));
        let mut scanned: Vec<ScannedDocument> = self
            .files
            .keys()
            .filter(|path| path.starts_with(&prefix))
            .filter(|path| devtoolbox_core::documents::is_indexable_document(Path::new(path)))
            .map(|path| ScannedDocument {
                path: PathBuf::from(path),
                relative_path: path[prefix.len()..].to_string(),
                size_bytes: self.size_of(path),
                modified_at: self.modified_of(path),
            })
            .collect();
        scanned.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let truncated = limit > 0 && scanned.len() > limit;
        if truncated {
            scanned.truncate(limit);
        }
        Ok((scanned, truncated))
    }

    fn extract(
        &self,
        path: &Path,
        _document_type: DocumentType,
        max_bytes: u64,
    ) -> Result<ExtractedContent, DocumentStoreError> {
        let key = path.to_string_lossy().to_string();
        if let Some(reason) = self.failures.get(&key) {
            return Err(DocumentStoreError(reason.clone()));
        }
        let size = self.size_of(&key);
        if size > max_bytes {
            return Ok(ExtractedContent::MetadataOnly(format!(
                "文件体积 {size} 字节超过索引上限 {max_bytes} 字节"
            )));
        }
        if let Some(reason) = self.metadata_only.get(&key) {
            return Ok(ExtractedContent::MetadataOnly(reason.clone()));
        }
        match self.files.get(&key) {
            Some(content) => Ok(ExtractedContent::Text(content.clone())),
            None => Err(DocumentStoreError(format!("无法读取 {key}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn root() -> KnowledgeRoot {
    KnowledgeRoot::new("docs", "资料", "/data/docs")
}

/// 默认设置：文档根复用文件根为空 → 显式配置一条文档根。
fn settings() -> KnowledgeSettings {
    KnowledgeSettings {
        document_roots: vec![root()],
        ..KnowledgeSettings::default()
    }
}

fn service(source: FakeDocumentSource) -> (DocumentService, Arc<FakeDocumentIndex>) {
    let index = Arc::new(FakeDocumentIndex::default());
    let service =
        DocumentService::with_config(index.clone(), Arc::new(source), DocumentConfig::default());
    (service, index)
}

fn index_once(service: &DocumentService, settings: &KnowledgeSettings) -> IndexReport {
    let mut reports = service
        .index_configured_roots(settings)
        .expect("index configured roots");
    assert_eq!(reports.len(), 1, "只有一个启用的文档根");
    reports.remove(0)
}

/// 3000 字符、无换行无句读 → 硬切成多个 chunk。
fn long_document() -> String {
    "字".repeat(3_000)
}

/// 带两级标题的长文：`## 网络` 之后的正文跨多个 chunk（§37 章节引用）。
fn sectioned_document() -> String {
    format!(
        "# 环境\n{}\n## 网络\n{}\n",
        "a".repeat(400),
        "b".repeat(3_000)
    )
}

// ---------------------------------------------------------------------------
// 检索（§30）
// ---------------------------------------------------------------------------

#[test]
fn search_正文命中返回分数位置与片段() {
    let (service, _index) = service(FakeDocumentSource::default().with_file(
        "/data/docs/notes/jenkins.md",
        "# Jenkins\nAccessDeniedException 出现在权限不足时\n",
    ));
    index_once(&service, &settings());

    let hits = service
        .search("AccessDeniedException", None, 10)
        .expect("search");
    assert!(!hits.is_empty(), "应命中正文");
    let hit = &hits[0];
    assert!(!hit.matched_in_title);
    assert!(hit.chunk_id.is_some(), "正文命中必须带 chunk_id");
    assert!(
        hit.location
            .as_deref()
            .is_some_and(|location| location.contains("字符")),
        "位置描述应含字符区间: {:?}",
        hit.location
    );
    assert!(hit.snippet.contains("AccessDeniedException"));
    assert!(document_score(hit, &keywords("AccessDeniedException")) > 0.0);
}

#[test]
fn search_标题命中排在正文命中之前() {
    let (service, _index) = service(
        FakeDocumentSource::default()
            .with_file("/data/docs/notes/docker.md", "环境配置说明")
            .with_file("/data/docs/notes/other.md", "docker volume 挂载说明"),
    );
    index_once(&service, &settings());

    let hits = service.search("docker", None, 10).expect("search");
    assert_eq!(hits.len(), 2);
    let title_hit = &hits[0];
    let body_hit = &hits[1];
    assert_eq!(
        title_hit.meta.document_id,
        document_id("docs", "notes/docker.md")
    );
    assert!(title_hit.matched_in_title);
    assert!(title_hit.snippet.is_empty(), "标题命中不带正文片段");
    assert!(!body_hit.matched_in_title);
    assert!(
        document_score(title_hit, &keywords("docker"))
            > document_score(body_hit, &keywords("docker")),
        "标题命中分数必须高于正文命中"
    );
}

#[test]
fn search_空查询与无命中都返回空() {
    let (service, _index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/docker.md", "Docker volume"),
    );
    index_once(&service, &settings());

    assert!(service.search("", None, 10).expect("search").is_empty());
    assert!(service.search("   ", None, 10).expect("search").is_empty());
    assert!(
        service
            .search("量子计算", None, 10)
            .expect("search")
            .is_empty()
    );
}

#[test]
fn get_返回元数据_不存在时错误包含文档_id() {
    let (service, _index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/jenkins.md", "Jenkins 记录"),
    );
    index_once(&service, &settings());

    let id = document_id("docs", "notes/jenkins.md");
    let meta = service.get(&id).expect("get");
    assert_eq!(meta.title, "jenkins.md");
    assert_eq!(meta.root_id, "docs");
    assert_eq!(meta.relative_path, "notes/jenkins.md");
    assert!(meta.chunk_count > 0);
    assert!(meta.content_available);
    assert_eq!(meta.document_type, DocumentType::Markdown);

    let error = service.get("doc-missing").expect_err("不存在的文档应报错");
    assert!(error.to_string().contains("doc-missing"));
}

// ---------------------------------------------------------------------------
// 读取（§31）
// ---------------------------------------------------------------------------

#[test]
fn read_按_chunk_id_返回单个_chunk() {
    let (service, index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/long.txt", &long_document()),
    );
    index_once(&service, &settings());

    let id = document_id("docs", "notes/long.txt");
    let chunks = index.chunks(&id).expect("chunks");
    assert!(chunks.len() > 1, "长文必须切成多个 chunk");
    let target = chunks[1].clone();

    let result = service
        .read(
            &id,
            &DocumentReadRequest {
                chunk_id: Some(target.chunk_id.clone()),
                ..Default::default()
            },
        )
        .expect("read chunk");
    assert_eq!(result.text, target.text);
    assert_eq!(result.chunk_ids, vec![target.chunk_id.clone()]);
    assert_eq!(result.total_chunks, chunks.len());
    assert_eq!(result.location, target.location);
    assert_eq!(result.document_id, id);
    assert_eq!(result.title, "long.txt");
    assert!(!result.truncated, "单 chunk 未超过 max_chars");
}

#[test]
fn read_按章节拼接该章节的多个_chunk() {
    let (service, index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/env.md", &sectioned_document()),
    );
    index_once(&service, &settings());

    let id = document_id("docs", "notes/env.md");
    let chunks = index.chunks(&id).expect("chunks");
    assert!(chunks.len() >= 3, "长文必须切成多个 chunk");

    let result = service
        .read(
            &id,
            &DocumentReadRequest {
                section: Some("网络".to_string()),
                ..Default::default()
            },
        )
        .expect("read section");
    assert_eq!(result.location.section.as_deref(), Some("网络"));
    // 起点 = 标题行首（`## 网络` 的 `#`），早于首个「归属网络」的 chunk。
    assert_eq!(result.location.char_start, 407);
    assert!(
        result.chunk_ids.len() >= 2,
        "章节正文跨多个 chunk 时应拼接: {:?}",
        result.chunk_ids
    );
    assert_eq!(result.chunk_ids[0], chunks[0].chunk_id);
    // 标题行（`## 网络`）必须包含在返回文本里（§37：引用要能定位到章节起点）。
    assert!(result.text.contains("网络"), "应包含章节标题");
    assert!(result.text.contains('b'), "应返回该章节的正文");
}

#[test]
fn read_按_offset与max_chars_范围读取() {
    let (service, index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/long.txt", &long_document()),
    );
    index_once(&service, &settings());
    let id = document_id("docs", "notes/long.txt");
    let total = index
        .chunks(&id)
        .expect("chunks")
        .last()
        .unwrap()
        .location
        .char_end;

    // 范围落在单个 chunk 内：字符预算硬生效，且绝不返回整份文档。
    let head = service
        .read(
            &id,
            &DocumentReadRequest {
                offset: 0,
                max_chars: 100,
                ..Default::default()
            },
        )
        .expect("read range");
    assert_eq!(head.text.chars().count(), 100, "字符数不得超上限");
    assert!(head.text.chars().count() < total, "不得返回整份文档");
    assert!(head.truncated, "未读到文档末尾 → truncated");
    assert_eq!(head.location.char_start, 0);
    assert_eq!(head.location.char_end, 100);
    assert!(!head.chunk_ids.is_empty());

    // 偏移量生效：返回的正是该区间的内容。
    let tail = service
        .read(
            &id,
            &DocumentReadRequest {
                offset: 1_500,
                max_chars: 200,
                ..Default::default()
            },
        )
        .expect("read range");
    assert_eq!(tail.text.chars().count(), 200);
    assert!(tail.text.chars().all(|ch| ch == '字'));
    assert_eq!(tail.location.char_start, 1_500);
    assert_eq!(tail.location.char_end, 1_700);
    assert!(tail.truncated);
}

#[test]
fn read_非法_chunk_未知章节与越界_offset_都报错() {
    let (service, _index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/long.txt", &long_document()),
    );
    index_once(&service, &settings());
    let id = document_id("docs", "notes/long.txt");

    let bad_chunk = service
        .read(
            &id,
            &DocumentReadRequest {
                chunk_id: Some("long.txt#99".to_string()),
                ..Default::default()
            },
        )
        .expect_err("不存在的 chunk 应报错");
    assert!(bad_chunk.to_string().contains("chunk"));

    let bad_section = service
        .read(
            &id,
            &DocumentReadRequest {
                section: Some("不存在的章节".to_string()),
                ..Default::default()
            },
        )
        .expect_err("不存在的章节应报错");
    assert!(bad_section.to_string().contains("章节"));

    let bad_offset = service
        .read(
            &id,
            &DocumentReadRequest {
                offset: 99_999,
                ..Default::default()
            },
        )
        .expect_err("越界 offset 应报错");
    assert!(bad_offset.to_string().contains("offset"));
}

// ---------------------------------------------------------------------------
// 仅元数据文档（§35/§92）
// ---------------------------------------------------------------------------

#[test]
fn metadata_only_document_not_readable_but_title_searchable() {
    let (service, index) = service(
        FakeDocumentSource::default()
            .with_file("/data/docs/manual.pdf", "%PDF-1.7 二进制内容")
            .metadata_only("/data/docs/manual.pdf", "PDF 暂不支持正文抽取"),
    );
    let report = index_once(&service, &settings());
    assert_eq!(report.metadata_only, 1);
    assert_eq!(report.indexed, 0);
    assert_eq!(report.failed, 0);

    let id = document_id("docs", "manual.pdf");
    let meta = service.get(&id).expect("get");
    assert!(!meta.content_available);
    assert!(!meta.is_indexed_content());
    assert_eq!(meta.index_error.as_deref(), Some("PDF 暂不支持正文抽取"));
    assert!(index.chunks(&id).expect("chunks").is_empty());

    let error = service
        .read(&id, &DocumentReadRequest::default())
        .expect_err("仅元数据文档不可读");
    let message = error.to_string();
    assert!(message.contains("仅索引了元数据"), "应说明原因: {message}");
    assert!(message.contains("PDF 暂不支持正文抽取"));

    let hits = service.search("manual", None, 10).expect("search");
    assert!(
        hits.iter()
            .any(|hit| hit.matched_in_title && hit.meta.document_id == id),
        "仅元数据文档的标题仍应命中"
    );
}

#[test]
fn oversized_document_indexes_metadata_only() {
    let (service, index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/huge.md", &"字".repeat(200)),
    );
    let mut limited = settings();
    limited.max_document_bytes = 64;
    let report = index_once(&service, &limited);
    assert_eq!(report.metadata_only, 1);
    assert_eq!(report.indexed, 0);

    let id = document_id("docs", "notes/huge.md");
    let meta = service.get(&id).expect("get");
    assert!(!meta.content_available);
    assert!(!meta.is_indexed_content());
    assert_eq!(meta.chunk_count, 0);
    assert!(
        meta.index_error
            .as_deref()
            .is_some_and(|reason| reason.contains("超过索引上限")),
        "应记录体积门原因: {:?}",
        meta.index_error
    );
    assert!(index.chunks(&id).expect("chunks").is_empty());
}

#[test]
fn large_document_chunks_sorted_and_multiple() {
    let (service, index) = service(
        FakeDocumentSource::default().with_file("/data/docs/notes/long.md", &"字".repeat(15_000)),
    );
    let report = index_once(&service, &settings());
    assert_eq!(report.indexed, 1);

    let id = document_id("docs", "notes/long.md");
    let meta = service.get(&id).expect("get");
    assert!(meta.chunk_count > 1);
    assert!(meta.content_available);

    let chunks = index.chunks(&id).expect("chunks");
    assert_eq!(chunks.len(), meta.chunk_count);
    for (ordinal, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.ordinal, ordinal, "chunk 必须有序");
        assert_eq!(
            chunk.chunk_id,
            devtoolbox_core::documents::chunk_id(&id, ordinal)
        );
    }

    let stats = service.stats().expect("stats");
    assert_eq!(stats.documents, 1);
    assert!(stats.chunks > 1);
    assert_eq!(stats.content_available, 1);
    assert_eq!(stats.metadata_only, 0);
    assert_eq!(stats.failed, 0);
}

// ---------------------------------------------------------------------------
// 增量索引（§88/§90）
// ---------------------------------------------------------------------------

#[test]
fn index_root_增量_变更重索引_消失则移除() {
    let index = Arc::new(FakeDocumentIndex::default());
    let settings = settings();

    let first_source = FakeDocumentSource::default()
        .with_file("/data/docs/notes/a.md", "Docker volume 说明")
        .with_file("/data/docs/notes/b.md", "Jenkins 记录");
    let service = DocumentService::with_config(
        index.clone(),
        Arc::new(first_source),
        DocumentConfig::default(),
    );
    let first = index_once(&service, &settings);
    assert_eq!((first.scanned, first.indexed, first.unchanged), (2, 2, 0));
    assert_eq!(index.chunk_writes.lock().len(), 2);

    // 同一扫描结果（size + mtime 未变）→ 全部 unchanged，且不再写 chunk。
    let second = index_once(&service, &settings);
    assert_eq!(
        (second.scanned, second.indexed, second.unchanged),
        (2, 0, 2)
    );
    assert_eq!(index.chunk_writes.lock().len(), 2);

    // 内容变化（size 变）→ 只有该文件重新索引。
    let changed = FakeDocumentSource::default()
        .with_file("/data/docs/notes/a.md", "Docker volume 换到了新目录")
        .with_file("/data/docs/notes/b.md", "Jenkins 记录");
    let service =
        DocumentService::with_config(index.clone(), Arc::new(changed), DocumentConfig::default());
    let third = index_once(&service, &settings);
    assert_eq!((third.indexed, third.unchanged), (1, 1));
    assert!(
        index.chunk_writes.lock().len() > 2,
        "变化文件必须重写 chunk"
    );
    assert_eq!(
        service
            .search("换到了新目录", None, 10)
            .expect("search")
            .len(),
        1,
        "重新索引后应能检索到新正文"
    );

    // 只有 mtime 变（size 不变）→ 同样视为变化。
    let touched = FakeDocumentSource::default()
        .with_file("/data/docs/notes/a.md", "Docker volume 换到了新目录")
        .with_file("/data/docs/notes/b.md", "Jenkins 记录")
        .with_modified("/data/docs/notes/a.md", DEFAULT_MODIFIED + 500);
    let service =
        DocumentService::with_config(index.clone(), Arc::new(touched), DocumentConfig::default());
    let fourth = index_once(&service, &settings);
    assert_eq!((fourth.indexed, fourth.unchanged), (1, 1));

    // 文件从扫描结果消失 → 只清索引（不碰文件系统）。
    let shrunk = FakeDocumentSource::default().with_file("/data/docs/notes/b.md", "Jenkins 记录");
    let service =
        DocumentService::with_config(index.clone(), Arc::new(shrunk), DocumentConfig::default());
    let fifth = index_once(&service, &settings);
    assert_eq!((fifth.scanned, fifth.removed), (1, 1));
    assert!(
        index
            .metas
            .lock()
            .get(&document_id("docs", "notes/a.md"))
            .is_none()
    );
    assert!(service.get(&document_id("docs", "notes/a.md")).is_err());
    assert_eq!(index.removed.lock().len(), 1);
}

#[test]
fn index_root_未配置根时不报错_且受文件数上限截断() {
    let (service, _index) = service(
        FakeDocumentSource::default()
            .with_file("/data/docs/notes/a.md", "Docker volume")
            .with_file("/data/docs/notes/b.md", "Jenkins 记录"),
    );

    // 未配置任何根：如实返回空报告（§7 降级而非崩溃）。
    let empty = service
        .index_configured_roots(&KnowledgeSettings::default())
        .expect("空配置不应报错");
    assert!(empty.is_empty());

    // 禁用的根不参与索引。
    let disabled = KnowledgeSettings {
        document_roots: vec![KnowledgeRoot {
            enabled: false,
            ..root()
        }],
        ..KnowledgeSettings::default()
    };
    assert!(
        service
            .index_configured_roots(&disabled)
            .expect("禁用根")
            .is_empty()
    );

    // max_indexed_files 截断（按 relative_path 稳定顺序）。
    let mut limited = settings();
    limited.max_indexed_files = 1;
    let report = index_once(&service, &limited);
    assert_eq!(report.scanned, 1);
    assert!(report.truncated);
    assert_eq!(report.indexed, 1);
}

#[test]
fn denied_patterns_are_metadata_only_and_never_extracted() {
    // §4.3：deny 规则与 Files 侧同源 —— 凭据类文件不得被抽取正文入库。
    let (service, index) = service(
        FakeDocumentSource::default()
            .with_file("/data/docs/notes/ok.md", "Docker volume 说明")
            .with_file(
                "/data/docs/notes/credentials.json",
                r#"{"api_key":"sk-abcdefghijklmnopqrstuvwx"}"#,
            )
            .with_file(
                "/data/docs/notes/secrets.json",
                r#"{"token":"ghp_abcdefghijklmnopqrstuvwx12"}"#,
            ),
    );
    let report = index_once(&service, &settings());
    assert_eq!(report.scanned, 3);
    assert_eq!(report.metadata_only, 2, "两个凭据文件只索引元数据");
    assert_eq!(report.indexed, 1);

    let denied = document_id("docs", "notes/credentials.json");
    let meta = service.get(&denied).expect("denied 文档写元数据");
    assert!(!meta.content_available);
    assert_eq!(
        meta.index_error.as_deref(),
        Some("命中 deny 规则，未索引正文")
    );
    assert!(
        index.chunks(&denied).expect("chunks").is_empty(),
        "deny 文件不得有正文 chunk"
    );

    let secrets = document_id("docs", "notes/secrets.json");
    let secrets_meta = service.get(&secrets).expect("secrets 元数据");
    assert!(!secrets_meta.content_available, "deny 文件不得可读正文");
    assert!(secrets_meta.index_error.is_some());

    // 凭据内容不可被检索命中（LIKE 只在 chunk/title 上跑，两者都空）。
    for query in ["sk-abcdefghijklmnopqrstuvwx", "credentials", "secrets"] {
        assert!(
            service
                .search(query, None, 10)
                .expect("search")
                .iter()
                .all(|hit| { hit.meta.document_id != denied && hit.meta.document_id != secrets }),
            "凭据内容不得可检索: {query}"
        );
    }
}

// ---------------------------------------------------------------------------
// 失败隔离（§91）
// ---------------------------------------------------------------------------

#[test]
fn extract_failure_isolated_and_recorded() {
    let (service, index) = service(
        FakeDocumentSource::default()
            .with_file("/data/docs/notes/ok.md", "Docker volume 说明")
            .with_file("/data/docs/notes/broken.md", "任意内容")
            .failing("/data/docs/notes/broken.md", "编码不是 UTF-8"),
    );
    let report = index_once(&service, &settings());
    assert_eq!(report.scanned, 2);
    assert_eq!(report.failed, 1);
    assert_eq!(report.indexed, 1);
    assert_eq!(report.total_changed(), 2);

    let broken = document_id("docs", "notes/broken.md");
    let meta = service.get(&broken).expect("失败文档也应写入元数据");
    assert_eq!(meta.index_error.as_deref(), Some("编码不是 UTF-8"));
    assert!(!meta.content_available);
    assert!(!meta.is_indexed_content());
    assert!(index.chunks(&broken).expect("chunks").is_empty());

    let hits = service.search("Docker", None, 10).expect("search");
    assert!(!hits.is_empty(), "其它文件仍应正常可检索");
    assert!(hits.iter().all(|hit| hit.meta.document_id != broken));
}

// ---------------------------------------------------------------------------
// Provenance（§37/§66）
// ---------------------------------------------------------------------------

#[test]
fn to_knowledge_results_每条都带_provenance() {
    let (service, _index) = service(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# 环境\nDocker volume 挂载在 /Volumes/Data\n",
    ));
    index_once(&service, &settings());

    let query = "docker";
    let hits = service.search(query, None, 10).expect("search");
    assert!(hits.iter().any(|hit| hit.matched_in_title));
    assert!(hits.iter().any(|hit| !hit.matched_in_title));

    let results = service.to_knowledge_results(&hits, query);
    assert_eq!(results.len(), hits.len());
    for (result, hit) in results.iter().zip(hits.iter()) {
        assert_eq!(result.source_type, KnowledgeSourceKind::Document);
        assert_eq!(result.provenance.source_kind, KnowledgeSourceKind::Document);
        assert_eq!(result.source_id, result.provenance.source_id);
        assert_eq!(result.provenance.source_id, hit.meta.document_id);
        assert_eq!(result.title, hit.meta.title);
        assert_eq!(result.provenance.title, hit.meta.title);
        assert_eq!(
            result.provenance.path.as_deref(),
            Some(hit.meta.path.as_str())
        );
        assert!(
            result
                .provenance
                .path
                .as_deref()
                .is_some_and(|path| !path.is_empty())
        );
        assert!(result.score > 0.0);

        let location = result.location.clone().unwrap_or_default();
        assert!(!location.is_empty(), "位置不得为空");
        if hit.matched_in_title {
            assert!(
                location.starts_with("chunk#"),
                "无正文命中时回落到 chunk#: {location}"
            );
        } else {
            assert!(
                location.contains("字符") || location.contains('§'),
                "正文命中应带章节 / 字符位置: {location}"
            );
            assert!(result.snippet.contains("Docker"));
        }
    }
}
