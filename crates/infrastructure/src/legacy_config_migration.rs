//! Explicit, copy-only import of the former project-local app config directory.
//!
//! Never infer a source from the current directory. The caller must supply a known
//! legacy directory. Secret-bearing settings and arbitrary user-selected roots are
//! deliberately excluded; SQLite files are copied through the existing `VACUUM INTO`
//! snapshot adapter, then integrity-checked before installation.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::backup::SqliteBackupSource;
use devtoolbox_core::backup::BackupSource;

const DATABASES: &[&str] = &[
    "dashboard.db",
    "news.db",
    "travel.db",
    "language.db",
    "geography.db",
    "learning.db",
    "conversations.db",
    "study_boards.db",
    "history_enrichment.db",
    "memory.db",
    "documents.db",
    "files.db",
    "server_actions.db",
];
const MEDIA_RELATIVE: &str = "language/nce";
const MARKER_PREFIX: &str = ".legacy-config-import-v1-";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyConfigMigrationReport {
    /// Relative paths successfully imported.
    pub copied: Vec<String>,
    /// Relative paths retained because the destination item already existed.
    pub skipped: Vec<String>,
    /// A valid completion marker for this source was already present.
    pub already_completed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MigrationMarker {
    version: u32,
    complete: bool,
    copied_count: usize,
    skipped_count: usize,
    completed_at_unix: u64,
}

/// Import known app-owned files from `source` to `target` without moving or
/// overwriting anything. The migration is keyed by canonical source path and is
/// idempotent; interrupted runs can be retried because each file is installed
/// atomically and existing destination files are always preserved.
///
/// `settings.json`, arbitrary `file_roots`/`document_roots`, cache data, downloaded
/// corpora, and the bundled read-only History database are intentionally excluded.
/// Secret-bearing settings must be configured again by the user.
///
/// # Errors
/// Returns an error for an invalid/overlapping path, unsafe symlink, unreadable or
/// corrupt SQLite source, failed copy, or invalid existing completion marker.
pub fn migrate_legacy_config(
    source: &Path,
    target: &Path,
) -> Result<LegacyConfigMigrationReport, String> {
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("inspect explicit legacy config source: {error}"))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err("explicit legacy config source is not a regular directory".to_string());
    }
    let source = source
        .canonicalize()
        .map_err(|error| format!("resolve explicit legacy config directory: {error}"))?;
    fs::create_dir_all(target)
        .map_err(|error| format!("create destination config directory: {error}"))?;
    let target = target
        .canonicalize()
        .map_err(|error| format!("resolve destination config directory: {error}"))?;
    if source.starts_with(&target) || target.starts_with(&source) {
        return Err("legacy source and destination directories overlap".to_string());
    }

    let marker_path = marker_path(&target, &source);
    match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("existing migration marker is not a regular file".to_string());
            }
            let marker: MigrationMarker = serde_json::from_slice(
                &fs::read(&marker_path)
                    .map_err(|error| format!("read migration marker: {error}"))?,
            )
            .map_err(|_| {
                "existing migration marker is invalid; refusing to import again".to_string()
            })?;
            if marker.version != 1 || !marker.complete {
                return Err("existing migration marker is incomplete or unsupported".to_string());
            }
            return Ok(LegacyConfigMigrationReport {
                already_completed: true,
                ..LegacyConfigMigrationReport::default()
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect migration marker: {error}")),
    }

    let mut report = LegacyConfigMigrationReport::default();
    for name in DATABASES {
        let source_path = source.join(name);
        let target_path = target.join(name);
        match fs::symlink_metadata(&source_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("inspect legacy database {name}: {error}")),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(format!("legacy database {name} is not a regular file"));
            }
            Ok(_) => {}
        }
        if target_path.exists() {
            report.skipped.push((*name).to_string());
            continue;
        }
        let installed = snapshot_sqlite_if_absent(&source_path, &target_path)
            .map_err(|error| format!("import legacy database {name}: {error}"))?;
        if installed {
            report.copied.push((*name).to_string());
        } else {
            report.skipped.push((*name).to_string());
        }
    }

    let media_source = source.join(MEDIA_RELATIVE);
    match fs::symlink_metadata(&media_source) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect legacy media directory: {error}")),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("legacy media path is not a regular directory".to_string());
        }
        Ok(_) => copy_media_tree(
            &media_source,
            Path::new(MEDIA_RELATIVE),
            &target,
            &mut report,
        )?,
    }

    write_marker_if_absent(&marker_path, &report)?;
    Ok(report)
}

fn marker_path(target: &Path, source: &Path) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(source.to_string_lossy().as_bytes());
    let suffix = format!("{:x}", digest.finalize());
    target.join(format!("{MARKER_PREFIX}{suffix}.json"))
}

fn snapshot_sqlite_if_absent(source: &Path, target: &Path) -> Result<bool, String> {
    if target.exists() {
        return Ok(false);
    }
    let parent = target.parent().ok_or("destination has no parent")?;
    let staging = tempfile::tempdir_in(parent)
        .map_err(|error| format!("create SQLite snapshot staging directory: {error}"))?;
    let snapshot_source = SqliteBackupSource::new("legacy-migration", source, true, false, None);
    let entry = snapshot_source
        .snapshot(staging.path())
        .map_err(|error| format!("create SQLite consistent snapshot: {error}"))?;
    let staged_db = staging.path().join(&entry.path);
    let connection = Connection::open_with_flags(
        &staged_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("open SQLite snapshot for verification: {error}"))?;
    let verdict: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| format!("check SQLite snapshot integrity: {error}"))?;
    if verdict != "ok" {
        return Err(format!("SQLite integrity_check returned {verdict}"));
    }
    drop(connection);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&staged_db)
            .map_err(|error| format!("inspect SQLite snapshot permissions: {error}"))?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&staged_db, permissions)
            .map_err(|error| format!("restrict SQLite snapshot permissions: {error}"))?;
    }
    install_without_overwrite(&staged_db, target)
}

fn copy_media_tree(
    source_dir: &Path,
    relative_dir: &Path,
    target_root: &Path,
    report: &mut LegacyConfigMigrationReport,
) -> Result<(), String> {
    ensure_target_directories(target_root, relative_dir)?;
    let mut entries = fs::read_dir(source_dir)
        .map_err(|error| format!("read legacy media directory: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("enumerate legacy media directory: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let relative = relative_dir.join(&file_name);
        validate_relative_path(&relative)?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("inspect legacy media item: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "legacy media contains a symlink at {}",
                relative.display()
            ));
        }
        if metadata.is_dir() {
            copy_media_tree(&entry.path(), &relative, target_root, report)?;
        } else if metadata.is_file() {
            let target = target_root.join(&relative);
            ensure_target_directories(target_root, relative.parent().unwrap_or(Path::new("")))?;
            if target.exists() {
                report.skipped.push(relative.to_string_lossy().into_owned());
                continue;
            }
            let installed = copy_regular_file_if_absent(&entry.path(), &target)?;
            if installed {
                report.copied.push(relative.to_string_lossy().into_owned());
            } else {
                report.skipped.push(relative.to_string_lossy().into_owned());
            }
        } else {
            return Err(format!(
                "legacy media contains a non-regular item at {}",
                relative.display()
            ));
        }
    }
    Ok(())
}

fn ensure_target_directories(root: &Path, relative: &Path) -> Result<(), String> {
    validate_relative_path(relative)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("destination path contains an unsafe component".to_string());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("destination media path is not a regular directory".to_string());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(format!("create destination media directory: {error}"));
                    }
                }
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|error| format!("verify destination media directory: {error}"))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("destination media path changed during import".to_string());
                }
            }
            Err(error) => return Err(format!("inspect destination media directory: {error}")),
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("migration path is not a safe relative path".to_string());
    }
    Ok(())
}

fn copy_regular_file_if_absent(source: &Path, target: &Path) -> Result<bool, String> {
    let staging = create_staging_file(target.parent().ok_or("destination has no parent")?)?;
    let result = (|| {
        let mut input =
            File::open(source).map_err(|error| format!("open media source: {error}"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .open(&staging)
            .map_err(|error| format!("open media staging file: {error}"))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|error| format!("read media source: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("write media staging file: {error}"))?;
        }
        output
            .sync_all()
            .map_err(|error| format!("sync media staging file: {error}"))?;
        drop(output);
        install_without_overwrite(&staging, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

fn create_staging_file(parent: &Path) -> Result<PathBuf, String> {
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".legacy-import-{}-{sequence}.tmp",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("create migration staging file: {error}")),
        }
    }
    Err("could not allocate a unique migration staging file".to_string())
}

fn install_without_overwrite(staging: &Path, target: &Path) -> Result<bool, String> {
    let installed = match fs::hard_link(staging, target) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => return Err(format!("install without overwriting destination: {error}")),
    };
    fs::remove_file(staging).map_err(|error| format!("remove migration staging file: {error}"))?;
    Ok(installed)
}

fn write_marker_if_absent(
    target: &Path,
    report: &LegacyConfigMigrationReport,
) -> Result<(), String> {
    if target.exists() {
        return Ok(());
    }
    let parent = target.parent().ok_or("migration marker has no parent")?;
    let staging = create_staging_file(parent)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let marker = MigrationMarker {
        version: 1,
        complete: true,
        copied_count: report.copied.len(),
        skipped_count: report.skipped.len(),
        completed_at_unix: now,
    };
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .open(&staging)
            .map_err(|error| format!("open migration marker staging file: {error}"))?;
        file.write_all(
            &serde_json::to_vec(&marker)
                .map_err(|error| format!("serialize migration marker: {error}"))?,
        )
        .map_err(|error| format!("write migration marker: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync migration marker: {error}"))?;
        drop(file);
        install_without_overwrite(&staging, target).map(|_| ())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::{marker_path, migrate_legacy_config};

    fn seed_db(path: &Path, value: &str) {
        let connection = Connection::open(path).expect("open fixture db");
        connection
            .execute_batch(
                "CREATE TABLE fixture (value TEXT NOT NULL); \
                 INSERT INTO fixture(value) VALUES ('seed');",
            )
            .expect("seed fixture db");
        connection
            .execute("INSERT INTO fixture(value) VALUES (?1)", [value])
            .expect("insert fixture value");
    }

    fn values(path: &Path) -> Vec<String> {
        let connection = Connection::open(path).expect("open migrated db");
        let mut statement = connection
            .prepare("SELECT value FROM fixture ORDER BY rowid")
            .expect("prepare fixture query");
        statement
            .query_map([], |row| row.get(0))
            .expect("query fixture rows")
            .map(Result::unwrap)
            .collect()
    }

    fn dirs() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let temp = TempDir::new().expect("temp root");
        let source = temp.path().join("legacy-config");
        let target = temp.path().join("release-config");
        fs::create_dir_all(&source).expect("legacy directory");
        fs::create_dir_all(&target).expect("target directory");
        (temp, source, target)
    }

    #[test]
    fn copies_allowlisted_sqlite_but_never_settings_and_retains_source() {
        let (_temp, source, target) = dirs();
        let source_db = source.join("learning.db");
        seed_db(&source_db, "source-progress");
        fs::write(
            source.join("settings.json"),
            br#"{"api_key":"must-not-copy"}"#,
        )
        .expect("write sentinel settings");

        let report = migrate_legacy_config(&source, &target).expect("migrate fixture");

        assert_eq!(report.copied, ["learning.db"]);
        assert_eq!(
            values(&target.join("learning.db")),
            ["seed", "source-progress"]
        );
        assert!(source_db.exists());
        assert!(source.join("settings.json").exists());
        assert!(!target.join("settings.json").exists());
    }

    #[test]
    fn active_wal_database_is_snapshotted_consistently() {
        let (_temp, source, target) = dirs();
        let source_db = source.join("learning.db");
        let source_connection = Connection::open(&source_db).expect("open WAL fixture");
        source_connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; \
                 PRAGMA wal_autocheckpoint=0; \
                 CREATE TABLE fixture (value TEXT NOT NULL); \
                 INSERT INTO fixture(value) VALUES ('committed-before-snapshot');",
            )
            .expect("seed WAL fixture");
        source_connection
            .execute(
                "INSERT INTO fixture(value) VALUES ('committed-with-wal-open')",
                [],
            )
            .expect("write committed WAL page");

        let report = migrate_legacy_config(&source, &target).expect("snapshot active WAL DB");

        assert!(report.copied.contains(&"learning.db".to_string()));
        assert_eq!(
            values(&target.join("learning.db")),
            ["committed-before-snapshot", "committed-with-wal-open"]
        );
        assert_eq!(
            values(&source_db),
            ["committed-before-snapshot", "committed-with-wal-open"]
        );
    }

    #[test]
    fn existing_destination_database_is_never_overwritten() {
        let (_temp, source, target) = dirs();
        seed_db(&source.join("learning.db"), "legacy");
        seed_db(&target.join("learning.db"), "newer-user-data");

        let report = migrate_legacy_config(&source, &target).expect("migrate fixture");

        assert!(report.copied.is_empty());
        assert_eq!(report.skipped, ["learning.db"]);
        assert_eq!(
            values(&target.join("learning.db")),
            ["seed", "newer-user-data"]
        );
    }

    #[test]
    fn empty_source_is_a_successful_one_time_noop() {
        let (_temp, source, target) = dirs();
        let first = migrate_legacy_config(&source, &target).expect("empty migration");
        let second = migrate_legacy_config(&source, &target).expect("idempotent retry");
        assert!(first.copied.is_empty());
        assert!(second.already_completed);
    }

    #[test]
    fn corrupt_sqlite_is_not_installed_or_marked_complete() {
        let (_temp, source, target) = dirs();
        fs::write(source.join("travel.db"), b"not a sqlite database").expect("corrupt fixture");

        let error = migrate_legacy_config(&source, &target).expect_err("corrupt DB must fail");
        assert!(error.contains("travel.db"));
        assert!(!target.join("travel.db").exists());
        assert!(!marker_path(&target, &source.canonicalize().unwrap()).exists());
    }

    #[test]
    fn interrupted_run_can_resume_without_replacing_prior_imports() {
        let (_temp, source, target) = dirs();
        seed_db(&source.join("dashboard.db"), "first");
        fs::write(source.join("news.db"), b"broken").expect("broken second fixture");
        assert!(migrate_legacy_config(&source, &target).is_err());
        assert_eq!(values(&target.join("dashboard.db")), ["seed", "first"]);

        fs::remove_file(source.join("news.db")).expect("remove repaired database fixture");
        seed_db(&source.join("news.db"), "second");
        let report = migrate_legacy_config(&source, &target).expect("resume migration");
        assert!(report.skipped.contains(&"dashboard.db".to_string()));
        assert!(report.copied.contains(&"news.db".to_string()));
        assert_eq!(values(&target.join("dashboard.db")), ["seed", "first"]);
        assert_eq!(values(&target.join("news.db")), ["seed", "second"]);
    }

    #[test]
    fn target_with_existing_user_files_is_preserved() {
        let (_temp, source, target) = dirs();
        fs::write(target.join("settings.json"), b"target-settings").expect("target settings");
        seed_db(&source.join("learning.db"), "legacy");
        seed_db(&target.join("learning.db"), "target");
        migrate_legacy_config(&source, &target).expect("migration should preserve files");
        assert_eq!(
            fs::read(target.join("settings.json")).unwrap(),
            b"target-settings"
        );
        assert_eq!(values(&target.join("learning.db")), ["seed", "target"]);
    }

    #[test]
    fn media_import_is_file_by_file_and_never_overwrites() {
        let (_temp, source, target) = dirs();
        let legacy_media = source.join("language/nce");
        fs::create_dir_all(legacy_media.join("lesson-1")).expect("media dirs");
        fs::write(legacy_media.join("lesson-1/audio.mp3"), b"audio-v1").expect("media source");
        let target_media = target.join("language/nce/lesson-1");
        fs::create_dir_all(&target_media).expect("target media dir");
        fs::write(target_media.join("audio.mp3"), b"audio-v2").expect("existing media");
        fs::write(target_media.join("notes.txt"), b"target-only").expect("target-only media");

        let report = migrate_legacy_config(&source, &target).expect("media import");
        assert!(
            report
                .skipped
                .contains(&"language/nce/lesson-1/audio.mp3".to_string())
        );
        assert_eq!(
            fs::read(target_media.join("audio.mp3")).unwrap(),
            b"audio-v2"
        );
        assert_eq!(
            fs::read(target_media.join("notes.txt")).unwrap(),
            b"target-only"
        );
    }

    #[test]
    fn overlapping_source_and_destination_are_rejected() {
        let temp = TempDir::new().expect("temp root");
        let source = temp.path().join("config");
        let target = source.join("release");
        fs::create_dir_all(&target).expect("nested destination");
        assert!(migrate_legacy_config(&source, &target).is_err());
    }

    #[test]
    fn destination_that_is_a_file_fails_without_mutating_source() {
        let temp = TempDir::new().expect("temp root");
        let source = temp.path().join("legacy");
        fs::create_dir(&source).expect("source dir");
        seed_db(&source.join("learning.db"), "kept");
        let target = temp.path().join("not-a-directory");
        fs::write(&target, b"destination sentinel").expect("file destination");
        assert!(migrate_legacy_config(&source, &target).is_err());
        assert_eq!(values(&source.join("learning.db")), ["seed", "kept"]);
        assert_eq!(fs::read(target).unwrap(), b"destination sentinel");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_source_directory_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temp root");
        let real_source = temp.path().join("real-legacy");
        let source_link = temp.path().join("legacy-link");
        let target = temp.path().join("release-config");
        fs::create_dir(&real_source).expect("real legacy directory");
        symlink(&real_source, &source_link).expect("create source directory symlink");

        let error = migrate_legacy_config(&source_link, &target)
            .expect_err("source directory symlink must fail");

        assert!(error.contains("regular directory"));
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_database_is_rejected() {
        use std::os::unix::fs::symlink;

        let (_temp, source, target) = dirs();
        let external = source.parent().unwrap().join("outside.db");
        seed_db(&external, "outside");
        symlink(&external, source.join("learning.db")).expect("create symlink fixture");
        let error = migrate_legacy_config(&source, &target).expect_err("symlink must fail");
        assert!(error.contains("learning.db"));
        assert!(!target.join("learning.db").exists());
    }
}
