//! Files 用例（V6 Track C，§40-§48）。
//!
//! 安全模型（V6 §44）：**先 canonicalize、再用策略授权、最后才访问**。
//! 任何文件操作都必须经过 [`FileService::authorize`]，不存在旁路。
//!
//! 只读边界（§42）：本服务没有任何写 / 移动 / 重命名 / 删除 / 执行能力。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use devtoolbox_core::files::{
    FileAccessDenied, FileAccessPolicy, FileContentKind, FileMetadata, KnowledgeRoot, display_path,
    extension_of, file_id, file_name_of,
};
use devtoolbox_core::knowledge::{KnowledgeResult, KnowledgeSourceKind, snippet};
use devtoolbox_core::personal_ai::Action;
use devtoolbox_core::settings::KnowledgeSettings;

use crate::error::ApplicationError;
use crate::files::ports::{
    FileIndexPort, FileIndexStats, FileQuery, FileReadOutcome, FileSystemPort, RawFile,
};
use crate::text::{coverage, keywords, normalize};
use crate::time::now_unix;

/// 服务配置（上限集中）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileConfig {
    /// 单次读取的字节上限（超过 → 仅元数据）。
    pub max_read_bytes: u64,
    /// 单次读取的字符上限（返回给模型的部分）。
    pub max_read_chars: usize,
    /// 检索默认上限。
    pub search_limit: usize,
    /// 返回片段字符上限。
    pub snippet_chars: usize,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            max_read_bytes: 2_000_000,
            max_read_chars: 20_000,
            search_limit: 10,
            snippet_chars: 400,
        }
    }
}

/// 一轮索引报告（只计数，不含正文）。
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileIndexReport {
    pub root_id: String,
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub restricted: usize,
    pub removed: usize,
    pub truncated: bool,
    pub duration_ms: u64,
}

impl FileIndexReport {
    #[must_use]
    pub fn total_changed(&self) -> usize {
        self.indexed + self.removed
    }
}

/// 安全读取结果。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileReadResult {
    pub file: FileMetadata,
    pub text: String,
    pub truncated: bool,
    pub char_count: usize,
}

impl FileReadResult {
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }
}

/// Files 服务。
pub struct FileService {
    files: Arc<dyn FileSystemPort>,
    index: Arc<dyn FileIndexPort>,
    config: FileConfig,
}

impl FileService {
    #[must_use]
    pub fn new(files: Arc<dyn FileSystemPort>, index: Arc<dyn FileIndexPort>) -> Self {
        Self {
            files,
            index,
            config: FileConfig::default(),
        }
    }

    #[must_use]
    pub fn with_config(
        files: Arc<dyn FileSystemPort>,
        index: Arc<dyn FileIndexPort>,
        config: FileConfig,
    ) -> Self {
        Self {
            files,
            index,
            config,
        }
    }

    // -----------------------------------------------------------------------
    // 授权（唯一入口）
    // -----------------------------------------------------------------------

    /// 解析并授权一个目标（`file_id` 或路径）。
    ///
    /// 步骤（§44）：canonicalize → 允许根前缀校验 → traversal / symlink escape 判定
    /// → deny 规则 → 元数据。拒绝时返回受控错误，绝不回显内容。
    ///
    /// Windows 上 `canonicalize` 返回 verbatim 路径（`\\?\D:\…`），因此**根也先
    /// canonicalize 再比较**，保证前缀校验在同一形态下进行（§44 不因平台失效）。
    pub fn authorize(
        &self,
        settings: &KnowledgeSettings,
        target: &str,
    ) -> Result<(FileMetadata, PathBuf), ApplicationError> {
        let policy = self.canonical_policy(settings);
        let target = target.trim();
        if target.is_empty() {
            return Err(file_error(FileAccessDenied::NotFound));
        }
        if !policy.is_configured() {
            return Err(file_error(FileAccessDenied::NoRootsConfigured));
        }

        let (input_path, indexed): (String, Option<FileMetadata>) =
            if let Some(entry) = self.index.get(target)? {
                (entry.path.clone(), Some(entry))
            } else if let Some(entry) = self.index.find_by_path(target)? {
                (entry.path.clone(), Some(entry))
            } else {
                (target.to_string(), None)
            };

        if FileAccessPolicy::contains_traversal(Path::new(&input_path)) {
            return Err(file_error(FileAccessDenied::Traversal));
        }
        let canonical = self.files.canonicalize(&input_path)?;
        let root_id = policy.authorize(Path::new(&input_path), &canonical)?;
        let root = policy
            .roots()
            .iter()
            .find(|root| root.id == root_id)
            .expect("root id came from the policy");

        let relative_path = canonical
            .strip_prefix(Path::new(&root.path))
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .map_err(|_| file_error(FileAccessDenied::SymlinkEscape))?;
        if relative_path.is_empty() {
            return Err(file_error(FileAccessDenied::NotAFile));
        }
        let raw = self.files.metadata(&canonical)?;
        if !raw.is_file {
            return Err(file_error(FileAccessDenied::NotAFile));
        }
        let denied = policy.is_denied(&relative_path, &file_name_of(&relative_path));
        let content_kind = if denied {
            FileContentKind::Unknown
        } else {
            classify(&self.files.read_text(&canonical, 4_096), indexed.as_ref())
        };
        let metadata = FileMetadata {
            file_id: file_id(&root.id, &relative_path),
            root_id: root.id.clone(),
            path: display_path(&canonical.to_string_lossy()),
            relative_path: relative_path.clone(),
            file_name: file_name_of(&relative_path),
            extension: extension_of(&relative_path),
            size_bytes: raw.size_bytes,
            modified_at: raw.modified_at,
            indexed_at: indexed
                .as_ref()
                .map_or_else(now_unix, |entry| entry.indexed_at),
            content_kind,
            restricted: denied,
            index_error: indexed.and_then(|entry| entry.index_error),
        };
        Ok((metadata, canonical))
    }

    /// 用 canonical 形式的根构造策略（Windows verbatim 前缀一致性，§44）。
    /// 允许根的规范形态（fail-closed）。
    ///
    /// 根 canonicalize 失败（不存在 / 权限 / 竞态删除）时**丢弃该根**而不是回退到
    /// 用户配置原文：回退会让「根」与「目标」处于不同形态空间（Windows verbatim
    /// `\\?\D:\…` vs 配置里的 `D:\…`），前缀比较结果不可预期（审查 V6-SEC-006）。
    fn canonical_policy(&self, settings: &KnowledgeSettings) -> FileAccessPolicy {
        let roots: Vec<KnowledgeRoot> = settings
            .file_policy()
            .roots()
            .iter()
            .filter_map(|root| {
                let canonical = self.files.canonicalize(&root.path).ok()?;
                Some(KnowledgeRoot {
                    path: canonical.to_string_lossy().to_string(),
                    ..root.clone()
                })
            })
            .collect();
        FileAccessPolicy::new(roots)
    }

    pub fn metadata(
        &self,
        settings: &KnowledgeSettings,
        target: &str,
    ) -> Result<FileMetadata, ApplicationError> {
        self.authorize(settings, target)
            .map(|(metadata, _)| metadata)
    }

    /// 安全读取（§42/§47）：受限文件拒绝；二进制/超大 → 受控错误，由调用方降级为元数据。
    pub fn read_text(
        &self,
        settings: &KnowledgeSettings,
        target: &str,
        max_chars: usize,
    ) -> Result<FileReadResult, ApplicationError> {
        let (metadata, path) = self.authorize(settings, target)?;
        if metadata.restricted {
            return Err(file_error(FileAccessDenied::DeniedPattern));
        }
        let limit = if max_chars == 0 {
            self.config.max_read_chars
        } else {
            max_chars.min(self.config.max_read_chars)
        };
        match self.files.read_text(&path, self.config.max_read_bytes)? {
            FileReadOutcome::Text(text) => {
                let char_count = text.chars().count();
                let truncated = char_count > limit;
                let text: String = text.chars().take(limit).collect();
                Ok(FileReadResult {
                    file: metadata,
                    text,
                    truncated,
                    char_count,
                })
            }
            FileReadOutcome::Binary => Err(file_error(FileAccessDenied::NotText)),
            FileReadOutcome::TooLarge => Err(file_error(FileAccessDenied::TooLarge)),
        }
    }

    /// 打开请求 → `OpenFile` Action（§46：backend 不执行 shell，由前端决定）。
    pub fn open_action(
        &self,
        settings: &KnowledgeSettings,
        target: &str,
    ) -> Result<(FileMetadata, Action), ApplicationError> {
        let (metadata, _) = self.authorize(settings, target)?;
        // §4.3/§46：deny 规则对「打开」同样生效 —— 否则 `files.open` 会成为
        // deny 保护的旁路（前端会拿 path 调系统默认程序打开）。
        if metadata.restricted {
            return Err(file_error(FileAccessDenied::DeniedPattern));
        }
        let action = Action::open_file(serde_json::json!({
            "file_id": metadata.file_id,
            "path": metadata.path,
            "file_name": metadata.file_name,
            "root_id": metadata.root_id,
        }));
        Ok((metadata, action))
    }

    /// 检索（§48）：优先索引库；索引为空时做一次有界的实时扫描（只读、不落库）。
    pub fn search(
        &self,
        settings: &KnowledgeSettings,
        spec: &FileQuery,
    ) -> Result<Vec<FileMetadata>, ApplicationError> {
        let policy = self.canonical_policy(settings);
        if !policy.is_configured() {
            return Err(file_error(FileAccessDenied::NoRootsConfigured));
        }
        let mut spec = spec.clone();
        if spec.limit == 0 {
            spec.limit = self.config.search_limit;
        }
        let enabled: Vec<String> = policy
            .enabled_roots()
            .into_iter()
            .map(|root| root.id.clone())
            .collect();
        let mut results: Vec<FileMetadata> = self
            .index
            .search(&spec)?
            .into_iter()
            .filter(|entry| enabled.contains(&entry.root_id))
            .collect();

        if results.is_empty() && self.index.stats()?.files == 0 {
            results = self.search_live(&policy, &spec)?;
        }
        results.sort_by(|left, right| {
            file_score(right, &spec.query)
                .partial_cmp(&file_score(left, &spec.query))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(right.modified_at.cmp(&left.modified_at))
                .then(left.relative_path.cmp(&right.relative_path))
        });
        results.truncate(spec.limit);
        Ok(results)
    }

    /// 索引全部允许根（手动 / 启动同步，§89）。
    pub fn index_configured_roots(
        &self,
        settings: &KnowledgeSettings,
    ) -> Result<Vec<FileIndexReport>, ApplicationError> {
        let policy = self.canonical_policy(settings);
        if !policy.is_configured() {
            return Err(file_error(FileAccessDenied::NoRootsConfigured));
        }
        let mut reports = Vec::new();
        for root in policy.enabled_roots() {
            let root = root.clone();
            reports.push(self.index_root(&policy, &root, settings)?);
        }
        Ok(reports)
    }

    fn index_root(
        &self,
        policy: &FileAccessPolicy,
        root: &KnowledgeRoot,
        settings: &KnowledgeSettings,
    ) -> Result<FileIndexReport, ApplicationError> {
        let started = std::time::Instant::now();
        let mut report = FileIndexReport {
            root_id: root.id.clone(),
            ..FileIndexReport::default()
        };
        let (raw_files, truncated) = self.files.walk(root, settings.max_indexed_files)?;
        report.scanned = raw_files.len();
        report.truncated = truncated;

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

        let now = now_unix();
        let mut seen: Vec<String> = Vec::with_capacity(raw_files.len());
        let mut batch: Vec<FileMetadata> = Vec::new();
        for raw in &raw_files {
            seen.push(raw.relative_path.clone());
            if existing
                .get(&raw.relative_path)
                .is_some_and(|(size, modified)| {
                    *size == raw.size_bytes && *modified == raw.modified_at
                })
            {
                report.unchanged += 1;
                continue;
            }
            let denied = policy.is_denied(&raw.relative_path, &file_name_of(&raw.relative_path));
            if denied {
                report.restricted += 1;
            }
            let content_kind = if denied {
                FileContentKind::Unknown
            } else {
                classify(&self.files.read_text(&raw.path, 4_096), None)
            };
            batch.push(FileMetadata {
                file_id: file_id(&root.id, &raw.relative_path),
                root_id: root.id.clone(),
                path: raw.path.to_string_lossy().to_string(),
                relative_path: raw.relative_path.clone(),
                file_name: file_name_of(&raw.relative_path),
                extension: extension_of(&raw.relative_path),
                size_bytes: raw.size_bytes,
                modified_at: raw.modified_at,
                indexed_at: now,
                content_kind,
                restricted: denied,
                index_error: None,
            });
            report.indexed += 1;
        }
        if !batch.is_empty() {
            self.index.upsert_many(&batch)?;
        }
        for relative in existing.keys() {
            if !seen.contains(relative) {
                self.index.remove(&file_id(&root.id, relative))?;
                report.removed += 1;
            }
        }
        report.duration_ms = started.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// 实时扫描检索（索引为空时的兜底；不写库）。
    fn search_live(
        &self,
        policy: &FileAccessPolicy,
        spec: &FileQuery,
    ) -> Result<Vec<FileMetadata>, ApplicationError> {
        let mut out: Vec<FileMetadata> = Vec::new();
        for root in policy.enabled_roots() {
            let root = root.clone();
            let (raw_files, _) = self.files.walk(&root, 5_000)?;
            for raw in raw_files {
                if let Some(entry) = live_entry(policy, &root, &raw, spec) {
                    out.push(entry);
                }
            }
        }
        Ok(out)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, ApplicationError> {
        let limit = if limit == 0 { 20 } else { limit };
        let mut entries = self.index.recent(limit)?;
        // §71：与 `search(include_restricted = false)` 同语义 —— 受限文件不进默认列表。
        entries.retain(|entry| !entry.restricted);
        Ok(entries)
    }

    pub fn stats(&self) -> Result<FileIndexStats, ApplicationError> {
        Ok(self.index.stats()?)
    }

    /// 文件 → 统一检索结果（provenance 保留路径；§66）。
    pub fn to_knowledge_results(
        &self,
        entries: &[FileMetadata],
        query: &str,
    ) -> Vec<KnowledgeResult> {
        entries
            .iter()
            .map(|entry| {
                KnowledgeResult::new(
                    KnowledgeSourceKind::File,
                    entry.file_id.clone(),
                    entry.file_name.clone(),
                    snippet(
                        &format!(
                            "{}（{}）",
                            entry.relative_path,
                            format_size(entry.size_bytes)
                        ),
                        self.config.snippet_chars,
                    ),
                    file_score(entry, query),
                )
                .with_path(entry.path.clone())
                .with_location(entry.relative_path.clone())
                .with_metadata(serde_json::json!({
                    "extension": entry.extension,
                    "size_bytes": entry.size_bytes,
                    "modified_at": entry.modified_at,
                    "content_kind": entry.content_kind,
                    "restricted": entry.restricted,
                }))
            })
            .collect()
    }
}

fn live_entry(
    policy: &FileAccessPolicy,
    root: &KnowledgeRoot,
    raw: &RawFile,
    spec: &FileQuery,
) -> Option<FileMetadata> {
    if spec.root_id.as_deref().is_some_and(|id| id != root.id) {
        return None;
    }
    if let Some(extension) = spec.extension.as_deref()
        && extension_of(&raw.relative_path).as_deref() != Some(extension)
    {
        return None;
    }
    if let Some(threshold) = spec.modified_after
        && raw.modified_at < threshold
    {
        return None;
    }
    let denied = policy.is_denied(&raw.relative_path, &file_name_of(&raw.relative_path));
    if denied && !spec.include_restricted {
        return None;
    }
    let name = file_name_of(&raw.relative_path);
    let haystack = normalize(&raw.relative_path);
    let tokens = keywords(&spec.query);
    if !tokens.is_empty() && !tokens.iter().any(|token| haystack.contains(token.as_str())) {
        return None;
    }
    Some(FileMetadata {
        file_id: file_id(&root.id, &raw.relative_path),
        root_id: root.id.clone(),
        path: raw.path.to_string_lossy().to_string(),
        relative_path: raw.relative_path.clone(),
        file_name: name,
        extension: extension_of(&raw.relative_path),
        size_bytes: raw.size_bytes,
        modified_at: raw.modified_at,
        indexed_at: now_unix(),
        content_kind: FileContentKind::Unknown,
        restricted: denied,
        index_error: None,
    })
}

/// 文件打分（确定性）：文件名命中 > 路径命中，叠加覆盖率与体积惩罚。
#[must_use]
pub fn file_score(entry: &FileMetadata, query: &str) -> f32 {
    let tokens = keywords(query);
    if tokens.is_empty() {
        return 0.3;
    }
    let name = normalize(&entry.file_name);
    let path = normalize(&entry.relative_path);
    let name_coverage = coverage(&name, &tokens);
    let path_coverage = coverage(&path, &tokens);
    let restricted_penalty = if entry.restricted { -0.2 } else { 0.0 };
    (0.3 + 0.4 * name_coverage + 0.3 * path_coverage + restricted_penalty).clamp(0.0, 1.0)
}

fn classify(
    outcome: &Result<FileReadOutcome, FileAccessDenied>,
    indexed: Option<&FileMetadata>,
) -> FileContentKind {
    match outcome {
        Ok(FileReadOutcome::Text(_)) => FileContentKind::Text,
        Ok(FileReadOutcome::Binary) => FileContentKind::Binary,
        Ok(FileReadOutcome::TooLarge) => {
            indexed.map_or(FileContentKind::Unknown, |entry| entry.content_kind)
        }
        Err(_) => indexed.map_or(FileContentKind::Unknown, |entry| entry.content_kind),
    }
}

fn file_error(reason: FileAccessDenied) -> ApplicationError {
    ApplicationError::Files {
        reason,
        reason_text: reason.as_str(),
        message: reason.message().to_string(),
    }
}

impl From<crate::files::ports::FileIndexError> for ApplicationError {
    fn from(error: crate::files::ports::FileIndexError) -> Self {
        ApplicationError::Knowledge {
            message: format!("文件索引不可用：{}", error.0),
        }
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = KB * 1_024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

impl From<FileAccessDenied> for ApplicationError {
    fn from(reason: FileAccessDenied) -> Self {
        file_error(reason)
    }
}
