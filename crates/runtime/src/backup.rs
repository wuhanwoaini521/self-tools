//! Application backup management and clean-startup restore activation.
//!
//! Restore requests are first verified into a private staging directory. They are
//! applied only while this process owns the data-directory's exclusive startup
//! lock, before any application database is opened. A full pre-restore snapshot
//! and a small transaction journal make interrupted activation recoverable.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use devtoolbox_application::backup::BackupService;
use devtoolbox_core::backup::{BackupManifest, BackupSource, EntryKind, is_safe_backup_path};
use devtoolbox_infrastructure::backup::SqliteBackupSource;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const BACKUP_DIRECTORY: &str = "backup";
const LOCK_FILE: &str = ".app-data.lock";
const PENDING_RESTORE_FILE: &str = ".restore-pending.json";
const RESTORE_JOURNAL_FILE: &str = ".restore-journal.json";
const RESTORE_STATUS_FILE: &str = ".restore-status.json";
const RESTORE_STAGING_DIRECTORY: &str = ".restore-staging";
const BACKUP_FORMAT_VERSION: u32 = 1;
const SAFETY_BACKUP_NOTE: &str = "automatic pre-restore safety snapshot";

#[derive(Clone, Copy)]
struct DatabaseSpec {
    id: &'static str,
    file_name: &'static str,
    sensitive: bool,
    rebuildable: bool,
    schema_version: Option<u32>,
}

// This is the app-owned SQLite allowlist also used by legacy-config migration.
// Downloaded corpora, user-selected roots, provider settings/secrets, and the
// bundled read-only History DuckDB are intentionally outside application backups.
const DATABASES: &[DatabaseSpec] = &[
    DatabaseSpec {
        id: "dashboard",
        file_name: "dashboard.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(devtoolbox_infrastructure::rss_store::RSS_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "news",
        file_name: "news.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(devtoolbox_infrastructure::NEWS_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "travel",
        file_name: "travel.db",
        sensitive: true,
        rebuildable: false,
        schema_version: None,
    },
    DatabaseSpec {
        id: "language",
        file_name: "language.db",
        sensitive: true,
        rebuildable: false,
        schema_version: None,
    },
    DatabaseSpec {
        id: "geography",
        file_name: "geography.db",
        sensitive: true,
        rebuildable: false,
        schema_version: None,
    },
    DatabaseSpec {
        id: "learning",
        file_name: "learning.db",
        sensitive: true,
        rebuildable: false,
        schema_version: None,
    },
    DatabaseSpec {
        id: "conversations",
        file_name: "conversations.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(1),
    },
    DatabaseSpec {
        id: "study_boards",
        file_name: "study_boards.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(devtoolbox_infrastructure::STUDY_BOARD_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "history_enrichment",
        file_name: "history_enrichment.db",
        sensitive: true,
        rebuildable: false,
        schema_version: None,
    },
    DatabaseSpec {
        id: "memory",
        file_name: "memory.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(devtoolbox_infrastructure::MEMORY_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "documents",
        file_name: "documents.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(devtoolbox_infrastructure::DOCUMENTS_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "files",
        file_name: "files.db",
        sensitive: false,
        rebuildable: true,
        schema_version: Some(devtoolbox_infrastructure::FILES_SCHEMA_VERSION),
    },
    DatabaseSpec {
        id: "server_actions",
        file_name: "server_actions.db",
        sensitive: true,
        rebuildable: false,
        schema_version: Some(1),
    },
];

/// A listed application backup. Paths and database internals stay out of the API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationBackupSummary {
    pub id: String,
    pub created_at: i64,
    pub app_version: String,
    pub format_version: u32,
    pub complete: bool,
    pub entry_count: usize,
    pub total_bytes: u64,
    pub has_sensitive_entries: bool,
    pub note: String,
}

/// Accepted restore request; application data is unchanged until clean startup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreReceipt {
    pub restore_id: String,
    pub backup_id: String,
    pub requires_restart: bool,
}

/// Persisted status for the most recent restore request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestoreStatus {
    pub state: String,
    pub restore_id: Option<String>,
    pub backup_id: Option<String>,
    pub safety_backup_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageRestoreError {
    ConfirmationRequired,
    InvalidId,
    AlreadyPending,
    InvalidBackup(String),
    Storage(String),
}

/// Shared manager used by the versioned local HTTP backup API.
#[derive(Clone)]
pub struct ApplicationBackupManager {
    data_directory: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl ApplicationBackupManager {
    /// Construct a manager for one application data directory.
    #[must_use]
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Create an application backup from the app-owned databases currently present.
    pub fn create_backup(&self, note: &str) -> Result<ApplicationBackupSummary, String> {
        let _guard = self.operation_lock.lock();
        self.create_backup_locked(note)
    }

    /// List valid backup manifests, newest first.
    pub fn list_backups(&self) -> Result<Vec<ApplicationBackupSummary>, String> {
        let _guard = self.operation_lock.lock();
        let root = self.backup_root();
        if !path_exists(&root)? {
            return Ok(Vec::new());
        }
        ensure_directory(&root)?;
        let mut summaries = Vec::new();
        let entries =
            fs::read_dir(&root).map_err(|error| format!("read backup directory: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read backup entry: {error}"))?;
            let path = entry.path();
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !is_canonical_uuid(id) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let Ok(manifest) = read_manifest_secure(&path) else {
                continue;
            };
            summaries.push(summary(id.to_string(), &manifest));
        }
        summaries.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(summaries)
    }

    /// Verify a backup into isolated staging and schedule activation at next startup.
    pub fn stage_restore(
        &self,
        backup_id: &str,
        confirmed: bool,
    ) -> Result<RestoreReceipt, StageRestoreError> {
        let _guard = self.operation_lock.lock();
        if !confirmed {
            return Err(StageRestoreError::ConfirmationRequired);
        }
        validate_uuid(backup_id).map_err(|_| StageRestoreError::InvalidId)?;
        let pending_path = self.data_directory.join(PENDING_RESTORE_FILE);
        if path_exists(&pending_path).map_err(StageRestoreError::Storage)? {
            return Err(StageRestoreError::AlreadyPending);
        }

        let backup_dir = self.backup_root().join(backup_id);
        let manifest =
            load_restorable_manifest(&backup_dir).map_err(StageRestoreError::InvalidBackup)?;
        let service =
            backup_service_for_restore(&self.data_directory).map_err(StageRestoreError::Storage)?;
        validate_manifest_targets(&manifest).map_err(StageRestoreError::InvalidBackup)?;
        verify_snapshot_files(&backup_dir, &manifest).map_err(StageRestoreError::InvalidBackup)?;

        let restore_id = Uuid::new_v4().to_string();
        let stage_root = self.data_directory.join(RESTORE_STAGING_DIRECTORY);
        ensure_private_directory(&stage_root).map_err(StageRestoreError::Storage)?;
        let stage_dir = stage_root.join(&restore_id);
        create_private_directory(&stage_dir).map_err(StageRestoreError::Storage)?;
        let verified_dir = stage_dir.join("verified");
        if let Err(error) = restore_into(&service, &backup_dir, &verified_dir, &manifest) {
            let _ = remove_owned_directory(&stage_root, &restore_id);
            return Err(StageRestoreError::InvalidBackup(error));
        }

        let pending = PendingRestore {
            format_version: BACKUP_FORMAT_VERSION,
            restore_id: restore_id.clone(),
            backup_id: backup_id.to_string(),
        };
        if let Err(error) = write_json_new(&pending_path, &pending) {
            let _ = remove_owned_directory(&stage_root, &restore_id);
            return Err(StageRestoreError::Storage(error));
        }
        Ok(RestoreReceipt {
            restore_id,
            backup_id: backup_id.to_string(),
            requires_restart: true,
        })
    }

    /// Read the pending request or the persisted result of the last activation.
    pub fn restore_status(&self) -> Result<RestoreStatus, String> {
        let _guard = self.operation_lock.lock();
        let pending_path = self.data_directory.join(PENDING_RESTORE_FILE);
        if path_exists(&pending_path)? {
            let pending: PendingRestore = read_json_secure(&pending_path)?;
            validate_uuid(&pending.restore_id)?;
            validate_uuid(&pending.backup_id)?;
            if pending.format_version != BACKUP_FORMAT_VERSION {
                return Err("unsupported pending restore format".to_string());
            }
            return Ok(RestoreStatus {
                state: "pending_restart".to_string(),
                restore_id: Some(pending.restore_id),
                backup_id: Some(pending.backup_id),
                safety_backup_id: None,
                message: Some(
                    "Restore is staged and will apply after all app instances exit.".to_string(),
                ),
            });
        }
        let status_path = self.data_directory.join(RESTORE_STATUS_FILE);
        if !path_exists(&status_path)? {
            return Ok(RestoreStatus {
                state: "none".to_string(),
                restore_id: None,
                backup_id: None,
                safety_backup_id: None,
                message: None,
            });
        }
        read_json_secure(&status_path)
    }

    fn backup_root(&self) -> PathBuf {
        self.data_directory.join(BACKUP_DIRECTORY)
    }

    fn create_backup_locked(&self, note: &str) -> Result<ApplicationBackupSummary, String> {
        if note.chars().count() > 240 {
            return Err("backup note must be 240 characters or fewer".to_string());
        }
        let backup_root = self.backup_root();
        ensure_private_directory(&backup_root)?;
        let service = backup_service_for_present(&self.data_directory)?;
        if service.source_count() == 0 {
            return Err("no app-owned database files are available to back up".to_string());
        }
        let id = Uuid::new_v4().to_string();
        let destination = backup_root.join(&id);
        create_private_directory(&destination)?;
        let manifest = match service.backup(&destination, env!("CARGO_PKG_VERSION"), note) {
            Ok(manifest) => manifest,
            Err(error) => {
                let _ = remove_owned_directory(&backup_root, &id);
                return Err(error);
            }
        };
        restrict_tree_files(&destination)?;
        Ok(summary(id, &manifest))
    }
}

/// A lifetime guard that prevents concurrent app processes from applying restores
/// while another process is using the same data directory.
pub struct AppDataDirectoryLock {
    _file: File,
}

impl AppDataDirectoryLock {
    /// Acquire the shared lifetime lock and apply pending restores before DB opens.
    pub fn acquire(data_directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_directory)
            .map_err(|error| format!("create app data directory: {error}"))?;
        let canonical_directory = data_directory
            .canonicalize()
            .map_err(|error| format!("resolve app data directory: {error}"))?;
        let lock_path = canonical_directory.join(LOCK_FILE);
        ensure_lock_file_path(&lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| format!("open app data lock: {error}"))?;
        restrict_file(&lock_path).map_err(|error| format!("secure app data lock: {error}"))?;

        if file.try_lock().is_ok() {
            recover_interrupted_restore(&canonical_directory)?;
            apply_pending_restore(&canonical_directory)?;
            sweep_orphan_restore_staging(&canonical_directory)?;
            drop(file);

            // Drop the startup-exclusive lock, then join the steady-state shared lock.
            // Another process may win the short gap; the recheck below handles that.
            let shared = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lock_path)
                .map_err(|error| format!("reopen app data lock: {error}"))?;
            shared
                .lock_shared()
                .map_err(|error| format!("acquire shared app data lock: {error}"))?;
            if restore_marker_exists(&canonical_directory)? {
                return Err(
                    "a restore is pending; close every app using this data directory and restart"
                        .to_string(),
                );
            }
            Ok(Self { _file: shared })
        } else {
            file.lock_shared()
                .map_err(|error| format!("wait for app data lock: {error}"))?;
            if restore_marker_exists(&canonical_directory)? {
                return Err(
                    "a restore is pending; close every app using this data directory and restart"
                        .to_string(),
                );
            }
            Ok(Self { _file: file })
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingRestore {
    format_version: u32,
    restore_id: String,
    backup_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RestoreJournal {
    format_version: u32,
    phase: JournalPhase,
    restore_id: String,
    backup_id: String,
    safety_backup_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum JournalPhase {
    Applying,
    Committed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PersistedRestoreStatus {
    state: String,
    restore_id: Option<String>,
    backup_id: Option<String>,
    safety_backup_id: Option<String>,
    message: Option<String>,
}

fn summary(id: String, manifest: &BackupManifest) -> ApplicationBackupSummary {
    let note = manifest
        .note
        .split(" | 失败来源 ")
        .next()
        .unwrap_or(&manifest.note)
        .split(" | 无可用快照")
        .next()
        .unwrap_or(&manifest.note);
    ApplicationBackupSummary {
        id,
        created_at: manifest.created_at,
        app_version: manifest.app_version.clone(),
        format_version: manifest.format_version,
        complete: manifest.complete,
        entry_count: manifest.entries.len(),
        total_bytes: manifest.total_bytes(),
        has_sensitive_entries: manifest.has_sensitive_entries(),
        note: note.to_string(),
    }
}

fn backup_service_for_present(data_directory: &Path) -> Result<BackupService, String> {
    let mut service = BackupService::new();
    for spec in DATABASES {
        let path = data_directory.join(spec.file_name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("inspect app database {}: {error}", spec.file_name)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "app database {} is not a regular file",
                spec.file_name
            ));
        }
        service
            .register(Arc::new(SqliteBackupSource::new(
                spec.id,
                path,
                spec.sensitive,
                spec.rebuildable,
                spec.schema_version,
            )))
            .map_err(|error| error.to_string())?;
    }
    Ok(service)
}

fn backup_service_for_restore(data_directory: &Path) -> Result<BackupService, String> {
    let mut service = BackupService::new();
    for spec in DATABASES {
        service
            .register(Arc::new(SqliteBackupSource::new(
                spec.id,
                data_directory.join(spec.file_name),
                spec.sensitive,
                spec.rebuildable,
                spec.schema_version,
            )))
            .map_err(|error| error.to_string())?;
    }
    Ok(service)
}

fn load_restorable_manifest(backup_directory: &Path) -> Result<BackupManifest, String> {
    if let Some(parent) = backup_directory.parent() {
        ensure_directory(parent)?;
    }
    ensure_directory(backup_directory)?;
    let manifest = read_manifest_secure(backup_directory)?;
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err("backup format is not supported for application restore".to_string());
    }
    if !manifest.complete || manifest.entries.is_empty() {
        return Err("only complete, non-empty application backups can be restored".to_string());
    }
    validate_manifest_targets(&manifest)?;
    Ok(manifest)
}

fn validate_manifest_targets(manifest: &BackupManifest) -> Result<(), String> {
    let known: HashMap<String, &DatabaseSpec> = DATABASES
        .iter()
        .map(|spec| (snapshot_name(spec), spec))
        .collect();
    let mut seen = BTreeSet::new();
    for entry in &manifest.entries {
        if !is_safe_backup_path(&entry.path) || !seen.insert(entry.path.as_str()) {
            return Err("backup manifest contains an unsafe or duplicate entry".to_string());
        }
        let Some(spec) = known.get(entry.path.as_str()) else {
            return Err(format!(
                "backup entry is outside the app database allowlist: {}",
                entry.path
            ));
        };
        if entry.kind != EntryKind::Sqlite || entry.path != snapshot_name(spec) {
            return Err(format!(
                "backup entry type does not match {}",
                spec.file_name
            ));
        }
    }
    for (id, stored_version) in &manifest.schema_versions {
        let Some(spec) = DATABASES.iter().find(|spec| spec.id == id) else {
            return Err(format!("backup manifest has an unknown schema id: {id}"));
        };
        if spec
            .schema_version
            .is_some_and(|current| *stored_version > current)
        {
            return Err(format!(
                "backup schema for {id} is newer than this application"
            ));
        }
    }
    Ok(())
}

fn snapshot_name(spec: &DatabaseSpec) -> String {
    format!("{}.bak", spec.file_name)
}

fn verify_snapshot_files(backup_directory: &Path, manifest: &BackupManifest) -> Result<(), String> {
    for entry in &manifest.entries {
        let path = backup_directory.join(&entry.path);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect backup snapshot {}: {error}", entry.path))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "backup snapshot {} is not a regular file",
                entry.path
            ));
        }
    }
    Ok(())
}

fn restore_into(
    service: &BackupService,
    backup_directory: &Path,
    destination: &Path,
    expected_manifest: &BackupManifest,
) -> Result<(), String> {
    create_private_directory(destination)?;
    let report = service.restore(backup_directory, destination)?;
    let actual_manifest = read_manifest_secure(backup_directory)?;
    if &actual_manifest != expected_manifest {
        return Err("backup manifest changed during verification".to_string());
    }
    if !report.is_ok()
        || report.restored.len() != expected_manifest.entries.len()
        || report.verified.len() != expected_manifest.entries.len()
    {
        return Err(format!("backup verification failed: {}", report.summary()));
    }
    for entry in &expected_manifest.entries {
        let restored = destination.join(&entry.path);
        ensure_regular_file(&restored)?;
        restrict_file(&restored).map_err(|error| format!("secure staged snapshot: {error}"))?;
    }
    Ok(())
}

fn apply_pending_restore(data_directory: &Path) -> Result<(), String> {
    let pending_path = data_directory.join(PENDING_RESTORE_FILE);
    if !path_exists(&pending_path)? {
        return Ok(());
    }
    let pending: PendingRestore = read_json_secure(&pending_path)?;
    validate_uuid(&pending.restore_id)?;
    validate_uuid(&pending.backup_id)?;
    if pending.format_version != BACKUP_FORMAT_VERSION {
        return Err("unsupported pending restore format".to_string());
    }
    match apply_pending_restore_transaction(data_directory, &pending) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("[backup] staged restore could not be applied: {error}");
            if path_exists(&data_directory.join(RESTORE_JOURNAL_FILE))? {
                recover_interrupted_restore(data_directory)?;
            } else {
                record_failed_restore(data_directory, &pending, None)?;
                cleanup_restore_artifacts(data_directory, &pending.restore_id, false)?;
            }
            Ok(())
        }
    }
}

fn apply_pending_restore_transaction(
    data_directory: &Path,
    pending: &PendingRestore,
) -> Result<(), String> {
    let manager = ApplicationBackupManager::new(data_directory);
    let backup_directory = manager.backup_root().join(&pending.backup_id);
    let manifest = load_restorable_manifest(&backup_directory)?;
    verify_snapshot_files(&backup_directory, &manifest)?;
    let service = backup_service_for_restore(data_directory)?;
    let stage_root = data_directory.join(RESTORE_STAGING_DIRECTORY);
    ensure_private_directory(&stage_root)?;
    let stage_dir = stage_root.join(&pending.restore_id);
    ensure_private_directory(&stage_dir)?;
    let activation_dir = stage_dir.join(format!("activation-{}", Uuid::new_v4()));
    restore_into(&service, &backup_directory, &activation_dir, &manifest)?;

    let safety = manager.create_backup_locked(SAFETY_BACKUP_NOTE)?;
    if !safety.complete || safety.entry_count == 0 {
        return Err("pre-restore safety snapshot was incomplete".to_string());
    }
    let safety_directory = manager.backup_root().join(&safety.id);
    let safety_manifest = load_restorable_manifest(&safety_directory)?;
    verify_snapshot_files(&safety_directory, &safety_manifest)?;

    let mut journal = RestoreJournal {
        format_version: BACKUP_FORMAT_VERSION,
        phase: JournalPhase::Applying,
        restore_id: pending.restore_id.clone(),
        backup_id: pending.backup_id.clone(),
        safety_backup_id: safety.id.clone(),
    };
    write_json_replace(&data_directory.join(RESTORE_JOURNAL_FILE), &journal)?;
    install_manifest_files(
        data_directory,
        &activation_dir,
        &manifest,
        MissingDatabasePolicy::Preserve,
    )?;

    journal.phase = JournalPhase::Committed;
    write_json_replace(&data_directory.join(RESTORE_JOURNAL_FILE), &journal)?;
    write_restore_status(
        data_directory,
        PersistedRestoreStatus {
            state: "applied".to_string(),
            restore_id: Some(pending.restore_id.clone()),
            backup_id: Some(pending.backup_id.clone()),
            safety_backup_id: Some(safety.id),
            message: Some(
                "Restore applied successfully; the pre-restore snapshot is retained.".to_string(),
            ),
        },
    )?;
    cleanup_restore_artifacts(data_directory, &pending.restore_id, true)
}

fn recover_interrupted_restore(data_directory: &Path) -> Result<(), String> {
    let journal_path = data_directory.join(RESTORE_JOURNAL_FILE);
    if !path_exists(&journal_path)? {
        return Ok(());
    }
    let journal: RestoreJournal = read_json_secure(&journal_path)?;
    if journal.format_version != BACKUP_FORMAT_VERSION {
        return Err(
            "unsupported restore journal format; application databases were not opened".to_string(),
        );
    }
    validate_uuid(&journal.restore_id)?;
    validate_uuid(&journal.backup_id)?;
    validate_uuid(&journal.safety_backup_id)?;
    match journal.phase {
        JournalPhase::Committed => {
            write_restore_status(
                data_directory,
                PersistedRestoreStatus {
                    state: "applied".to_string(),
                    restore_id: Some(journal.restore_id.clone()),
                    backup_id: Some(journal.backup_id),
                    safety_backup_id: Some(journal.safety_backup_id),
                    message: Some(
                        "Restore applied successfully; the pre-restore snapshot is retained."
                            .to_string(),
                    ),
                },
            )?;
            cleanup_restore_artifacts(data_directory, &journal.restore_id, true)
        }
        JournalPhase::Applying => {
            rollback_from_safety_backup(
                data_directory,
                &journal.safety_backup_id,
                &journal.restore_id,
            )?;
            write_restore_status(
                data_directory,
                PersistedRestoreStatus {
                    state: "failed".to_string(),
                    restore_id: Some(journal.restore_id.clone()),
                    backup_id: Some(journal.backup_id),
                    safety_backup_id: Some(journal.safety_backup_id),
                    message: Some(
                        "Restore did not finish. The pre-restore snapshot was restored."
                            .to_string(),
                    ),
                },
            )?;
            cleanup_restore_artifacts(data_directory, &journal.restore_id, true)
        }
    }
}

fn rollback_from_safety_backup(
    data_directory: &Path,
    safety_backup_id: &str,
    restore_id: &str,
) -> Result<(), String> {
    let manager = ApplicationBackupManager::new(data_directory);
    let safety_directory = manager.backup_root().join(safety_backup_id);
    let manifest = load_restorable_manifest(&safety_directory)?;
    verify_snapshot_files(&safety_directory, &manifest)?;
    let service = backup_service_for_restore(data_directory)?;
    let stage_root = data_directory.join(RESTORE_STAGING_DIRECTORY);
    ensure_private_directory(&stage_root)?;
    let rollback_dir = stage_root
        .join(restore_id)
        .join(format!("rollback-{}", Uuid::new_v4()));
    restore_into(&service, &safety_directory, &rollback_dir, &manifest)?;
    install_manifest_files(
        data_directory,
        &rollback_dir,
        &manifest,
        MissingDatabasePolicy::Remove,
    )
}

#[derive(Clone, Copy)]
enum MissingDatabasePolicy {
    Preserve,
    Remove,
}

fn install_manifest_files(
    data_directory: &Path,
    restored_directory: &Path,
    manifest: &BackupManifest,
    missing_database_policy: MissingDatabasePolicy,
) -> Result<(), String> {
    let by_target: HashMap<&str, _> = manifest
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    for spec in DATABASES {
        let snapshot = snapshot_name(spec);
        if let Some(entry) = by_target.get(snapshot.as_str()) {
            let target = data_directory.join(spec.file_name);
            ensure_safe_optional_file(&target)?;
            remove_sqlite_sidecars(data_directory, spec.file_name)?;
            let staged = restored_directory.join(&entry.path);
            ensure_regular_file(&staged)?;
            let temporary =
                data_directory.join(format!(".{}.install-{}", spec.file_name, Uuid::new_v4()));
            let mut temporary_created = false;
            let install_result = (|| {
                let mut source = File::open(&staged)
                    .map_err(|error| format!("open staged database {}: {error}", spec.file_name))?;
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut temporary_file = options.open(&temporary).map_err(|error| {
                    format!(
                        "create private restore database {}: {error}",
                        spec.file_name
                    )
                })?;
                temporary_created = true;
                io::copy(&mut source, &mut temporary_file)
                    .map_err(|error| format!("copy staged database {}: {error}", spec.file_name))?;
                temporary_file.sync_all().map_err(|error| {
                    format!("sync restored database {}: {error}", spec.file_name)
                })?;
                drop(temporary_file);
                restrict_file(&temporary).map_err(|error| {
                    format!("secure restored database {}: {error}", spec.file_name)
                })?;
                File::open(&temporary)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| {
                        format!("sync restored database {}: {error}", spec.file_name)
                    })?;
                replace_file(&temporary, &target)?;
                sync_directory(data_directory)?;
                let source = SqliteBackupSource::new(
                    spec.id,
                    &target,
                    spec.sensitive,
                    spec.rebuildable,
                    spec.schema_version,
                );
                source.verify_restored(&target, entry).map_err(|error| {
                    format!("verify installed database {}: {error}", spec.file_name)
                })?;
                Ok(())
            })();
            if install_result.is_err() {
                if temporary_created {
                    let _ = fs::remove_file(&temporary);
                }
                return install_result;
            }
        } else if matches!(missing_database_policy, MissingDatabasePolicy::Remove) {
            let target = data_directory.join(spec.file_name);
            ensure_safe_optional_file(&target)?;
            remove_sqlite_sidecars(data_directory, spec.file_name)?;
            remove_regular_file_if_present(&target)?;
        }
    }
    sync_directory(data_directory)
}

fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if path_exists(target).map_err(|error| error.to_string())? {
        fs::remove_file(target).map_err(|error| format!("replace restored database: {error}"))?;
    }
    fs::rename(source, target).map_err(|error| format!("install restored database: {error}"))
}

fn remove_sqlite_sidecars(data_directory: &Path, file_name: &str) -> Result<(), String> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let path = data_directory.join(format!("{file_name}{suffix}"));
        remove_regular_file_if_present(&path)?;
    }
    Ok(())
}

fn record_failed_restore(
    data_directory: &Path,
    pending: &PendingRestore,
    safety_backup_id: Option<String>,
) -> Result<(), String> {
    write_restore_status(
        data_directory,
        PersistedRestoreStatus {
            state: "failed".to_string(),
            restore_id: Some(pending.restore_id.clone()),
            backup_id: Some(pending.backup_id.clone()),
            safety_backup_id,
            message: Some(
                "Restore could not be applied. Existing application data was retained.".to_string(),
            ),
        },
    )
}

fn write_restore_status(
    data_directory: &Path,
    status: PersistedRestoreStatus,
) -> Result<(), String> {
    write_json_replace(&data_directory.join(RESTORE_STATUS_FILE), &status)
}

fn sweep_orphan_restore_staging(data_directory: &Path) -> Result<(), String> {
    let stage_root = data_directory.join(RESTORE_STAGING_DIRECTORY);
    if path_exists(&stage_root)? {
        ensure_directory(&stage_root)?;

        let mut retained = BTreeSet::new();
        let pending_path = data_directory.join(PENDING_RESTORE_FILE);
        if path_exists(&pending_path)? {
            let pending: PendingRestore = read_json_secure(&pending_path)?;
            validate_uuid(&pending.restore_id)?;
            retained.insert(pending.restore_id);
        }
        let journal_path = data_directory.join(RESTORE_JOURNAL_FILE);
        if path_exists(&journal_path)? {
            let journal: RestoreJournal = read_json_secure(&journal_path)?;
            validate_uuid(&journal.restore_id)?;
            retained.insert(journal.restore_id);
        }

        for entry in
            fs::read_dir(&stage_root).map_err(|error| format!("list restore staging: {error}"))?
        {
            let path = entry
                .map_err(|error| format!("read restore staging entry: {error}"))?
                .path();
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if is_canonical_uuid(id) && !retained.contains(id) {
                remove_owned_directory(&stage_root, id)?;
            }
        }
        sync_directory(&stage_root)?;
    }

    let mut removed_install_files = false;
    for entry in
        fs::read_dir(data_directory).map_err(|error| format!("list app data directory: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("read app data entry: {error}"))?
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_restore_install_temporary_name(name) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect restore install temporary: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("refuse to remove non-regular restore install temporary".to_string());
        }
        fs::remove_file(&path)
            .map_err(|error| format!("remove orphan restore install temporary: {error}"))?;
        removed_install_files = true;
    }
    if removed_install_files {
        sync_directory(data_directory)?;
    }
    Ok(())
}

fn is_restore_install_temporary_name(name: &str) -> bool {
    DATABASES.iter().any(|spec| {
        let prefix = format!(".{}.install-", spec.file_name);
        name.strip_prefix(&prefix).is_some_and(is_canonical_uuid)
    })
}

fn cleanup_restore_artifacts(
    data_directory: &Path,
    restore_id: &str,
    remove_journal: bool,
) -> Result<(), String> {
    let stage_root = data_directory.join(RESTORE_STAGING_DIRECTORY);
    if path_exists(&stage_root).map_err(|error| error.to_string())? {
        ensure_directory(&stage_root)?;
        remove_owned_directory(&stage_root, restore_id)?;
    }
    let pending_path = data_directory.join(PENDING_RESTORE_FILE);
    if path_exists(&pending_path).map_err(|error| error.to_string())? {
        let pending: PendingRestore = read_json_secure(&pending_path)?;
        if pending.restore_id == restore_id {
            fs::remove_file(&pending_path)
                .map_err(|error| format!("remove pending restore marker: {error}"))?;
        }
    }
    if remove_journal {
        let journal_path = data_directory.join(RESTORE_JOURNAL_FILE);
        if path_exists(&journal_path).map_err(|error| error.to_string())? {
            let journal: RestoreJournal = read_json_secure(&journal_path)?;
            if journal.restore_id == restore_id {
                fs::remove_file(&journal_path)
                    .map_err(|error| format!("remove restore journal: {error}"))?;
            }
        }
    }
    sync_directory(data_directory)?;
    Ok(())
}

fn restore_marker_exists(data_directory: &Path) -> Result<bool, String> {
    Ok(path_exists(&data_directory.join(PENDING_RESTORE_FILE))?
        || path_exists(&data_directory.join(RESTORE_JOURNAL_FILE))?)
}

fn ensure_lock_file_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("app data lock path is not a regular file".to_string())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect app data lock: {error}")),
    }
}

fn read_manifest_secure(directory: &Path) -> Result<BackupManifest, String> {
    let path = directory.join("manifest.json");
    let text = read_text_secure(&path)?;
    BackupManifest::from_json(&text).map_err(|error| format!("parse backup manifest: {error}"))
}

fn read_json_secure<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = read_text_secure(path)?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn read_text_secure(path: &Path) -> Result<String, String> {
    ensure_regular_file(path)?;
    fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temporary = temporary_sibling(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create restore marker staging file: {error}"))?;
        serde_json::to_writer(&mut file, value)
            .map_err(|error| format!("encode restore marker: {error}"))?;
        file.flush()
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("sync restore marker: {error}"))?;
        restrict_file(&temporary).map_err(|error| format!("secure restore marker: {error}"))?;
        fs::hard_link(&temporary, path)
            .map_err(|error| format!("reserve restore marker without overwrite: {error}"))?;
        sync_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn write_json_replace<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if path_exists(path)? {
        ensure_regular_file(path)?;
    }
    let temporary = temporary_sibling(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create status staging file: {error}"))?;
        serde_json::to_writer(&mut file, value)
            .map_err(|error| format!("encode restore status: {error}"))?;
        file.flush()
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("sync restore status: {error}"))?;
        restrict_file(&temporary).map_err(|error| format!("secure restore status: {error}"))?;
        fs::rename(&temporary, path).map_err(|error| format!("install restore status: {error}"))?;
        sync_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn temporary_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    path.with_file_name(format!(".{name}.{}.tmp", Uuid::new_v4()))
}

fn path_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect path {}: {error}", path.display())),
    }
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect directory {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{} is not a regular directory", path.display()));
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!("{} is not a regular directory", path.display()));
            }
            restrict_directory(path).map_err(|error| format!("secure directory: {error}"))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create directory parent: {error}"))?;
            }
            create_private_directory(path).or_else(|error| {
                if path_exists(path).unwrap_or(false) {
                    ensure_private_directory(path)
                } else {
                    Err(error)
                }
            })
        }
        Err(error) => Err(format!("inspect directory {}: {error}", path.display())),
    }
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| format!("create private directory {}: {error}", path.display()))?;
    restrict_directory(path)
        .map_err(|error| format!("secure directory {}: {error}", path.display()))?;
    sync_directory(path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect file {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(())
}

fn ensure_safe_optional_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(format!("{} is not a regular file", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {}: {error}", path.display())),
    }
}

fn remove_regular_file_if_present(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!(
                    "refuse to remove non-regular file {}",
                    path.display()
                ));
            }
            fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {}: {error}", path.display())),
    }
}

fn remove_owned_directory(parent: &Path, id: &str) -> Result<(), String> {
    validate_uuid(id)?;
    ensure_directory(parent)?;
    let path = parent.join(id);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("refuse to remove non-directory restore staging path".to_string());
            }
            fs::remove_dir_all(&path)
                .map_err(|error| format!("remove owned staging directory: {error}"))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect owned staging directory: {error}")),
    }
}

fn restrict_tree_files(directory: &Path) -> Result<(), String> {
    ensure_directory(directory)?;
    for entry in fs::read_dir(directory).map_err(|error| format!("list backup files: {error}"))? {
        let path = entry
            .map_err(|error| format!("read backup file: {error}"))?
            .path();
        ensure_regular_file(&path)?;
        restrict_file(&path).map_err(|error| format!("secure backup file: {error}"))?;
        File::open(&path)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("sync backup file: {error}"))?;
    }
    sync_directory(directory)?;
    Ok(())
}

fn restrict_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn restrict_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("sync directory {}: {error}", path.display()))
}

fn validate_uuid(id: &str) -> Result<(), String> {
    if is_canonical_uuid(id) {
        Ok(())
    } else {
        Err("backup and restore ids must be canonical UUIDs".to_string())
    }
}

fn is_canonical_uuid(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::{
        AppDataDirectoryLock, ApplicationBackupManager, BACKUP_DIRECTORY, BACKUP_FORMAT_VERSION,
        JournalPhase, RESTORE_JOURNAL_FILE, RESTORE_STAGING_DIRECTORY, RestoreJournal,
        StageRestoreError, Uuid, write_json_replace,
    };

    fn set_value(database: &Path, value: &str) {
        let connection = Connection::open(database).expect("open fixture database");
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS fixture_state (value TEXT NOT NULL); DELETE FROM fixture_state;",
            )
            .expect("initialize fixture table");
        connection
            .execute("INSERT INTO fixture_state (value) VALUES (?1)", [value])
            .expect("write fixture value");
    }

    fn read_value(database: &Path) -> String {
        Connection::open(database)
            .expect("open fixture database")
            .query_row("SELECT value FROM fixture_state", [], |row| row.get(0))
            .expect("read fixture value")
    }

    #[test]
    fn application_backup_stages_restore_only_on_clean_startup_and_preserves_unlisted_database() {
        let home = tempdir().expect("temporary application home");
        let database = home.path().join("learning.db");
        let unlisted_database = home.path().join("news.db");
        set_value(&database, "from-backup");

        let manager = ApplicationBackupManager::new(home.path());
        let backup = manager
            .create_backup("isolated recovery fixture")
            .expect("create backup");
        assert!(
            backup.complete,
            "backup should include every present app source"
        );

        set_value(&database, "current-live-value");
        set_value(&unlisted_database, "created-after-backup");
        let staged = manager
            .stage_restore(&backup.id, true)
            .expect("stage the verified restore");
        assert!(staged.requires_restart);
        assert_eq!(read_value(&database), "current-live-value");

        drop(manager);
        let _data_lock = AppDataDirectoryLock::acquire(home.path())
            .expect("clean startup applies the staged restore");

        assert_eq!(read_value(&database), "from-backup");
        assert_eq!(read_value(&unlisted_database), "created-after-backup");
        let manager = ApplicationBackupManager::new(home.path());
        assert_eq!(
            manager.restore_status().expect("restore status").state,
            "applied"
        );
        assert!(
            manager
                .list_backups()
                .expect("list backups")
                .iter()
                .any(|item| item.note == "automatic pre-restore safety snapshot"),
            "the pre-restore state remains available as a backup"
        );
    }

    #[test]
    fn startup_sweeps_orphaned_restore_staging_and_install_files() {
        let home = tempdir().expect("temporary application home");
        let orphan_id = Uuid::new_v4().to_string();
        let orphan = home.path().join(RESTORE_STAGING_DIRECTORY).join(&orphan_id);
        std::fs::create_dir_all(&orphan).expect("create orphan staging directory");
        std::fs::write(orphan.join("learning.db"), b"sensitive staged fixture")
            .expect("write staged fixture");
        let install_temporary = home
            .path()
            .join(format!(".learning.db.install-{orphan_id}"));
        std::fs::write(&install_temporary, b"partial database fixture")
            .expect("write orphan install fixture");
        let unrelated = home.path().join(".learning.db.install-not-a-uuid");
        std::fs::write(&unrelated, b"unrelated fixture").expect("write unrelated file fixture");

        let _data_lock = AppDataDirectoryLock::acquire(home.path())
            .expect("startup cleans stale restore staging and install files");
        assert!(!orphan.exists());
        assert!(!install_temporary.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn startup_rolls_back_an_interrupted_restore_from_safety_snapshot() {
        let home = tempdir().expect("temporary application home");
        let database = home.path().join("learning.db");
        let partially_installed_database = home.path().join("news.db");
        set_value(&database, "restore-target");
        let manager = ApplicationBackupManager::new(home.path());
        let target = manager
            .create_backup("restore target")
            .expect("create restore target");

        set_value(&database, "pre-restore-live-value");
        let safety = manager
            .create_backup("automatic pre-restore safety snapshot")
            .expect("create safety snapshot");
        let receipt = manager
            .stage_restore(&target.id, true)
            .expect("stage restore");
        write_json_replace(
            &home.path().join(RESTORE_JOURNAL_FILE),
            &RestoreJournal {
                format_version: BACKUP_FORMAT_VERSION,
                phase: JournalPhase::Applying,
                restore_id: receipt.restore_id,
                backup_id: target.id,
                safety_backup_id: safety.id,
            },
        )
        .expect("persist interrupted journal");
        set_value(&database, "partially-installed-value");
        set_value(&partially_installed_database, "partial-install");

        let _data_lock = AppDataDirectoryLock::acquire(home.path())
            .expect("startup rolls interrupted restore back");
        assert_eq!(read_value(&database), "pre-restore-live-value");
        assert!(!partially_installed_database.exists());
    }

    #[test]
    fn corrupted_snapshot_is_rejected_without_touching_live_data() {
        let home = tempdir().expect("temporary application home");
        let database = home.path().join("learning.db");
        set_value(&database, "live-value");
        let manager = ApplicationBackupManager::new(home.path());
        let backup = manager.create_backup("fixture").expect("create backup");
        std::fs::write(
            home.path()
                .join(BACKUP_DIRECTORY)
                .join(&backup.id)
                .join("learning.db.bak"),
            b"corrupted snapshot",
        )
        .expect("corrupt fixture backup");

        assert!(matches!(
            manager.stage_restore(&backup.id, true),
            Err(StageRestoreError::InvalidBackup(_))
        ));
        assert_eq!(read_value(&database), "live-value");
        assert_eq!(
            manager.restore_status().expect("restore status").state,
            "none"
        );
    }
}
