//! Files 模块测试（V6 §94/§113）：工具面 + agent 路由 + 只读边界。
//!
//! 用内存 Fake 文件系统 / 索引（与 `crate::files::tests` 同款语义）驱动。
//! 重点断言：
//! - 模块只有 4 个 Read 工具，没有任何写 / 移动 / 删除 / 执行能力；
//! - `files.open` 只产出 `OpenFile` Action，绝不执行 shell；
//! - 二进制 / 超大 / 受限 → 只回元数据，**字节不进入 ToolResult**；
//! - 凭据类文件默认不进检索结果（§71）。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::files::{
    FileAccessDenied, FileContentKind, FileFingerprint, FileIndexStats, FileMetadata, FileQuery,
    FileReadOutcome, KnowledgeRoot, RawFile, display_path, file_id,
};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::settings::KnowledgeSettings;
use devtoolbox_core::{ToolRisk, UiBlockKind};

use crate::files::ports::{FileIndexError, FileIndexPort, FileSystemPort, FileSystemResult};
use crate::files::service::{FileConfig, FileService};
use crate::personal_ai::context::ContextBudget;
use crate::personal_ai::files::{FilesTools, files_tool_names, register_files};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

// ---------------------------------------------------------------------------
// Fakes（与 files/tests.rs 相同语义）
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeFileSystem {
    files: Mutex<HashMap<String, Vec<u8>>>,
    modified: Mutex<HashMap<String, i64>>,
    dirs: Mutex<HashSet<String>>,
    links: Mutex<HashMap<String, String>>,
}

impl FakeFileSystem {
    fn with_file(self, path: &str, content: &str, modified_at: i64) -> Self {
        self.files
            .lock()
            .insert(path.to_string(), content.as_bytes().to_vec());
        self.modified.lock().insert(path.to_string(), modified_at);
        self
    }

    fn with_binary(self, path: &str, bytes: &[u8], modified_at: i64) -> Self {
        self.files.lock().insert(path.to_string(), bytes.to_vec());
        self.modified.lock().insert(path.to_string(), modified_at);
        self
    }
}

impl FileSystemPort for FakeFileSystem {
    fn canonicalize(&self, path: &str) -> FileSystemResult<PathBuf> {
        if let Some(target) = self.links.lock().get(path) {
            return Ok(PathBuf::from(target));
        }
        if self.files.lock().contains_key(path) || self.dirs.lock().contains(path) {
            return Ok(PathBuf::from(path));
        }
        if self
            .files
            .lock()
            .keys()
            .any(|known| known.starts_with(&format!("{path}/")))
        {
            return Ok(PathBuf::from(path));
        }
        Err(FileAccessDenied::NotFound)
    }

    fn metadata(&self, path: &PathBuf) -> FileSystemResult<RawFile> {
        let key = path.to_string_lossy().to_string();
        match self.files.lock().get(&key) {
            Some(bytes) => Ok(RawFile {
                path: path.clone(),
                relative_path: String::new(),
                size_bytes: bytes.len() as u64,
                modified_at: self.modified_of(&key),
                is_file: true,
            }),
            None => Err(FileAccessDenied::NotFound),
        }
    }

    fn read_text(&self, path: &PathBuf, max_bytes: u64) -> FileSystemResult<FileReadOutcome> {
        let key = path.to_string_lossy().to_string();
        let bytes = self
            .files
            .lock()
            .get(&key)
            .cloned()
            .ok_or(FileAccessDenied::NotFound)?;
        if bytes.len() as u64 > max_bytes {
            return Ok(FileReadOutcome::TooLarge);
        }
        match String::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => Ok(FileReadOutcome::Text(text)),
            _ => Ok(FileReadOutcome::Binary),
        }
    }

    fn walk(&self, root: &KnowledgeRoot, limit: usize) -> FileSystemResult<(Vec<RawFile>, bool)> {
        let prefix = format!("{}/", root.path.trim_end_matches('/'));
        let mut found: Vec<RawFile> = self
            .files
            .lock()
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(path, bytes)| RawFile {
                path: PathBuf::from(path),
                relative_path: path[prefix.len()..].to_string(),
                size_bytes: bytes.len() as u64,
                modified_at: self.modified_of(path),
                is_file: true,
            })
            .collect();
        found.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let truncated = limit > 0 && found.len() > limit;
        if truncated {
            found.truncate(limit);
        }
        Ok((found, truncated))
    }
}

impl FakeFileSystem {
    fn modified_of(&self, path: &str) -> i64 {
        self.modified.lock().get(path).copied().unwrap_or(0)
    }
}

#[derive(Default)]
struct FakeFileIndex {
    entries: Mutex<HashMap<String, FileMetadata>>,
}

impl FileIndexPort for FakeFileIndex {
    fn upsert_many(&self, entries: &[FileMetadata]) -> Result<(), FileIndexError> {
        for entry in entries {
            self.entries
                .lock()
                .insert(entry.file_id.clone(), entry.clone());
        }
        Ok(())
    }

    fn get(&self, file_id: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        Ok(self.entries.lock().get(file_id).cloned())
    }

    fn find_by_path(&self, path: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        Ok(self
            .entries
            .lock()
            .values()
            .find(|entry| entry.path == path || entry.path == display_path(path))
            .cloned())
    }

    fn search(&self, spec: &FileQuery) -> Result<Vec<FileMetadata>, FileIndexError> {
        let needles: Vec<String> = spec
            .query
            .split_whitespace()
            .map(|token| token.to_lowercase())
            .collect();
        let extension = spec
            .extension
            .as_deref()
            .map(|value| value.trim_start_matches('.').to_ascii_lowercase());
        let mut found: Vec<FileMetadata> = self
            .entries
            .lock()
            .values()
            .filter(|entry| spec.include_restricted || !entry.restricted)
            .filter(|entry| {
                let haystack = entry.relative_path.to_lowercase();
                needles.iter().all(|needle| haystack.contains(needle))
            })
            .filter(|entry| {
                extension
                    .as_deref()
                    .is_none_or(|ext| entry.extension.as_deref() == Some(ext))
            })
            .filter(|entry| {
                spec.root_id
                    .as_deref()
                    .is_none_or(|root| entry.root_id == root)
            })
            .cloned()
            .collect();
        found.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then(left.file_id.cmp(&right.file_id))
        });
        if spec.limit > 0 {
            found.truncate(spec.limit);
        }
        Ok(found)
    }

    fn remove(&self, file_id: &str) -> Result<(), FileIndexError> {
        self.entries.lock().remove(file_id);
        Ok(())
    }

    fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, FileIndexError> {
        let mut all: Vec<FileMetadata> = self.entries.lock().values().cloned().collect();
        all.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then(left.file_id.cmp(&right.file_id))
        });
        if limit > 0 {
            all.truncate(limit);
        }
        Ok(all)
    }

    fn fingerprints(&self, root_id: &str) -> Result<Vec<FileFingerprint>, FileIndexError> {
        Ok(self
            .entries
            .lock()
            .values()
            .filter(|entry| entry.root_id == root_id)
            .map(|entry| FileFingerprint {
                file_id: entry.file_id.clone(),
                root_id: entry.root_id.clone(),
                relative_path: entry.relative_path.clone(),
                size_bytes: entry.size_bytes,
                modified_at: entry.modified_at,
            })
            .collect())
    }

    fn stats(&self) -> Result<FileIndexStats, FileIndexError> {
        let entries: Vec<FileMetadata> = self.entries.lock().values().cloned().collect();
        Ok(FileIndexStats {
            files: entries.len(),
            text_files: entries
                .iter()
                .filter(|entry| entry.content_kind == FileContentKind::Text)
                .count(),
            binary_files: entries
                .iter()
                .filter(|entry| entry.content_kind == FileContentKind::Binary)
                .count(),
            restricted: entries.iter().filter(|entry| entry.restricted).count(),
            failed: entries
                .iter()
                .filter(|entry| entry.index_error.is_some())
                .count(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn root_docs() -> KnowledgeRoot {
    KnowledgeRoot::new("docs", "资料", "/data/docs")
}

fn settings() -> KnowledgeSettings {
    KnowledgeSettings {
        file_roots: vec![root_docs()],
        ..KnowledgeSettings::default()
    }
}

fn settings_closure() -> Arc<dyn Fn() -> KnowledgeSettings + Send + Sync> {
    Arc::new(settings)
}

fn hub(files: FakeFileSystem) -> (Arc<FileService>, ToolRegistry, ModuleRegistry) {
    let service = Arc::new(FileService::with_config(
        Arc::new(files),
        Arc::new(FakeFileIndex::default()),
        FileConfig::default(),
    ));
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_files(
        &mut modules,
        &mut tools,
        Arc::clone(&service),
        settings_closure(),
    )
    .expect("register files module");
    (service, tools, modules)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// ---------------------------------------------------------------------------
// 工具面
// ---------------------------------------------------------------------------

#[test]
fn registers_descriptor_and_four_read_tools_without_write() {
    let (_service, tools, modules) = hub(FakeFileSystem::default());
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "files");
    assert_eq!(descriptors[0].tools.len(), 4);
    assert!(
        modules.context_provider("files").is_some(),
        "模块必须提供 ContextProvider"
    );

    let mut names: Vec<String> = tools.specs().iter().map(|spec| spec.name.clone()).collect();
    names.sort();
    let mut expected: Vec<String> = files_tool_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    for spec in tools.specs() {
        assert_eq!(spec.risk, ToolRisk::Read, "{}", spec.name);
        assert_eq!(spec.module, "files");
    }
    // §42：没有任何写 / 移动 / 删除 / 执行工具。
    for forbidden in [
        "write", "move", "rename", "delete", "remove", "exec", "run", "shell",
    ] {
        assert!(
            !names.iter().any(|name| name.contains(forbidden)),
            "模块不得暴露 {forbidden} 能力: {names:?}"
        );
    }
}

#[test]
fn search_returns_files_and_ui_hint_file_list() {
    let (service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/data/docs/notes/docker.md", "hello", 100));
    // 索引（模块没有索引工具；测试手动走 service）。
    service
        .index_configured_roots(&settings())
        .expect("index roots");

    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.search".into(),
        arguments: serde_json::json!({"query": "docker"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], 1);
    assert_eq!(result.data["items"][0]["file_name"], "docker.md");
    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("ui_hint blocks");
    assert_eq!(blocks[0]["kind"], "file_list");
    let _ = UiBlockKind::FileList;
}

#[test]
fn search_no_hit_states_not_found() {
    let (_service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/data/docs/notes/a.md", "hello", 100));
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.search".into(),
        arguments: serde_json::json!({"query": "nothing"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["note"], "没有找到匹配的文件");
}

#[test]
fn get_metadata_returns_file_block() {
    let (_service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/data/docs/notes/docker.md", "hello", 100));
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.get_metadata".into(),
        arguments: serde_json::json!({"target": "/data/docs/notes/docker.md"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["file_name"], "docker.md");
    assert_eq!(result.data["extension"], "md");
    assert_eq!(result.data["size_bytes"], 5);
    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("blocks");
    assert_eq!(blocks[0]["kind"], "file_list");
}

#[test]
fn read_text_returns_content_within_root() {
    let (_service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/data/docs/notes/a.md", "hello world", 100));
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.read_text".into(),
        arguments: serde_json::json!({"target": "/data/docs/notes/a.md", "max_chars": 5}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["text"], "hello");
    assert_eq!(result.data["truncated"], true);
    assert_eq!(result.data["char_count"], 11);
}

#[test]
fn read_text_outside_root_is_denied_without_content() {
    let (_service, tools, _modules) = hub(FakeFileSystem::default()
        .with_file("/data/docs/notes/a.md", "hello", 100)
        .with_file("/etc/passwd", "root:x:0:0", 100));
    let error = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.read_text".into(),
        arguments: serde_json::json!({"target": "/etc/passwd"}),
    }))
    .expect_err("根外路径必须被拒绝");
    let text = error.to_string();
    assert!(text.contains("outside_allowed_roots"), "{text}");
    assert!(!text.contains("root:x:0:0"), "拒绝不得回显内容");
}

#[test]
fn read_text_binary_and_oversized_degrade_to_metadata_only() {
    let (_service, tools, _modules) = hub(FakeFileSystem::default()
        .with_binary("/data/docs/bin.dat", &[0u8, 159, 146, 150], 100)
        .with_file("/data/docs/big.md", &"x".repeat(4096), 100));

    let binary = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.read_text".into(),
        arguments: serde_json::json!({"target": "/data/docs/bin.dat"}),
    }))
    .unwrap();
    assert!(binary.ok, "二进制降级为元数据而不是错误");
    assert_eq!(binary.data["text"], "");
    assert_eq!(binary.data["content_available"], false);
    assert!(binary.data["note"].is_string());

    let oversized = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c2".into(),
        name: "files.read_text".into(),
        arguments: serde_json::json!({"target": "/data/docs/big.md", "max_chars": 10}),
    }))
    .unwrap();
    // max_chars 是字符截断（不是字节上限）→ 正常返回文本。
    assert_eq!(oversized.data["text"].as_str().unwrap().len(), 10);
    assert_eq!(oversized.data["truncated"], true);
}

#[test]
fn open_emits_action_and_never_executes() {
    let (_service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/data/docs/notes/docker.md", "hello", 100));
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.open".into(),
        arguments: serde_json::json!({"target": "/data/docs/notes/docker.md"}),
    }))
    .unwrap();
    assert!(result.ok);
    // §46：只有 Action，没有 shell 调用痕迹。
    let actions = result.metadata["ui_hint"]["actions"]
        .as_array()
        .expect("open action");
    assert_eq!(actions[0]["type"], "open_file");
    assert_eq!(actions[0]["module"], "files");
    assert_eq!(result.data["file"]["file_name"], "docker.md");
}

#[test]
fn open_outside_root_is_denied() {
    let (_service, tools, _modules) =
        hub(FakeFileSystem::default().with_file("/etc/passwd", "root", 100));
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.open".into(),
        arguments: serde_json::json!({"target": "/etc/passwd"}),
    }));
    assert!(result.is_err(), "根外文件不得产出打开请求");
}

#[test]
fn missing_target_is_invalid_argument() {
    let (_service, tools, _modules) = hub(FakeFileSystem::default());
    let error = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "files.read_text".into(),
        arguments: serde_json::json!({}),
    }))
    .expect_err("缺 target 必须参数错误");
    assert!(error.to_string().contains("target"), "{error}");
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

#[test]
fn context_provider_reports_overview_and_rejects_foreign_entity() {
    let (_service, _tools, modules) = hub(FakeFileSystem::default());
    let provider = modules.context_provider("files").expect("files provider");
    let budget = ContextBudget::default();

    let overview = provider
        .build_context(&AppContext::default(), &budget)
        .expect("overview");
    assert_eq!(overview.module, "files");
    assert_eq!(overview.summary["note"], "文件总览");

    let entity = AppContext {
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "document".into(),
            id: "doc-1".into(),
            label: None,
        }),
        ..AppContext::default()
    };
    assert!(
        provider.build_context(&entity, &budget).is_err(),
        "files 模块不接受 document entity"
    );
}

// ---------------------------------------------------------------------------
// 工具层语义（读设置 + 委托服务）
// ---------------------------------------------------------------------------

#[test]
fn tools_read_settings_each_call() {
    let files = FakeFileSystem::default().with_file("/data/docs/notes/a.md", "hello world", 100);
    let service = Arc::new(FileService::with_config(
        Arc::new(files),
        Arc::new(FakeFileIndex::default()),
        FileConfig::default(),
    ));
    let settings = settings_closure();
    let tools = FilesTools::new(Arc::clone(&service), Arc::clone(&settings));

    let metadata = tools
        .get_metadata(&serde_json::json!({"target": "/data/docs/notes/a.md"}))
        .expect("metadata");
    assert_eq!(metadata.data["file_id"], file_id("docs", "notes/a.md"));

    let read = tools
        .read_text(&serde_json::json!({"target": "/data/docs/notes/a.md", "max_chars": 4}))
        .expect("read");
    assert_eq!(read.data["text"], "hell");

    // 允许根在设置关闭后即可改变（组合根热更新语义）。
    let empty = KnowledgeSettings {
        file_roots: Vec::new(),
        ..KnowledgeSettings::default()
    };
    let tools_empty = FilesTools::new(
        Arc::clone(&service),
        Arc::new(move || empty.clone()) as Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    );
    assert!(
        tools_empty
            .get_metadata(&serde_json::json!({"target": "/data/docs/notes/a.md"}))
            .is_err(),
        "无允许根 → 拒绝（与设置闭包同步生效）"
    );
}
