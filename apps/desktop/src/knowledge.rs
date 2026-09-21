//! Personal Knowledge 组合根（V6 Gates 2-6）。
//!
//! 这里做三件事，全部属于「装配」而非业务：
//! 1. 把 infrastructure 的三个 SQLite / 文件系统实现包装成 application 端口适配器；
//! 2. 构造三个域服务与统一检索服务（`KnowledgeRetrievalService`）；
//! 3. 提供启动轻量同步（§89）与设置读取器（每次调用读最新 `settings.json`）。
//!
//! 依赖方向：desktop 是唯一组合根，`application → infrastructure` 依赖依然为零。

use std::path::Path;
use std::sync::Arc;

use devtoolbox_application::documents::{
    DocumentIndexPort, DocumentService, DocumentSourcePort, DocumentStoreError, ExtractedContent,
    ScannedDocument,
};
use devtoolbox_application::files::{
    FileIndexError, FileIndexPort, FileService, FileSystemPort,
};
use devtoolbox_application::knowledge::{
    DocumentRetriever, FileRetriever, KnowledgeMetrics, KnowledgeRetrievalService, MemoryRetriever,
};
use devtoolbox_application::memory::{MemoryService, MemoryStoreError, MemoryStorePort};
use devtoolbox_core::documents::{
    DocumentChunk, DocumentFingerprint, DocumentHit, DocumentIndexStats, DocumentMeta, DocumentType,
};
use devtoolbox_core::files::{
    FileAccessDenied, FileFingerprint, FileIndexStats, FileMetadata, FileQuery, FileReadOutcome,
    KnowledgeRoot, RawFile,
};
use devtoolbox_core::knowledge::KnowledgeBudget;
use devtoolbox_core::memory::{MemoryCategory, MemoryItem, MemoryQuery, MemoryStatus};
use devtoolbox_core::settings::{AppSettings, KnowledgeSettings};
use devtoolbox_infrastructure::{
    DocumentIndexSqliteStore, FileIndexSqliteStore, LocalDocumentSource, LocalFileSystem,
    MemorySqliteStore,
};

/// 设置读取器（与 history enrichment / travel 同款：每次调用读最新 settings.json）。
pub type SettingsLoader = Arc<dyn Fn() -> Result<AppSettings, String> + Send + Sync>;

/// 知识层设置读取器（读失败 → 默认值 = 无允许根 = fail-closed）。
pub type KnowledgeSettingsLoader = Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>;

/// 由设置读取器派生「只取知识设置」的读取器。
#[must_use]
pub fn knowledge_settings_loader(loader: &SettingsLoader) -> KnowledgeSettingsLoader {
    let loader = Arc::clone(loader);
    Arc::new(move || {
        loader()
            .map(|settings| settings.knowledge)
            .unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// 适配器（infrastructure → application 端口）
// ---------------------------------------------------------------------------

/// `MemorySqliteStore` → `MemoryStorePort`。
pub struct MemoryStoreAdapter {
    store: MemorySqliteStore,
}

impl MemoryStoreAdapter {
    #[must_use]
    pub fn new(store: MemorySqliteStore) -> Self {
        Self { store }
    }
}

impl MemoryStorePort for MemoryStoreAdapter {
    fn upsert(&self, item: &MemoryItem) -> Result<(), MemoryStoreError> {
        self.store.upsert(item).map_err(err_text_memory)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryItem>, MemoryStoreError> {
        self.store.get(id).map_err(err_text_memory)
    }

    fn query(&self, spec: &MemoryQuery) -> Result<Vec<MemoryItem>, MemoryStoreError> {
        self.store.query(spec).map_err(err_text_memory)
    }

    fn touch_used(&self, ids: &[String], now: i64) -> Result<(), MemoryStoreError> {
        self.store.touch_used(ids, now).map_err(err_text_memory)
    }

    fn count_by_status(&self) -> Result<Vec<(MemoryStatus, usize)>, MemoryStoreError> {
        self.store.count_by_status().map_err(err_text_memory)
    }

    fn count_by_category(&self) -> Result<Vec<(MemoryCategory, usize)>, MemoryStoreError> {
        self.store.count_by_category().map_err(err_text_memory)
    }
}

/// `DocumentIndexSqliteStore` → `DocumentIndexPort`。
pub struct DocumentIndexAdapter {
    store: DocumentIndexSqliteStore,
}

impl DocumentIndexAdapter {
    #[must_use]
    pub fn new(store: DocumentIndexSqliteStore) -> Self {
        Self { store }
    }
}

impl DocumentIndexPort for DocumentIndexAdapter {
    fn upsert(&self, meta: &DocumentMeta) -> Result<(), DocumentStoreError> {
        self.store.upsert(meta).map_err(err_text_documents)
    }

    fn replace_chunks(
        &self,
        document_id: &str,
        chunks: &[DocumentChunk],
    ) -> Result<(), DocumentStoreError> {
        self.store
            .replace_chunks(document_id, chunks)
            .map_err(err_text_documents)
    }

    fn get(&self, document_id: &str) -> Result<Option<DocumentMeta>, DocumentStoreError> {
        self.store.get(document_id).map_err(err_text_documents)
    }

    fn chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>, DocumentStoreError> {
        self.store.chunks(document_id).map_err(err_text_documents)
    }


    fn search_candidates(
        &self,
        keywords: &[String],
        document_type: Option<DocumentType>,
        limit: usize,
    ) -> Result<Vec<DocumentHit>, DocumentStoreError> {
        self.store
            .search_candidates(keywords, document_type, limit)
            .map_err(err_text_documents)
    }

    fn recent(&self, limit: usize) -> Result<Vec<DocumentMeta>, DocumentStoreError> {
        self.store.recent(limit).map_err(err_text_documents)
    }

    fn fingerprints(&self, root_id: &str) -> Result<Vec<DocumentFingerprint>, DocumentStoreError> {
        self.store.fingerprints(root_id).map_err(err_text_documents)
    }

    fn remove(&self, document_id: &str) -> Result<(), DocumentStoreError> {
        self.store.remove(document_id).map_err(err_text_documents)
    }

    fn stats(&self) -> Result<DocumentIndexStats, DocumentStoreError> {
        self.store.stats().map_err(err_text_documents)
    }
}

/// `LocalDocumentSource` → `DocumentSourcePort`。
pub struct DocumentSourceAdapter;

impl DocumentSourcePort for DocumentSourceAdapter {
    fn scan_root(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<ScannedDocument>, bool), DocumentStoreError> {
        LocalDocumentSource
            .scan_root(root, limit)
            .map_err(err_text_documents_infra)
    }

    fn extract(
        &self,
        path: &Path,
        document_type: DocumentType,
        max_bytes: u64,
    ) -> Result<ExtractedContent, DocumentStoreError> {
        LocalDocumentSource
            .extract(path, document_type, max_bytes)
            .map_err(err_text_documents_infra)
    }
}

/// `FileIndexSqliteStore` → `FileIndexPort`。
pub struct FileIndexAdapter {
    store: FileIndexSqliteStore,
}

impl FileIndexAdapter {
    #[must_use]
    pub fn new(store: FileIndexSqliteStore) -> Self {
        Self { store }
    }
}

impl FileIndexPort for FileIndexAdapter {
    fn upsert_many(&self, entries: &[FileMetadata]) -> Result<(), FileIndexError> {
        self.store.upsert_many(entries).map_err(err_text_files)
    }

    fn get(&self, file_id: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        self.store.get(file_id).map_err(err_text_files)
    }

    fn find_by_path(&self, path: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        self.store.find_by_path(path).map_err(err_text_files)
    }

    fn search(&self, spec: &FileQuery) -> Result<Vec<FileMetadata>, FileIndexError> {
        self.store.search(spec).map_err(err_text_files)
    }

    fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, FileIndexError> {
        self.store.recent(limit).map_err(err_text_files)
    }

    fn fingerprints(&self, root_id: &str) -> Result<Vec<FileFingerprint>, FileIndexError> {
        self.store.fingerprints(root_id).map_err(err_text_files)
    }

    fn remove(&self, file_id: &str) -> Result<(), FileIndexError> {
        self.store.remove(file_id).map_err(err_text_files)
    }

    fn stats(&self) -> Result<FileIndexStats, FileIndexError> {
        self.store.stats().map_err(err_text_files)
    }
}

/// `LocalFileSystem` → `FileSystemPort`（错误类型已经是 core 的 `FileAccessDenied`）。
pub struct FileSystemAdapter;

impl FileSystemPort for FileSystemAdapter {
    fn canonicalize(&self, path: &str) -> Result<std::path::PathBuf, FileAccessDenied> {
        LocalFileSystem.canonicalize(path)
    }

    fn metadata(&self, path: &std::path::PathBuf) -> Result<RawFile, FileAccessDenied> {
        LocalFileSystem.metadata(path)
    }

    fn read_text(
        &self,
        path: &std::path::PathBuf,
        max_bytes: u64,
    ) -> Result<FileReadOutcome, FileAccessDenied> {
        LocalFileSystem.read_text(path, max_bytes)
    }

    fn walk(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<RawFile>, bool), FileAccessDenied> {
        LocalFileSystem.walk(root, limit)
    }
}

fn err_text_memory(error: devtoolbox_infrastructure::InfrastructureError) -> MemoryStoreError {
    MemoryStoreError(error.to_string())
}

fn err_text_documents(error: devtoolbox_infrastructure::DocumentIndexError) -> DocumentStoreError {
    DocumentStoreError(error.to_string())
}

/// infra 统一错误 → `DocumentStoreError`（`LocalDocumentSource` 走这条路径）。
fn err_text_documents_infra(
    error: devtoolbox_infrastructure::InfrastructureError,
) -> DocumentStoreError {
    DocumentStoreError(error.to_string())
}

fn err_text_files(error: devtoolbox_infrastructure::FileIndexError) -> FileIndexError {
    FileIndexError(error.to_string())
}

// ---------------------------------------------------------------------------
// 运行时
// ---------------------------------------------------------------------------

/// 知识层运行时（组合根装配，`AppState` 持有）。
pub struct KnowledgeRuntime {
    pub memory: Arc<MemoryService>,
    pub documents: Arc<DocumentService>,
    pub files: Arc<FileService>,
    pub retrieval: Arc<KnowledgeRetrievalService>,
    pub settings: KnowledgeSettingsLoader,
    pub metrics: Arc<KnowledgeMetrics>,
}

impl KnowledgeRuntime {
    /// 打开三个索引库并装配服务。数据库位于 `<config>/`（gitignored）。
    pub fn build(
        config_directory: &Path,
        settings_loader: SettingsLoader,
        budget: KnowledgeBudget,
    ) -> Result<Self, String> {
        let memory_store = MemorySqliteStore::open(config_directory.join("memory.db"))
            .map_err(|error| error.to_string())?;
        let document_store = DocumentIndexSqliteStore::open(config_directory.join("documents.db"))
            .map_err(|error| error.to_string())?;
        let file_store = FileIndexSqliteStore::open(config_directory.join("files.db"))
            .map_err(|error| error.to_string())?;

        let memory = Arc::new(MemoryService::new(Arc::new(MemoryStoreAdapter::new(
            memory_store,
        ))));
        let documents = Arc::new(DocumentService::new(
            Arc::new(DocumentIndexAdapter::new(document_store)),
            Arc::new(DocumentSourceAdapter),
        ));
        let files = Arc::new(FileService::new(
            Arc::new(FileSystemAdapter),
            Arc::new(FileIndexAdapter::new(file_store)),
        ));

        let settings = knowledge_settings_loader(&settings_loader);
        let metrics = Arc::new(KnowledgeMetrics::new());
        let retrievers: Vec<Arc<dyn devtoolbox_application::knowledge::KnowledgeSourceRetriever>> = vec![
            Arc::new(MemoryRetriever::new(Arc::clone(&memory))),
            Arc::new(DocumentRetriever::new(Arc::clone(&documents))),
            Arc::new(FileRetriever::new(
                Arc::clone(&files),
                Arc::clone(&settings),
            )),
        ];
        let retrieval = Arc::new(KnowledgeRetrievalService::with_metrics(
            retrievers,
            budget,
            Arc::clone(&metrics),
        ));

        Ok(Self {
            memory,
            documents,
            files,
            retrieval,
            settings,
            metrics,
        })
    }

    /// 当前知识设置（读失败 → 默认值）。
    #[must_use]
    pub fn settings(&self) -> KnowledgeSettings {
        (self.settings)()
    }

    /// 启动轻量同步（§89）：只扫描允许根；未配置 → 直接返回（不报错）。
    /// 文件系统不可用 / 单个根失败不影响启动。
    pub fn startup_sync(&self) -> Vec<String> {
        let settings = self.settings();
        if !settings.is_configured() {
            return Vec::new();
        }
        let mut notes: Vec<String> = Vec::new();
        let started = std::time::Instant::now();
        let mut document_reports = Vec::new();
        let mut file_reports = Vec::new();
        match self.documents.index_configured_roots(&settings) {
            Ok(reports) => {
                for report in &reports {
                    notes.push(format!(
                        "documents:{} scanned={} indexed={} unchanged={} failed={}",
                        report.root_id, report.scanned, report.indexed, report.unchanged,
                        report.failed
                    ));
                }
                document_reports = reports;
            }
            Err(error) => notes.push(format!("documents 索引不可用: {error}")),
        }
        match self.files.index_configured_roots(&settings) {
            Ok(reports) => {
                for report in &reports {
                    notes.push(format!(
                        "files:{} scanned={} indexed={} unchanged={}",
                        report.root_id, report.scanned, report.indexed, report.unchanged
                    ));
                }
                file_reports = reports;
            }
            Err(error) => notes.push(format!("files 索引不可用: {error}")),
        }
        self.metrics.record_index(
            &document_reports,
            &file_reports,
            started.elapsed().as_millis() as u64,
        );
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::memory::{MemoryDraft, MemoryStatus};
    use devtoolbox_core::settings::KnowledgeSettings;

    fn runtime(directory: &Path, settings: KnowledgeSettings) -> KnowledgeRuntime {
        let settings_for_loader = settings.clone();
        let loader: SettingsLoader = Arc::new(move || {
            Ok(AppSettings {
                knowledge: settings_for_loader.clone(),
                ..AppSettings::default()
            })
        });
        KnowledgeRuntime::build(directory, loader, KnowledgeBudget::default()).expect("build")
    }

    #[test]
    fn runtime_builds_three_isolated_stores() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = runtime(directory.path(), KnowledgeSettings::default());
        // 三个独立库文件（§40 物理隔离）。
        assert!(directory.path().join("memory.db").is_file());
        assert!(directory.path().join("documents.db").is_file());
        assert!(directory.path().join("files.db").is_file());
        assert_eq!(runtime.retrieval.kinds().len(), 3);
        assert!(runtime.startup_sync().is_empty(), "未配置允许根 → 不同步");
    }

    #[test]
    fn memory_service_persists_across_runtime_rebuild() {
        let directory = tempfile::tempdir().unwrap();
        let draft = MemoryDraft::new(
            devtoolbox_core::memory::MemoryCategory::Environment,
            "Docker 数据目录是 /Volumes/Data/docker",
        );
        let id = {
            let runtime = runtime(directory.path(), KnowledgeSettings::default());
            let item = runtime
                .memory
                .save_confirmed(draft.clone())
                .expect("save confirmed");
            assert_eq!(item.status, MemoryStatus::Active);
            item.id
        };
        let runtime = runtime(directory.path(), KnowledgeSettings::default());
        let loaded = runtime.memory.get(&id, true).expect("reopen");
        assert_eq!(loaded.content, draft.content);
    }

    #[test]
    fn startup_sync_indexes_configured_roots_and_reports_counts() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("docs");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("docker.md"), "# Docker\nvolume 配置").unwrap();
        std::fs::write(root.join("notes.txt"), "普通文本").unwrap();

        let settings = KnowledgeSettings {
            file_roots: vec![KnowledgeRoot::new(
                "docs",
                "资料",
                root.to_string_lossy().to_string(),
            )],
            ..KnowledgeSettings::default()
        };
        let runtime = runtime(directory.path(), settings);
        let notes = runtime.startup_sync();
        assert!(notes.iter().any(|note| note.starts_with("documents:docs")));
        assert!(notes.iter().any(|note| note.starts_with("files:docs")));

        let stats = runtime.documents.stats().expect("document stats");
        assert_eq!(stats.documents, 2);
        assert!(stats.content_available >= 2);

        let file_stats = runtime.files.stats().expect("file stats");
        assert_eq!(file_stats.files, 2);

        // 第二次同步：增量（全部 unchanged）。
        let notes = runtime.startup_sync();
        assert!(notes.iter().any(|note| note.contains("unchanged=2")));

        // 文档可检索（关键词命中）。
        let hits = runtime
            .documents
            .search("volume", None, 5)
            .expect("search documents");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("volume"));
    }

    #[test]
    fn startup_sync_degrades_when_roots_missing() {
        let directory = tempfile::tempdir().unwrap();
        let settings = KnowledgeSettings {
            file_roots: vec![KnowledgeRoot::new("ghost", "不存在", "/definitely/missing")],
            ..KnowledgeSettings::default()
        };
        let runtime = runtime(directory.path(), settings);
        // 不 panic；报告 scanned=0。
        let notes = runtime.startup_sync();
        assert!(notes.iter().any(|note| note.contains("scanned=0")));
    }
}
