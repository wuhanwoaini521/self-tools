//! Files 域测试（V6 §97）：授权（允许根 / traversal / symlink / deny）、安全读取降级、
//! open Action、检索（索引 + 实时扫描兜底）、增量索引。
//!
//! 全部用内存 Fake 端口（`FileSystemPort` / `FileIndexPort`）驱动，不触真实文件系统
//! 与 SQLite：路径规范化、权限与扫描语义由 infrastructure 的适配器测试覆盖。
//! Fake 的 `canonicalize` 只认已登记的文件 / 目录 / symlink 映射，其余一律 `NotFound`
//! ——与真实实现「不存在即拒绝」一致（§44）。

use super::*;
use crate::error::ApplicationError;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::files::{
    FileAccessDenied, FileContentKind, FileMetadata, KnowledgeRoot, display_path, file_id,
};
use devtoolbox_core::personal_ai::{Action, ActionKind};
use devtoolbox_core::settings::KnowledgeSettings;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 内存文件系统：`canonicalize` 只解析已登记的路径，symlink 用 `links` 模拟。
#[derive(Default)]
struct FakeFileSystem {
    /// 规范路径 → 内容。
    files: Mutex<HashMap<String, Vec<u8>>>,
    /// 规范路径 → 修改时间。
    modified: Mutex<HashMap<String, i64>>,
    /// 目录（`metadata` 返回 `is_file = false`）。
    dirs: Mutex<HashSet<String>>,
    /// 输入路径 → 规范路径。
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

    fn with_dir(self, path: &str) -> Self {
        self.dirs.lock().insert(path.to_string());
        self
    }

    fn with_link(self, input: &str, canonical: &str) -> Self {
        self.links
            .lock()
            .insert(input.to_string(), canonical.to_string());
        self
    }

    fn modified_of(&self, path: &str) -> i64 {
        self.modified.lock().get(path).copied().unwrap_or(0)
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
        // 目录（含允许根本身）解析为自身，与 `std::fs::canonicalize` 一致。
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
        if self.dirs.lock().contains(&key) {
            return Ok(RawFile {
                path: path.clone(),
                relative_path: String::new(),
                size_bytes: 0,
                modified_at: 0,
                is_file: false,
            });
        }
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

/// 内存索引：语义与 `FileIndexSqliteStore` 一致（关键词 AND + 过滤 + 受限默认隐藏）。
#[derive(Default)]
struct FakeFileIndex {
    entries: Mutex<HashMap<String, FileMetadata>>,
    removed: Mutex<Vec<String>>,
}

impl FakeFileIndex {
    fn all(&self) -> Vec<FileMetadata> {
        self.entries.lock().values().cloned().collect()
    }
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
            .all()
            .into_iter()
            .find(|entry| entry.path == path || entry.path == display_path(path)))
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
            .all()
            .into_iter()
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
            .filter(|entry| {
                spec.modified_after
                    .is_none_or(|threshold| entry.modified_at >= threshold)
            })
            .collect();
        found.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then(left.relative_path.cmp(&right.relative_path))
        });
        if spec.limit > 0 {
            found.truncate(spec.limit);
        }
        Ok(found)
    }

    fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, FileIndexError> {
        let mut found = self.all();
        found.sort_by(|left, right| {
            right
                .indexed_at
                .cmp(&left.indexed_at)
                .then(left.relative_path.cmp(&right.relative_path))
        });
        found.truncate(limit);
        Ok(found)
    }

    fn fingerprints(&self, root_id: &str) -> Result<Vec<FileFingerprint>, FileIndexError> {
        Ok(self
            .all()
            .into_iter()
            .filter(|entry| entry.root_id == root_id)
            .map(|entry| FileFingerprint {
                file_id: entry.file_id,
                root_id: entry.root_id,
                relative_path: entry.relative_path,
                size_bytes: entry.size_bytes,
                modified_at: entry.modified_at,
            })
            .collect())
    }

    fn remove(&self, file_id: &str) -> Result<(), FileIndexError> {
        self.entries.lock().remove(file_id);
        self.removed.lock().push(file_id.to_string());
        Ok(())
    }

    fn stats(&self) -> Result<FileIndexStats, FileIndexError> {
        let entries = self.all();
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

fn root_other() -> KnowledgeRoot {
    KnowledgeRoot::new("other", "其它", "/data/other")
}

fn settings() -> KnowledgeSettings {
    KnowledgeSettings {
        file_roots: vec![root_docs()],
        ..KnowledgeSettings::default()
    }
}

fn two_root_settings() -> KnowledgeSettings {
    KnowledgeSettings {
        file_roots: vec![root_docs(), root_other()],
        ..KnowledgeSettings::default()
    }
}

fn service(files: FakeFileSystem) -> (FileService, Arc<FakeFileIndex>) {
    service_with(files, FileConfig::default())
}

fn service_with(files: FakeFileSystem, config: FileConfig) -> (FileService, Arc<FakeFileIndex>) {
    let index = Arc::new(FakeFileIndex::default());
    let service = FileService::with_config(Arc::new(files), index.clone(), config);
    (service, index)
}

fn query(text: &str) -> FileQuery {
    FileQuery {
        query: text.to_string(),
        limit: 10,
        ..FileQuery::default()
    }
}

/// 取 `ApplicationError::Files` 的稳定拒绝原因（§44 受控错误）。
fn denial(error: &ApplicationError) -> (FileAccessDenied, String) {
    match error {
        ApplicationError::Files {
            reason,
            reason_text,
            message,
        } => (*reason, format!("{reason_text}: {message}")),
        other => panic!("期望 Files 拒绝错误，实际 {other:?}"),
    }
}

#[test]
fn open_action_denies_restricted_files() {
    // §4.3/§46：deny 规则对「打开」同样生效（否则 open_file Action 会旁路保护）。
    let (service, _index) = service(FakeFileSystem::default().with_file(
        "/data/docs/notes/.env",
        "API_KEY=sk-abcdefghijklmnopqrstuvwx",
        100,
    ));
    let error = service
        .open_action(&settings(), "/data/docs/notes/.env")
        .expect_err("受限文件不得产出打开请求");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::DeniedPattern);
    assert!(text.contains("denied_pattern"), "{text}");
}

#[test]
fn recent_hides_restricted_files() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/data/docs/notes/.env", "API_KEY=x", 200),
    );
    service.index_configured_roots(&settings()).expect("index");
    let recent = service.recent(10).expect("recent");
    assert!(
        recent.iter().all(|entry| !entry.restricted),
        "recent 默认不返回受限文件"
    );
    assert_eq!(recent.len(), 1);
}

// ---------------------------------------------------------------------------
// 安全读取（§42/§44/§47）
// ---------------------------------------------------------------------------

#[test]
fn read_text_within_root_and_truncates() {
    let (service, _index) = service(
        FakeFileSystem::default().with_file("/data/docs/notes/a.md", "hello world", 100),
    );
    let settings = settings();

    let result = service
        .read_text(&settings, "/data/docs/notes/a.md", 0)
        .expect("read text");
    assert_eq!(result.text, "hello world");
    assert!(!result.truncated);
    assert_eq!(result.char_count, 11);
    assert_eq!(result.char_count(), 11);
    assert_eq!(result.file.file_id, file_id("docs", "notes/a.md"));
    assert_eq!(result.file.root_id, "docs");
    assert_eq!(result.file.relative_path, "notes/a.md");
    assert_eq!(result.file.file_name, "a.md");
    assert_eq!(result.file.extension.as_deref(), Some("md"));
    assert_eq!(result.file.content_kind, FileContentKind::Text);
    assert!(!result.file.restricted);
    assert_eq!(result.file.path, display_path("/data/docs/notes/a.md"));
    assert!(!result.file.path.is_empty());

    let capped = service
        .read_text(&settings, "/data/docs/notes/a.md", 5)
        .expect("read capped");
    assert_eq!(capped.text, "hello");
    assert!(capped.truncated, "超过 max_chars → truncated");
    assert_eq!(capped.char_count, 11, "char_count 是原文长度");
}

#[test]
fn path_outside_root_denied() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/etc/passwd", "root:x:0:0", 100),
    );

    let error = service
        .metadata(&settings(), "/etc/passwd")
        .expect_err("根外路径必须被拒绝");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::OutsideAllowedRoots);
    assert!(text.contains("outside_allowed_roots"), "{text}");
    assert!(!text.contains("root:x:0:0"), "拒绝不得回显内容");
}

#[test]
fn directory_target_denied() {
    let (service, _index) = service(
        FakeFileSystem::default().with_file("/data/docs/notes/a.md", "notes", 100),
    );

    let error = service
        .read_text(&settings(), "/data/docs/../../etc/passwd", 0)
        .expect_err("`..` 必须被拒绝");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::Traversal);
    assert!(text.contains("traversal"), "{text}");
}

#[test]
fn symlink_escape_denied() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/home/user/.ssh/id_rsa", "PRIVATE KEY", 100)
            .with_link("/data/docs/ssh-link", "/home/user/.ssh/id_rsa"),
    );

    let error = service
        .metadata(&settings(), "/data/docs/ssh-link")
        .expect_err("symlink 解析后逃逸出允许根");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::OutsideAllowedRoots);
    assert!(!text.contains("PRIVATE KEY"), "拒绝不得回显内容: {text}");
}

#[test]
fn deny规则命中时元数据受限且永不返回内容() {
    let (service, _index) = service(
        FakeFileSystem::default().with_file("/data/docs/config/.env", "SECRET_TOKEN=abc", 100),
    );
    let settings = settings();

    let metadata = service
        .metadata(&settings, "/data/docs/config/.env")
        .expect("元数据仍可取");
    assert!(metadata.restricted, "deny 规则命中的文件必须标记受限");
    assert_eq!(metadata.content_kind, FileContentKind::Unknown);
    assert_eq!(metadata.file_name, ".env");

    let error = service
        .read_text(&settings, "/data/docs/config/.env", 0)
        .expect_err("受限文件不可读");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::DeniedPattern);
    assert!(text.contains("denied_pattern"), "{text}");
    assert!(!text.contains("SECRET_TOKEN"), "拒绝不得回显内容: {text}");
}

#[test]
fn binary_and_oversized_denied_but_metadata() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_binary("/data/docs/notes/blob.dat", &[0x00, 0xff, 0x41], 100)
            .with_file("/data/docs/notes/ten.txt", "0123456789", 100),
    );
    let settings = settings();

    let error = service
        .read_text(&settings, "/data/docs/notes/blob.dat", 0)
        .expect_err("二进制不可读");
    let (reason, _) = denial(&error);
    assert_eq!(reason, FileAccessDenied::NotText);
    let metadata = service
        .metadata(&settings, "/data/docs/notes/blob.dat")
        .expect("二进制文件仍可取元数据（§47 降级）");
    assert_eq!(metadata.content_kind, FileContentKind::Binary);
    assert_eq!(metadata.size_bytes, 3);

    // 读取上限收紧 → TooLarge，但元数据路径不受影响。
    let (tight, _index) = service_with(
        FakeFileSystem::default().with_file("/data/docs/notes/ten.txt", "0123456789", 100),
        FileConfig {
            max_read_bytes: 4,
            ..FileConfig::default()
        },
    );
    let error = tight
        .read_text(&settings, "/data/docs/notes/ten.txt", 0)
        .expect_err("超过读取上限 → TooLarge");
    let (reason, _) = denial(&error);
    assert_eq!(reason, FileAccessDenied::TooLarge);
    let metadata = tight
        .metadata(&settings, "/data/docs/notes/ten.txt")
        .expect("超大文件仍可取元数据（§47 降级）");
    assert_eq!(metadata.size_bytes, 10);
    assert!(!metadata.restricted);
}

#[test]
fn missing_and_directory_targets_denied() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_dir("/data/docs/subdir"),
    );
    let settings = settings();

    let error = service
        .metadata(&settings, "/data/docs/notes/missing.md")
        .expect_err("不存在的文件");
    assert_eq!(denial(&error).0, FileAccessDenied::NotFound);

    let error = service
        .metadata(&settings, "/data/docs/subdir")
        .expect_err("目录不是普通文件");
    assert_eq!(denial(&error).0, FileAccessDenied::NotAFile);
}

#[test]
fn no_configured_roots_denies_all() {
    let (service, _index) = service(
        FakeFileSystem::default().with_file("/data/docs/notes/a.md", "notes", 100),
    );
    let empty = KnowledgeSettings::default();
    assert!(!empty.is_configured());

    let error = service
        .metadata(&empty, "/data/docs/notes/a.md")
        .expect_err("未配置允许根");
    let (reason, text) = denial(&error);
    assert_eq!(reason, FileAccessDenied::NoRootsConfigured);
    assert!(text.contains("no_roots_configured"), "{text}");

    let error = service.search(&empty, &query("")).expect_err("检索也必须拒绝");
    assert_eq!(denial(&error).0, FileAccessDenied::NoRootsConfigured);
    let error = service
        .index_configured_roots(&empty)
        .expect_err("索引也必须拒绝");
    assert_eq!(denial(&error).0, FileAccessDenied::NoRootsConfigured);
}

// ---------------------------------------------------------------------------
// open Action（§46）
// ---------------------------------------------------------------------------

#[test]
fn open_action_产出_open_file_动作() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/etc/passwd", "root:x:0:0", 100),
    );

    let (metadata, action) = service
        .open_action(&settings(), "/data/docs/notes/a.md")
        .expect("open action");
    assert_eq!(
        action,
        Action::open_file(serde_json::json!({
            "file_id": metadata.file_id,
            "path": metadata.path,
            "file_name": metadata.file_name,
            "root_id": metadata.root_id,
        }))
    );
    assert_eq!(action.kind, ActionKind::OpenFile);
    assert_eq!(action.module, "files");
    assert_eq!(action.target["file_id"].as_str(), Some(metadata.file_id.as_str()));
    assert_eq!(action.target["path"].as_str(), Some(metadata.path.as_str()));
    assert_eq!(action.target["file_name"].as_str(), Some("a.md"));
    assert_eq!(action.target["root_id"].as_str(), Some("docs"));

    // 授权失败的路径同样拿不到 Action。
    let error = service
        .open_action(&settings(), "/etc/passwd")
        .expect_err("根外路径必须拒绝");
    assert_eq!(denial(&error).0, FileAccessDenied::OutsideAllowedRoots);
}

// ---------------------------------------------------------------------------
// 检索（§48）
// ---------------------------------------------------------------------------

#[test]
fn search_索引为空时走实时扫描() {
    let (service, index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_binary("/data/docs/notes/blob.dat", &[0x00, 0xff], 200)
            .with_file("/data/docs/config/.env", "SECRET_TOKEN=abc", 300),
    );
    let settings = settings();
    assert_eq!(index.stats().expect("stats").files, 0, "索引必须为空");

    // 无关键词 → 返回全部非受限文件；受限文件默认不出现（§71）。
    let found = service.search(&settings, &query("")).expect("live search");
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|entry| !entry.restricted));
    assert!(found
        .iter()
        .any(|entry| entry.relative_path == "notes/a.md"));
    assert!(found
        .iter()
        .all(|entry| entry.content_kind == FileContentKind::Unknown));

    let with_restricted = service
        .search(
            &settings,
            &FileQuery {
                include_restricted: true,
                ..query("")
            },
        )
        .expect("live search");
    assert_eq!(with_restricted.len(), 3);
    assert!(with_restricted.iter().any(|entry| entry.restricted));

    let keyword = service.search(&settings, &query("a.md")).expect("search");
    assert_eq!(keyword.len(), 1);
    assert_eq!(keyword[0].relative_path, "notes/a.md");
    assert!(service
        .search(&settings, &query("量子计算"))
        .expect("search")
        .is_empty());
}

#[test]
fn search_使用索引过滤关键词扩展名根与时间() {
    let (service, index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/data/docs/notes/b.txt", "jenkins", 200)
            .with_file("/data/docs/config/.env", "SECRET_TOKEN=abc", 300)
            .with_file("/data/other/c.md", "other", 400),
    );
    let settings = two_root_settings();

    let reports = service
        .index_configured_roots(&settings)
        .expect("index roots");
    assert_eq!(reports.len(), 2);
    assert_eq!(reports.iter().map(|report| report.indexed).sum::<usize>(), 4);
    assert_eq!(
        reports.iter().map(|report| report.restricted).sum::<usize>(),
        1
    );
    assert_eq!(index.stats().expect("stats").restricted, 1);

    // 关键词（相对路径，大小写不敏感）。
    let by_keyword = service.search(&settings, &query("A.MD")).expect("search");
    assert_eq!(by_keyword.len(), 1);
    assert_eq!(by_keyword[0].relative_path, "notes/a.md");

    // 扩展名过滤。
    let by_extension = service
        .search(
            &settings,
            &FileQuery {
                extension: Some(".txt".to_string()),
                ..query("")
            },
        )
        .expect("search");
    assert_eq!(by_extension.len(), 1);
    assert_eq!(by_extension[0].relative_path, "notes/b.txt");

    // 根过滤。
    let by_root = service
        .search(
            &settings,
            &FileQuery {
                root_id: Some("other".to_string()),
                ..query("")
            },
        )
        .expect("search");
    assert_eq!(by_root.len(), 1);
    assert_eq!(by_root[0].relative_path, "c.md");

    // 时间过滤 + 受限文件默认隐藏。
    let recent = service
        .search(
            &settings,
            &FileQuery {
                modified_after: Some(150),
                ..query("")
            },
        )
        .expect("search");
    assert_eq!(recent.len(), 2);
    assert!(recent.iter().all(|entry| entry.modified_at >= 150));
    assert!(recent.iter().all(|entry| !entry.restricted));

    let recent_all = service
        .search(
            &settings,
            &FileQuery {
                modified_after: Some(150),
                include_restricted: true,
                ..query("")
            },
        )
        .expect("search");
    assert_eq!(recent_all.len(), 3);
    assert!(recent_all.iter().any(|entry| entry.restricted));
}

// ---------------------------------------------------------------------------
// 索引（§89/§90）
// ---------------------------------------------------------------------------

#[test]
fn index_configured_roots_增量_受限标记与消失移除() {
    let index = Arc::new(FakeFileIndex::default());
    let settings = settings();

    let full = FakeFileSystem::default()
        .with_file("/data/docs/notes/a.md", "notes", 100)
        .with_file("/data/docs/notes/b.txt", "jenkins", 200)
        .with_file("/data/docs/config/.env", "SECRET_TOKEN=abc", 300);
    let service = FileService::new(Arc::new(full), index.clone());
    let first = service
        .index_configured_roots(&settings)
        .expect("index roots");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].scanned, 3);
    assert_eq!(first[0].indexed, 3);
    assert_eq!(first[0].unchanged, 0);
    assert_eq!(first[0].restricted, 1);
    assert_eq!(first[0].removed, 0);
    assert!(!first[0].truncated);
    assert_eq!(index.stats().expect("stats").files, 3);

    // 第二次扫描：size + mtime 未变 → 全部 unchanged，不重复写库。
    let second = service
        .index_configured_roots(&settings)
        .expect("index roots");
    assert_eq!((second[0].scanned, second[0].indexed, second[0].unchanged), (3, 0, 3));

    // 文件消失 → 只清索引（不碰文件系统）。
    let shrunk = FakeFileSystem::default().with_file("/data/docs/notes/a.md", "notes", 100);
    let service = FileService::new(Arc::new(shrunk), index.clone());
    let third = service
        .index_configured_roots(&settings)
        .expect("index roots");
    assert_eq!((third[0].scanned, third[0].unchanged, third[0].removed), (1, 1, 2));
    assert_eq!(index.removed.lock().len(), 2);
    assert_eq!(index.stats().expect("stats").files, 1);

    // 索引命中后 `file_id` 也能作为 target（§43）。
    let metadata = service
        .metadata(&settings, &file_id("docs", "notes/a.md"))
        .expect("authorize by file_id");
    assert_eq!(metadata.file_id, file_id("docs", "notes/a.md"));
    assert_eq!(metadata.root_id, "docs");
    assert_eq!(metadata.relative_path, "notes/a.md");
}

#[test]
fn index_configured_roots_受文件数上限截断() {
    let (service, _index) = service(
        FakeFileSystem::default()
            .with_file("/data/docs/notes/a.md", "notes", 100)
            .with_file("/data/docs/notes/b.md", "jenkins", 200),
    );
    let mut limited = settings();
    limited.max_indexed_files = 1;

    let reports = service
        .index_configured_roots(&limited)
        .expect("index roots");
    assert_eq!(reports[0].scanned, 1);
    assert!(reports[0].truncated);
    assert_eq!(reports[0].indexed, 1);
}
