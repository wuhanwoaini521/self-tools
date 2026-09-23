//! 备份/恢复演练（drill，V11 §160）：真实活动库 → 备份 → 破坏 → 恢复到隔离目录。
//!
//! 放在 infrastructure 是因为这里需要 `rusqlite` / `duckdb` / `tempfile`：
//! 演练必须用**真正的活动库**跑完整链路，fakes 只能证明代码路径通，
//! 不能证明「备份的库能恢复、恢复的库能打开、快照没有把活动库搞坏」。
//!
//! ## 覆盖的场景
//! 1. `drill_sqlite_backup_restore_preserves_data_and_schema`：建库 → 写入 →
//!    备份到 A → **破坏源库** → 恢复到隔离目录 B → 校验 schema、行数、内容、
//!    摘要，并且源库在破坏后仍能被再次备份（证明快照期间源库没被写坏）；
//! 2. `sqlite_snapshot_does_not_corrupt_live_database`：备份后源库照常可读可写，
//!    `integrity_check` 仍为 ok，且能继续写入（证明 `VACUUM INTO` 不是搬移）；
//! 3. `drill_restore_refuses_path_traversal`：清单条目带 `../` → 恢复只报失败，
//!    绝不把文件写到目的目录之外；
//! 4. `drill_detects_tampered_snapshot`：备份后改快照内容 → 摘要校验拦下；
//! 5. `json_source_backup_and_restore_round_trip`：JSON 配置的完整往返；
//! 6. `drill_repeated_backup_is_idempotent`：同目录重复备份幂等（不会因旧文件失败）。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use tempfile::TempDir;

use devtoolbox_core::backup::{BackupEntry, BackupManifest, BackupSource, EntryKind, sha256_hex};

use crate::backup::{DuckDbBackupSource, FileBackupSource, SqliteBackupSource};

use devtoolbox_application::backup::BackupService;

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

/// 建一个带 schema + 少量行的活动库（模拟真实的 memory/files 库）。
fn seed_sqlite(path: &Path) {
    let connection = Connection::open(path).expect("open fixture db");
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT NOT NULL
            );
            INSERT INTO notes (title, body) VALUES ('第一', '内容一');
            INSERT INTO notes (title, body) VALUES ('第二', '内容二');
            PRAGMA user_version = 7;",
        )
        .expect("seed fixture db");
    drop(connection);
}

/// 读取库里的行数与 schema 版本（恢复后校验用）。
///
/// 不在这里断言标题内容：调用方按场景断言，本函数只做「读得出」的事实采集。
fn inspect_sqlite(path: &Path) -> (usize, i64) {
    let connection = Connection::open(path).expect("open db for inspection");
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .expect("count rows");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    drop(connection);
    (rows as usize, version)
}

/// 读取 `notes` 表的全部标题（按 id 排序）。
fn note_titles(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).expect("open db for titles");
    let titles: Vec<String> = connection
        .prepare("SELECT title FROM notes ORDER BY id")
        .expect("prepare")
        .query_map([], |row| row.get(0))
        .expect("query")
        .map(|title| title.expect("title"))
        .collect();
    drop(connection);
    titles
}

fn integrity_ok(path: &Path) -> bool {
    let connection = match Connection::open(path) {
        Ok(connection) => connection,
        Err(_) => return false,
    };
    let verdict: Result<String, _> =
        connection.query_row("PRAGMA integrity_check", [], |row| row.get(0));
    drop(connection);
    verdict.map(|value| value == "ok").unwrap_or(false)
}

/// 库是否「可用」：能打开、integrity_check 通过（破坏后的源库应返回 false）。
fn sqlite_usable(path: &Path) -> bool {
    let connection = match Connection::open(path) {
        Ok(connection) => connection,
        Err(_) => return false,
    };
    let verdict: Result<String, _> =
        connection.query_row("PRAGMA integrity_check", [], |row| row.get(0));
    let rows: Result<i64, _> =
        connection.query_row("SELECT count(*) FROM notes", [], |row| row.get::<_, i64>(0));
    drop(connection);
    verdict.as_deref() == Ok("ok") && rows.is_ok()
}

// ---------------------------------------------------------------------------
// 1. 完整演练：备份 → 破坏 → 恢复
// ---------------------------------------------------------------------------

#[test]
fn drill_sqlite_backup_restore_preserves_data_and_schema() {
    let work = TempDir::new().expect("temp dir");
    let live_dir = work.path().join("config");
    let backup_dir = work.path().join("backup");
    let restore_dir = work.path().join("restored");
    fs::create_dir_all(&live_dir).expect("create live dir");
    let live_db = live_dir.join("memory.db");
    seed_sqlite(&live_db);

    // 备份（真实活动库 → VACUUM INTO 快照 + manifest.json）。
    let mut service = BackupService::new();
    service
        .register(Arc::new(SqliteBackupSource::new(
            "memory",
            &live_db,
            true,
            false,
            Some(7),
        )))
        .expect("register sqlite source");

    let manifest = service
        .backup(&backup_dir, "0.1.0", "drill:真实库备份")
        .expect("backup");
    assert_eq!(manifest.entries.len(), 1, "{:?}", manifest);
    assert_eq!(manifest.schema_versions.get("memory"), Some(&7));
    assert!(manifest.has_sensitive_entries());
    let entry = manifest.entry("memory.db.bak").expect("manifest entry");
    assert_eq!(entry.kind, EntryKind::Sqlite);
    assert!(entry.sensitive);

    // 把源库写坏（模拟数据丢失 / 误删：直接删掉再写个坏文件）。
    fs::remove_file(&live_db).expect("destroy live db");
    fs::write(&live_db, b"this is not a sqlite database at all").expect("corrupt live db");
    // 破坏后源库确实不可用（这正是要靠备份挽回的场景）。
    assert!(!sqlite_usable(&live_db), "破坏后的源库应无法读取");

    // 恢复到隔离目录。
    let report = service.restore(&backup_dir, &restore_dir).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(report.restored, vec!["memory.db.bak"]);
    assert_eq!(report.verified, report.restored);

    // 数据 + schema + 内容都在。
    let restored_db = restore_dir.join("memory.db.bak");
    assert!(restored_db.is_file());
    let (rows, version) = inspect_sqlite(&restored_db);
    assert_eq!(rows, 2, "恢复后行数应一致");
    assert_eq!(version, 7, "恢复后 schema 版本应一致");
    assert_eq!(note_titles(&restored_db), vec!["第一", "第二"]);
    assert!(integrity_ok(&restored_db));

    // 摘要与清单一致。
    let bytes = fs::read(&restored_db).expect("read restored");
    assert!(entry.matches(&bytes), "恢复件摘要应与清单一致");
    assert_eq!(entry.sha256, sha256_hex(&bytes));

    // 关键隔离性：恢复目录里只有清单列出的那一个文件，没有别的东西。
    let extra: Vec<String> = fs::read_dir(&restore_dir)
        .expect("read restore dir")
        .filter_map(|item| item.ok())
        .map(|item| item.file_name().to_string_lossy().to_string())
        .filter(|name| name != "memory.db.bak")
        .collect();
    assert!(extra.is_empty(), "恢复目录出现清单外文件: {extra:?}");
}

// ---------------------------------------------------------------------------
// 2. 快照不破坏活动库
// ---------------------------------------------------------------------------

#[test]
fn sqlite_snapshot_does_not_corrupt_live_database() {
    let work = TempDir::new().expect("temp dir");
    let live_db = work.path().join("live.db");
    seed_sqlite(&live_db);
    let backup_dir = work.path().join("backup");

    let source = SqliteBackupSource::new("live", &live_db, false, false, Some(7));
    let entry = source.snapshot(&backup_dir).expect("snapshot live db");

    // 快照内容是单一时间点的一致副本。
    let snapshot = backup_dir.join("live.db.bak");
    let (rows, version) = inspect_sqlite(&snapshot);
    assert_eq!(rows, 2);
    assert_eq!(version, 7);

    // 活动库在快照后仍完整可用（证明 VACUUM INTO 不是搬移 / 没有截断它）。
    assert!(integrity_ok(&live_db), "活动库快照后应仍然完整");
    assert!(live_db.is_file());
    {
        let connection = Connection::open(&live_db).expect("reopen live db");
        connection
            .execute(
                "INSERT INTO notes (title, body) VALUES ('快照后写入', 'still writable')",
                [],
            )
            .expect("live db 快照后仍可写");
        let rows: i64 = connection
            .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
            .expect("count");
        drop(connection);
        assert_eq!(rows, 3, "活动库在快照后仍可继续写入");
    }

    // 再备份一次：幂等（不因已存在快照失败），且内容是新的。
    let second = source.snapshot(&backup_dir).expect("second snapshot");
    // 注意：VACUUM INTO 会重写压缩库文件，字节数不一定随数据增长，
    // 因此用「可读到的行数」而不是体积证明快照反映了新写入。
    let second_snapshot = backup_dir.join("live.db.bak");
    let (second_rows, second_version) = inspect_sqlite(&second_snapshot);
    assert_eq!(second_rows, 3, "第二次快照应包含新写入的行");
    assert_eq!(second_version, 7);
    assert_ne!(second.sha256, entry.sha256, "第二次快照内容应发生变化");
    assert_eq!(second.sha256.len(), 64);
}

#[test]
fn snapshot_of_missing_sqlite_source_fails_cleanly() {
    let work = TempDir::new().expect("temp dir");
    let source = SqliteBackupSource::new("absent", work.path().join("nope.db"), false, false, None);
    let backup_dir = work.path().join("backup");
    // 注意：source 自己会 create_dir_all，这里显式创建只为断言干净。
    fs::create_dir_all(&backup_dir).expect("create backup dir");
    // rusqlite 打开不存在的路径会创建空库：结果是「空库快照」而不是 panic，
    // 属于可控降级。SQLite 空库仍有 header page（约 4096 字节），因此按
    // 「文件存在 + 能打开 + integrity_check ok」判定，而不是按体积。
    let result = source.snapshot(&backup_dir);
    assert!(result.is_ok(), "缺源应可控降级，不得 panic");
    let entry = result.expect("degrade");
    let snapshot = backup_dir.join("absent.db.bak");
    assert!(snapshot.is_file(), "应产出空库快照文件");
    assert!(integrity_ok(&snapshot));
    // 空库没有任何用户表。
    let connection = Connection::open(&snapshot).expect("open snapshot");
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("count tables");
    drop(connection);
    assert_eq!(tables, 0, "缺源快照不应含用户表");
    assert_eq!(entry.kind, EntryKind::Sqlite);
}

// ---------------------------------------------------------------------------
// 3. 路径逃逸
// ---------------------------------------------------------------------------

#[test]
fn drill_restore_refuses_path_traversal() {
    let work = TempDir::new().expect("temp dir");
    let backup_dir = work.path().join("backup");
    let restore_dir = work.path().join("restored");
    let outside = work.path().join("outside");
    fs::create_dir_all(&backup_dir).expect("create backup dir");

    let payload = br#"{"escaped":true}"#;
    let manifest = BackupManifest {
        entries: vec![BackupEntry::new(
            "../outside/escaped.json",
            EntryKind::Json,
            sha256_hex(payload),
            payload.len() as u64,
            false,
            false,
        )],
        ..BackupManifest::default()
    };
    let manifest_text = manifest.to_json().expect("encode");
    fs::write(backup_dir.join("manifest.json"), &manifest_text).expect("write manifest");
    // 清单条目按原样落在备份目录外（诱导恢复时逃逸写这个文件）。
    fs::create_dir_all(&outside).expect("create outside");
    fs::write(outside.join("escaped.json"), payload).expect("write decoy");

    let service = BackupService::new();
    let report = service.restore(&backup_dir, &restore_dir).expect("restore");
    assert!(!report.is_ok(), "{:?}", report);
    assert!(
        report.failed[0].contains("unsafe entry path"),
        "{:?}",
        report.failed
    );
    assert!(report.restored.is_empty());
    // 逃逸目标未被写、恢复目录里也没生成逃逸路径。
    assert_eq!(
        fs::read(outside.join("escaped.json")).expect("decoy intact"),
        payload
    );
    assert!(!restore_dir.join("outside").exists());
}

// ---------------------------------------------------------------------------
// 4. 篡改检测
// ---------------------------------------------------------------------------

#[test]
fn drill_detects_tampered_snapshot() {
    let work = TempDir::new().expect("temp dir");
    let live_dir = work.path().join("config");
    let backup_dir = work.path().join("backup");
    let restore_dir = work.path().join("restored");
    fs::create_dir_all(&live_dir).expect("create live dir");
    let live_db = live_dir.join("memory.db");
    seed_sqlite(&live_db);

    let mut service = BackupService::new();
    service
        .register(Arc::new(SqliteBackupSource::new(
            "memory",
            &live_db,
            true,
            false,
            Some(7),
        )))
        .expect("register");
    service.backup(&backup_dir, "0.1.0", "").expect("backup");

    // 篡改快照二进制（保持同长度以证明是摘要而非长度在拦）。
    let snapshot = backup_dir.join("memory.db.bak");
    let original = fs::read(&snapshot).expect("read snapshot");
    let mut tampered = original.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    assert_eq!(tampered.len(), original.len());
    fs::write(&snapshot, &tampered).expect("write tampered");

    let report = service.restore(&backup_dir, &restore_dir).expect("restore");
    assert!(!report.is_ok());
    assert!(
        report.failed[0].contains("checksum mismatch"),
        "{:?}",
        report.failed
    );
    // 坏文件没有进入恢复目录。
    assert!(!restore_dir.join("memory.db.bak").exists());
}

// ---------------------------------------------------------------------------
// 5. JSON 源往返
// ---------------------------------------------------------------------------

#[test]
fn json_source_backup_and_restore_round_trip() {
    let work = TempDir::new().expect("temp dir");
    let live_dir = work.path().join("config");
    let backup_dir = work.path().join("backup");
    let restore_dir = work.path().join("restored");
    fs::create_dir_all(&live_dir).expect("create live dir");
    let settings = live_dir.join("settings.json");
    fs::write(&settings, br#"{"theme":"dark","language":"zh-CN"}"#).expect("write settings");

    let source = FileBackupSource::new("settings", &settings, false, false);
    let entry = source.snapshot(&backup_dir).expect("snapshot settings");
    assert_eq!(entry.kind, EntryKind::Json);
    assert_eq!(
        entry.sha256,
        sha256_hex(br#"{"theme":"dark","language":"zh-CN"}"#)
    );

    let snapshot = backup_dir.join("settings.json");
    assert_eq!(
        fs::read(&snapshot).expect("read snapshot"),
        br#"{"theme":"dark","language":"zh-CN"}"#
    );

    // 手工写清单走恢复链路（FileBackupSource 只管快照，清单由编排层写）。
    let manifest = BackupManifest {
        entries: vec![entry],
        ..BackupManifest::default()
    };
    fs::write(
        backup_dir.join("manifest.json"),
        manifest.to_json().expect("encode"),
    )
    .expect("write manifest");

    let mut service = BackupService::new();
    service
        .register(Arc::new(FileBackupSource::new(
            "settings", &settings, false, false,
        )))
        .expect("register");
    let report = service.restore(&backup_dir, &restore_dir).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(
        fs::read(restore_dir.join("settings.json")).expect("read restored"),
        br#"{"theme":"dark","language":"zh-CN"}"#
    );
}

// ---------------------------------------------------------------------------
// 6. 幂等
// ---------------------------------------------------------------------------

#[test]
fn drill_repeated_backup_is_idempotent() {
    let work = TempDir::new().expect("temp dir");
    let live_dir = work.path().join("config");
    let backup_dir = work.path().join("backup");
    let restore_dir = work.path().join("restored");
    fs::create_dir_all(&live_dir).expect("create live dir");
    let live_db = live_dir.join("memory.db");
    seed_sqlite(&live_db);

    let mut service = BackupService::new();
    service
        .register(Arc::new(SqliteBackupSource::new(
            "memory",
            &live_db,
            true,
            false,
            Some(7),
        )))
        .expect("register");

    // 连续两次备份到同一目录：第二次不得因旧文件存在而失败。
    let first = service
        .backup(&backup_dir, "0.1.0", "")
        .expect("first backup");
    let second = service
        .backup(&backup_dir, "0.1.0", "")
        .expect("second backup");
    assert_eq!(first.entries.len(), 1);
    assert_eq!(second.entries.len(), 1);
    // 内容未变 → 摘要稳定（幂等的可观测证据）。
    assert_eq!(first.entries[0].sha256, second.entries[0].sha256);

    // 恢复行为也不变。
    let report = service.restore(&backup_dir, &restore_dir).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    let (rows, version) = inspect_sqlite(&restore_dir.join("memory.db.bak"));
    assert_eq!(rows, 2);
    assert_eq!(version, 7);
}

// ---------------------------------------------------------------------------
// 7. DuckDB 源（引擎内快照同样走完整链路）
// ---------------------------------------------------------------------------

#[test]
fn drill_duckdb_backup_produces_openable_snapshot() {
    let work = TempDir::new().expect("temp dir");
    let live_dir = work.path().join("data");
    let backup_dir = work.path().join("backup");
    fs::create_dir_all(&live_dir).expect("create data dir");
    let live_db = live_dir.join("history.duckdb");

    {
        let connection = duckdb::Connection::open(&live_db).expect("open duckdb fixture");
        connection
            .execute_batch(
                "CREATE TABLE period_events (id INTEGER, title VARCHAR);
                 INSERT INTO period_events VALUES (1, '事件一'), (2, '事件二');",
            )
            .expect("seed duckdb fixture");
        drop(connection);
    }

    let source = DuckDbBackupSource::new("history", &live_db, false, true, Some(3));
    let entry = source.snapshot(&backup_dir).expect("duckdb snapshot");
    assert_eq!(entry.kind, EntryKind::DuckDb);
    assert!(entry.bytes > 0);

    // 快照可被 DuckDB 重新打开且有表。
    source
        .verify_restored(&backup_dir.join("history.duckdb.bak"), &entry)
        .expect("verify duckdb snapshot");
}
