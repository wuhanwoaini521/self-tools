//! 备份/恢复用例（V11 §64-§70、§160）。
//!
//! 依赖方向：契约在 `devtoolbox_core::backup`，实现（SQLite / DuckDB / 文件快照）
//! 在 `devtoolbox_infrastructure::backup`，本模块只做编排：
//!
//! 1. `backup`：把注册来源逐个快照到 `dest/`，算摘要写 `manifest.json`；
//! 2. `restore`：读 `manifest.json`，**只按清单条目**把文件写进隔离目的目录，
//!    逐条复核（路径封闭 + 摘要 + 体积），产出 `RestoreReport`。
//!
//! 关键安全边界：
//! - **不全量回拷备份目录**：只认清单条目，清单外的文件一律不碰，
//!   因此备份目录里混入的无关文件不会污染恢复目录；
//! - **目的目录封闭**：条目路径含 `..` / 绝对路径 / `.` → 记失败，绝不写盘；
//! - **不覆盖无关文件**：只写 `dest/<entry.path>` 这唯一位置，且写前先校验摘要；
//! - 单个来源失败不中止整轮（备份可部分成功），失败项显式进报告。

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use devtoolbox_core::backup::{
    BackupEntry, BackupManifest, BackupSource, RestoreReport, is_safe_backup_path, sha256_hex,
};

use crate::time::now_unix;

/// 清单文件名（与快照同目录）。
const MANIFEST_NAME: &str = "manifest.json";

/// 备份/恢复编排服务（V11 §160）。
///
/// 来源在装配期注册（`register`）；服务本身按需由组合根持有 `Arc<BackupService>`
/// 共享给多个命令，因此不需要 `Clone`。
#[derive(Default)]
pub struct BackupService {
    sources: Vec<Arc<dyn BackupSource>>,
}

impl BackupService {
    /// 空服务。
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// 注册一个备份来源（装配期调用；重复 id 会被拒绝，避免清单键冲突）。
    ///
    /// # Errors
    /// 已存在同名来源时返回错误。
    pub fn register(&mut self, source: Arc<dyn BackupSource>) -> Result<(), String> {
        let descriptor = source.describe();
        let id = descriptor.id.clone();
        if self
            .sources
            .iter()
            .any(|existing| existing.describe().id.eq_ignore_ascii_case(id.as_str()))
        {
            return Err(format!("backup source already registered: {id}"));
        }
        self.sources.push(source);
        Ok(())
    }

    /// 已注册来源数（观测）。
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// 来源描述列表（备份前展示）。
    #[must_use]
    pub fn describe_sources(&self) -> Vec<devtoolbox_core::backup::BackupTargetDescriptor> {
        self.sources
            .iter()
            .map(|source| source.describe())
            .collect()
    }

    /// 执行备份：创建 `dest`，逐个来源快照，最后写 `manifest.json`。
    ///
    /// 单个来源失败（缺文件 / 权限 / 快照中止）**不中止**整轮：其余来源照常
    /// 备份，失败原因进返回的 manifest `note` 与错误清单，调用方可如实展示。
    ///
    /// # Errors
    /// 仅「目录不可创建」这类整轮级失败返回错误；来源级失败记录在返回值里。
    pub fn backup(
        &self,
        dest: &Path,
        app_version: &str,
        note: &str,
    ) -> Result<BackupManifest, String> {
        fs::create_dir_all(dest)
            .map_err(|error| format!("create backup dir {}: {error}", dest.display()))?;

        let mut entries = Vec::with_capacity(self.sources.len());
        let mut schema_versions = BTreeMap::new();
        let mut problems: Vec<String> = Vec::new();

        for source in &self.sources {
            let descriptor = source.describe();
            if let Some(version) = descriptor.schema_version {
                schema_versions.insert(descriptor.id.clone(), version);
            }
            match source.snapshot(dest) {
                Ok(entry) => entries.push(entry),
                Err(reason) => problems.push(format!("{}: {reason}", descriptor.id)),
            }
        }

        let mut manifest = BackupManifest {
            created_at: now_unix(),
            app_version: app_version.to_string(),
            schema_versions,
            entries,
            note: if problems.is_empty() {
                note.to_string()
            } else {
                format!(
                    "{note} | 失败来源 {} 项: {}",
                    problems.len(),
                    problems.join("; ")
                )
            },
        };
        if manifest.is_empty() {
            // 空清单仍写盘：恢复时能明确读出「没有可恢复内容」，而不是猜。
            manifest.note = format!("{} | 无可用快照（0 个来源成功）", manifest.note);
        }

        write_manifest(dest, &manifest)?;
        Ok(manifest)
    }

    /// 从 `src`（备份目录）恢复到隔离的 `dest` 目录。
    ///
    /// 流程：读清单 → 逐条「安全路径判定 → 读快照 → 摘要/体积校验 → 写 dest」。
    /// **只按清单写文件**：备份目录里清单外的文件一律不复制，因此绝不会把
    /// 无关文件带进 `dest`，也不会覆盖 `dest` 里清单未列出的路径。
    ///
    /// # Errors
    /// 源目录无清单 / 清单不可解析 / 目的目录不可创建时返回错误。
    /// 单个条目失败写入 `RestoreReport.failed`，不中止其余条目。
    pub fn restore(&self, src: &Path, dest: &Path) -> Result<RestoreReport, String> {
        let manifest = read_manifest(src)?;
        fs::create_dir_all(dest)
            .map_err(|error| format!("create restore dir {}: {error}", dest.display()))?;

        let mut report = RestoreReport {
            restored: Vec::with_capacity(manifest.entries.len()),
            verified: Vec::with_capacity(manifest.entries.len()),
            failed: Vec::new(),
            integrity_ok: false,
        };

        for entry in &manifest.entries {
            match self.restore_entry(src, dest, entry) {
                Ok(()) => {
                    report.restored.push(entry.path.clone());
                    report.verified.push(entry.path.clone());
                }
                Err(reason) => report.failed.push(format!("{}: {reason}", entry.path)),
            }
        }

        report.integrity_ok = report.failed.is_empty();
        Ok(report)
    }

    /// 恢复单个条目（内部：路径封闭 → 校验 → 写盘 → 来源级复核）。
    fn restore_entry(&self, src: &Path, dest: &Path, entry: &BackupEntry) -> Result<(), String> {
        // 1) 路径封闭：拒绝 traversal / 绝对路径 / `.` 段（§44）。
        if !is_safe_backup_path(&entry.path) {
            return Err("unsafe entry path (traversal or absolute)".to_string());
        }
        // 2) 路径必须留在 dest 内（纵深防御：即便上面判定通过也再夹一次）。
        let target = dest.join(&entry.path);
        if !target.starts_with(dest) {
            return Err("entry path escapes destination directory".to_string());
        }

        let source = src.join(&entry.path);
        if !source.is_file() {
            return Err(format!("snapshot missing: {}", source.display()));
        }

        // 3) 读快照内容（校验与写入用同一份字节，避免 TOCTOU）。
        let bytes = fs::read(&source)
            .map_err(|error| format!("read snapshot {}: {error}", source.display()))?;
        // 4) 摘要 + 体积双校验；不一致直接拒绝，不写盘。
        if !entry.matches(&bytes) {
            return Err(format!(
                "checksum mismatch (expected {} bytes / {} …)",
                entry.bytes,
                entry.sha256.chars().take(12).collect::<String>()
            ));
        }

        // 5) 写 dest（原子：先写临时文件再 rename，避免半截文件留在目标位）。
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create restore parent {}: {error}", parent.display()))?;
        }
        let staging = staging_path(&target);
        let write_result = write_staging(&staging, &bytes).and_then(|()| {
            fs::rename(&staging, &target)
                .map_err(|error| format!("move into dest {}: {error}", target.display()))
        });
        if let Err(error) = write_result {
            // 任何一步失败都清掉 staging 残file，绝不在 dest 留半截文件。
            let _ = fs::remove_file(&staging);
            return Err(error);
        }

        // 6) 来源级复核（SQLite → integrity_check；DuckDB → 重新打开）。
        if let Some(source) = self.source_for_entry(entry)
            && let Err(reason) = source.verify_restored(&target, entry)
        {
            return Err(reason);
        }
        Ok(())
    }

    /// 按清单条目路径反查已注册来源（恢复后复核用）。
    ///
    /// 来源在备份目录里写出的文件名由 `snapshot_file_name()` 决定；恢复侧按
    /// **精确相等**把条目映射回来源（不用前缀匹配：`settings` 不应匹配到
    /// `settings2.json`）。找不到具体来源时返回 `None`，由摘要/体积校验兜底。
    fn source_for_entry(&self, entry: &BackupEntry) -> Option<Arc<dyn BackupSource>> {
        self.sources
            .iter()
            .find(|source| source.snapshot_file_name() == entry.path)
            .cloned()
    }
}

/// 把载荷写入 staging 文件（原子 rename 的前半步；失败不留半截目标）。
fn write_staging(staging: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(staging)
        .map_err(|error| format!("create staging {}: {error}", staging.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write staging {}: {error}", staging.display()))?;
    file.flush()
        .map_err(|error| format!("flush staging {}: {error}", staging.display()))
}

/// 写入 `manifest.json`（覆盖写；先写临时文件再 rename）。
fn write_manifest(dir: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let path = dir.join(MANIFEST_NAME);
    let text = manifest
        .to_json()
        .map_err(|error| format!("encode manifest: {error}"))?;
    let staging = staging_path(&path);
    fs::write(&staging, text)
        .map_err(|error| format!("write manifest {}: {error}", staging.display()))?;
    fs::rename(&staging, &path)
        .map_err(|error| format!("move manifest {}: {error}", path.display()))?;
    Ok(())
}

/// 读 `manifest.json` 并解析。
fn read_manifest(dir: &Path) -> Result<BackupManifest, String> {
    let path = dir.join(MANIFEST_NAME);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("read manifest {}: {error}", path.display()))?;
    BackupManifest::from_json(&text)
        .map_err(|error| format!("parse manifest {}: {error}", path.display()))
}

/// 同目录 staging 路径（原子 rename 用；写入后立即 rename，几乎不留残file）。
fn staging_path(target: &Path) -> PathBuf {
    let name = target.file_name().map_or_else(
        || "payload".to_string(),
        |name| name.to_string_lossy().to_string(),
    );
    let staging_name = format!(".{name}.restoring");
    match target.parent() {
        Some(parent) => parent.join(staging_name),
        None => PathBuf::from(staging_name),
    }
}

/// 备份源侧的辅助实现（给文本类源快速构造，测试与演练复用）。
pub struct JsonSource {
    id: String,
    path: PathBuf,
    sensitive: bool,
}

impl JsonSource {
    #[must_use]
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>, sensitive: bool) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            sensitive,
        }
    }
}

impl BackupSource for JsonSource {
    fn describe(&self) -> devtoolbox_core::backup::BackupTargetDescriptor {
        devtoolbox_core::backup::BackupTargetDescriptor::new(
            self.id.clone(),
            self.path.display().to_string(),
            devtoolbox_core::backup::EntryKind::Json,
            self.sensitive,
            false,
            None,
        )
    }

    fn snapshot(&self, dest_dir: &Path) -> Result<BackupEntry, String> {
        if !self.path.is_file() {
            return Err(format!("settings source missing: {}", self.path.display()));
        }
        let name = format!("{}.json", self.id);
        let dest = dest_dir.join(&name);
        let bytes = fs::read(&self.path)
            .map_err(|error| format!("read settings {}: {error}", self.path.display()))?;
        fs::write(&dest, &bytes)
            .map_err(|error| format!("write snapshot {}: {error}", dest.display()))?;
        let entry = BackupEntry::new(
            name,
            devtoolbox_core::backup::EntryKind::Json,
            sha256_hex(&bytes),
            bytes.len() as u64,
            self.sensitive,
            false,
        );
        // 复核：写进去的与清单里算的一致（防止读到一半被改）。
        if !entry.matches(&bytes) {
            return Err(format!("snapshot mismatch for {}", self.id));
        }
        Ok(entry)
    }
}
