//! Documents 模块测试（V6 §94/§113）：工具面 + agent 路由 + 写入边界。
//!
//! 用内存 Fake 索引 / 来源（与 `crate::documents::tests` 同款语义）驱动，
//! 不触真实文件系统与 SQLite。重点断言：
//! - 模块**没有**索引写入工具（`documents.scan` 不存在）；
//! - 检索 / 读取 / 上下文都只读已索引内容；
//! - 结果带 `ui_hint`（`document_list` / `document_card` / `document_reference`）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::documents::{
    DocumentChunk, DocumentHit, DocumentMeta, DocumentType, DocumentFingerprint, chunk_text,
};
use devtoolbox_core::files::KnowledgeRoot;
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::settings::KnowledgeSettings;
use devtoolbox_core::{ToolRisk, UiBlockKind};

use crate::documents::ports::{
    DocumentIndexPort, DocumentSourcePort, DocumentStoreError,
};
use crate::documents::{
    DocumentConfig, DocumentService, ExtractedContent, IndexReport, ScannedDocument,
};
use crate::personal_ai::context::ContextBudget;
use crate::personal_ai::documents::{
    DocumentsTools, documents_tool_names, register_documents,
};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

// ---------------------------------------------------------------------------
// Fakes（与 documents/tests.rs 相同语义：内存索引 + 内存来源）
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeDocumentIndex {
    metas: Mutex<HashMap<String, DocumentMeta>>,
    chunks: Mutex<HashMap<String, Vec<DocumentChunk>>>,
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
        metas.retain(|meta| document_type.is_none_or(|kind| meta.document_type == kind));

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
        Ok(())
    }

    fn stats(&self) -> Result<devtoolbox_core::documents::DocumentIndexStats, DocumentStoreError> {
        let metas = self.metas.lock();
        Ok(devtoolbox_core::documents::DocumentIndexStats {
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

#[derive(Default)]
struct FakeDocumentSource {
    files: HashMap<String, String>,
}

impl FakeDocumentSource {
    fn with_file(mut self, path: &str, content: &str) -> Self {
        self.files.insert(path.to_string(), content.to_string());
        self
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
                size_bytes: self.files.get(path).map_or(0, |content| content.len() as u64),
                modified_at: 1_700_000_000,
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
        _max_bytes: u64,
    ) -> Result<ExtractedContent, DocumentStoreError> {
        let key = path.to_string_lossy().to_string();
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

fn settings() -> KnowledgeSettings {
    KnowledgeSettings {
        document_roots: vec![root()],
        ..KnowledgeSettings::default()
    }
}

fn settings_closure() -> Arc<dyn Fn() -> KnowledgeSettings + Send + Sync> {
    Arc::new(settings)
}

fn hub(source: FakeDocumentSource) -> (Arc<DocumentService>, ToolRegistry, ModuleRegistry) {
    let service = Arc::new(DocumentService::with_config(
        Arc::new(FakeDocumentIndex::default()),
        Arc::new(source),
        DocumentConfig::default(),
    ));
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_documents(
        &mut modules,
        &mut tools,
        Arc::clone(&service),
        settings_closure(),
    )
    .expect("register documents module");
    (service, tools, modules)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

/// 取第 ordinal 个 chunk 的 id（模块读取测试用）。
fn chunk_id_for(service: &DocumentService, document_id: &str, ordinal: usize) -> String {
    service
        .chunks(document_id)
        .expect("chunks")
        .into_iter()
        .nth(ordinal)
        .map(|chunk| chunk.chunk_id)
        .expect("chunk exists")
}

/// 索引一份文档（测试里手动触发索引；模块本身没有索引写入工具）。
fn index(service: &DocumentService, path: &str, _content: &str) -> String {
    let reports: Vec<IndexReport> = service
        .index_configured_roots(&settings())
        .expect("index configured roots");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].indexed, 1, "{path} 应被索引");
    devtoolbox_core::documents::document_id("docs", &path["/data/docs/".len()..])
}

// ---------------------------------------------------------------------------
// 工具面
// ---------------------------------------------------------------------------

#[test]
fn registers_descriptor_and_five_read_tools_without_write() {
    let (_service, tools, modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 说明",
    ));
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "documents");
    assert_eq!(descriptors[0].tools.len(), 5);
    assert!(
        modules.context_provider("documents").is_some(),
        "模块必须提供 ContextProvider"
    );

    let mut names: Vec<String> = tools.specs().iter().map(|spec| spec.name.clone()).collect();
    names.sort();
    let mut expected: Vec<String> = documents_tool_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    for spec in tools.specs() {
        assert_eq!(spec.risk, ToolRisk::Read, "{}", spec.name);
        assert_eq!(spec.module, "documents");
    }
    // §6：索引写入不是模型工具。
    assert!(
        !names.iter().any(|name| name.contains("scan") || name.contains("index")),
        "模型不得拥有索引写入工具: {names:?}"
    );
}

#[test]
fn search_returns_hits_and_ui_hint_document_list() {
    let (service, tools, _modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 挂载说明",
    ));
    let _ = index(&service, "/data/docs/notes/docker.md", "# Docker\nvolume 挂载说明");

    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.search".into(),
        arguments: serde_json::json!({"query": "docker"}),
    }))
    .unwrap();
    assert!(result.ok);
    // FakeDocumentIndex 与 SQLite 同语义：标题命中 + 正文命中各一条（§30）。
    assert_eq!(result.data["count"], 2);
    assert_eq!(result.data["items"][0]["document_type"], "markdown");
    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("ui_hint blocks");
    assert_eq!(blocks[0]["kind"], "document_list");
    let _ = UiBlockKind::DocumentList;
}

#[test]
fn search_no_hit_states_not_found() {
    let (_service, tools, _modules) = hub(FakeDocumentSource::default());
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.search".into(),
        arguments: serde_json::json!({"query": "量子计算"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], serde_json::Value::Null);
    assert_eq!(result.data["note"], "没有找到匹配的文档");
    assert!(
        result.metadata["ui_hint"].is_null() || result.metadata["ui_hint"]["ui_blocks"].is_null(),
        "未命中不得产出 ui_hint"
    );
}

#[test]
fn get_and_get_context_return_card_and_outline() {
    let (service, tools, _modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 挂载说明\n",
    ));
    let id = index(
        &service,
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 挂载说明\n",
    );

    let card = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.get".into(),
        arguments: serde_json::json!({"document_id": id}),
    }))
    .unwrap();
    assert!(card.ok);
    assert_eq!(card.data["title"], "docker.md");
    assert_eq!(card.data["content_available"], true);
    let blocks = card.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("card blocks");
    assert_eq!(blocks[0]["kind"], "document_card");
    let _ = UiBlockKind::DocumentCard;

    let context = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c2".into(),
        name: "documents.get_context".into(),
        arguments: serde_json::json!({"document_id": id, "max_chars": 6}),
    }))
    .unwrap();
    assert!(context.ok);
    // §31：get_context 只给章节标题 + 文首片段，绝不整份进 prompt。
    let head = context.data["head"].as_str().unwrap();
    assert_eq!(head.chars().count(), 6, "head 应按 max_chars 截断");
    assert_eq!(context.data["chunk_count"], 1);
}

#[test]
fn read_chunk_and_section_and_range() {
    let (service, tools, _modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 挂载说明\n## 网络\nhost 网络\n",
    ));
    let id = index(
        &service,
        "/data/docs/notes/docker.md",
        "# Docker\nvolume 挂载说明\n## 网络\nhost 网络\n",
    );

    let by_chunk = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.read".into(),
        arguments: serde_json::json!({"document_id": id, "chunk_id": chunk_id_for(&service, &id, 0)}),
    }))
    .unwrap();
    assert!(by_chunk.ok);
    assert!(by_chunk.data["text"].as_str().unwrap().contains("Docker"));
    let blocks = by_chunk.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("reference blocks");
    assert_eq!(blocks[0]["kind"], "document_reference");
    let _ = UiBlockKind::DocumentReference;

    // 章节读取：标题在原文中的位置定位（§37）。短文档是单 chunk，
    // `location.section` 只记录首个标题，按章节读取需长文档。
    let long = format!(
        "# 环境\n{}\n## 网络\n{}\n",
        "a".repeat(400),
        "b".repeat(3_000)
    );
    let (long_service, long_tools, _modules) =
        hub(FakeDocumentSource::default().with_file("/data/docs/notes/env.md", &long));
    let long_id = index(&long_service, "/data/docs/notes/env.md", &long);
    let by_section = block_on(long_tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c2".into(),
        name: "documents.read".into(),
        arguments: serde_json::json!({"document_id": long_id, "section": "网络"}),
    }))
    .unwrap();
    assert!(by_section.ok);
    assert!(
        by_section.data["text"]
            .as_str()
            .unwrap()
            .contains('b'),
        "应返回该章节的正文"
    );
    assert!(
        by_section.data["location"]
            .as_str()
            .is_some_and(|text| text.contains("§网络")),
        "读取区间必须归属正确章节: {:?}",
        by_section.data["location"]
    );

    let by_range = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c3".into(),
        name: "documents.read".into(),
        arguments: serde_json::json!({"document_id": id, "offset": 2, "max_chars": 3}),
    }))
    .unwrap();
    assert!(by_range.ok);
    assert_eq!(by_range.data["text"], "Doc");
    assert_eq!(by_range.data["truncated"], true);
}

#[test]
fn read_unknown_document_fails() {
    let (_service, tools, _modules) = hub(FakeDocumentSource::default());
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.read".into(),
        arguments: serde_json::json!({"document_id": "doc-missing"}),
    }));
    assert!(result.is_err(), "未知文档必须报错");
}

#[test]
fn list_recent_returns_document_list_block() {
    let (service, tools, _modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume",
    ));
    let _ = index(&service, "/data/docs/notes/docker.md", "# Docker\nvolume");

    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "documents.list_recent".into(),
        arguments: serde_json::json!({"limit": 5}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], 1);
    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("recent blocks");
    assert_eq!(blocks[0]["kind"], "document_list");
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

#[test]
fn context_provider_reports_overview_and_entity() {
    let (service, _tools, modules) = hub(FakeDocumentSource::default().with_file(
        "/data/docs/notes/docker.md",
        "# Docker\nvolume",
    ));
    let id = index(&service, "/data/docs/notes/docker.md", "# Docker\nvolume");
    let provider = modules
        .context_provider("documents")
        .expect("documents provider");

    let budget = ContextBudget::default();
    let overview = provider
        .build_context(&AppContext::default(), &budget)
        .expect("overview");
    assert_eq!(overview.module, "documents");
    assert_eq!(overview.summary["stats"]["documents"], 1);

    let entity = AppContext {
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "document".into(),
            id: id.clone(),
            label: None,
        }),
        ..AppContext::default()
    };
    let bundle = provider
        .build_context(&entity, &budget)
        .expect("entity context");
    assert_eq!(bundle.headline, "Documents · docker.md");
    assert_eq!(bundle.summary["document"]["title"], "docker.md");

    let wrong = AppContext {
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "file".into(),
            id: id,
            label: None,
        }),
        ..AppContext::default()
    };
    assert!(
        provider.build_context(&wrong, &budget).is_err(),
        "非 document entity 必须受控报错"
    );
}

// ---------------------------------------------------------------------------
// 读取工具语义（与 service 层复用同一配置）
// ---------------------------------------------------------------------------

#[test]
fn tools_read_settings_each_call() {
    // 组合根闭包每次调用读取设置 → 改变 max_read_chars 立即影响模块。
    let source = FakeDocumentSource::default().with_file("/data/docs/notes/docker.md", "0123456789");
    let service = Arc::new(DocumentService::with_config(
        Arc::new(FakeDocumentIndex::default()),
        Arc::new(source),
        DocumentConfig::default(),
    ));
    let settings = settings_closure();
    let tools = DocumentsTools::new(Arc::clone(&service), Arc::clone(&settings));
    // 索引（直接走 service，与模块工具无关）。
    service
        .index_configured_roots(&KnowledgeSettings {
            document_roots: vec![root()],
            ..KnowledgeSettings::default()
        })
        .expect("index");
    let id = devtoolbox_core::documents::document_id("docs", "notes/docker.md");

    let read = tools
        .read(&serde_json::json!({"document_id": id, "max_chars": 4}))
        .expect("read");
    assert_eq!(read.data["text"], "0123");
    assert_eq!(read.data["truncated"], true);

    // `get_context` 的 head 上限同理（默认 800，显式覆盖）。
    let context = tools
        .get_context(&serde_json::json!({"document_id": id, "max_chars": 2}))
        .expect("get_context");
    assert_eq!(context.data["head"], "01");

    // 分块函数与索引一致（outline 依赖同一配置）。
    let chunks = chunk_text(&id, "0123456789", &settings().chunk_config());
    assert_eq!(chunks.len(), 1);
}
