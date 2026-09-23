//! BackupService 用例测试（V11 §160）。
//!
//! 全部用内存/临时目录 + `JsonSource` 驱动，**不触 SQLite / DuckDB**：
//! 引擎内快照与恢复的真实演练在 `devtoolbox_infrastructure::backup::drill`。
//!
//! 覆盖：
//! 1. 清单往返（备份 → manifest.json → 恢复读回同一结构）；
//! 2. 篡改检测：改快照内容 → 摘要校验拦下，不写盘；
//! 3. 路径逃逸：`../` 条目被拒绝，绝不写盘；
//! 4. 隔离：备份目录里清单外的无关文件**不会**被带进恢复目录；
//! 5. 目的目录已有同名/异名文件时的行为（只覆盖清单列出的路径）；
//! 6. 单源失败不中止整轮（真实部分备份）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use devtoolbox_core::backup::{BackupEntry, BackupManifest, sha256_hex};

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn temp_dir(label: &str) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("devtoolbox-backup-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create temp dir");
    base
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write file");
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).expect("read file")
}

/// 用 JsonSource 注册一个来源（settings 语义）。
fn register_json(service: &mut BackupService, id: &str, source_dir: &Path, content: &str) {
    let path = source_dir.join(format!("{id}.json"));
    write_file(&path, content);
    service
        .register(Arc::new(JsonSource::new(id, path, false)))
        .expect("register");
}

// ---------------------------------------------------------------------------
// 1. 往返
// ---------------------------------------------------------------------------

#[test]
fn backup_writes_manifest_and_snapshots_then_restore_replays_it() {
    let work = temp_dir("round-trip");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    register_json(&mut service, "settings", &live, r#"{"theme":"dark"}"#);
    register_json(&mut service, "workspace", &live, r#"{"root":"D:/notes"}"#);

    let manifest = service
        .backup(&backup_dir, "0.1.0", "drill-1")
        .expect("backup");
    assert_eq!(manifest.entries.len(), 2);
    assert_eq!(manifest.app_version, "0.1.0");
    assert!(manifest.created_at > 0, "created_at 应记录备份时间");
    assert!(manifest.note.contains("drill-1"));
    assert_eq!(service.source_count(), 2);
    assert_eq!(service.describe_sources().len(), 2);

    // manifest.json 落盘且与返回的结构一致（往返）。
    let on_disk = read_file(&backup_dir.join("manifest.json"));
    let decoded = BackupManifest::from_json(&on_disk).expect("parse manifest");
    assert_eq!(decoded, manifest);
    assert!(decoded.entry("settings.json").is_some());

    // 快照文件存在，且内容与来源一致。
    assert_eq!(
        read_file(&backup_dir.join("settings.json")),
        r#"{"theme":"dark"}"#
    );

    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(report.restored, vec!["settings.json", "workspace.json"]);
    assert_eq!(report.verified, report.restored);
    assert!(report.failed.is_empty());
    assert!(report.integrity_ok);

    // 恢复出来的内容就是原始内容。
    assert_eq!(
        read_file(&restored.join("settings.json")),
        r#"{"theme":"dark"}"#
    );
    assert_eq!(
        read_file(&restored.join("workspace.json")),
        r#"{"root":"D:/notes"}"#
    );

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn empty_service_backup_produces_empty_manifest_and_restore_succeeds() {
    let work = temp_dir("empty");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");

    let service = BackupService::new();
    let manifest = service.backup(&backup_dir, "0.1.0", "").expect("backup");
    assert!(manifest.is_empty());
    assert!(manifest.note.contains("无可用快照"));
    // 空清单也写盘，恢复能明确读出「没有可恢复内容」。
    assert!(backup_dir.join("manifest.json").is_file());

    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok());
    assert!(report.restored.is_empty());

    let _ = fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// 2. 篡改检测
// ---------------------------------------------------------------------------

#[test]
fn restore_rejects_tampered_snapshot_without_writing_it() {
    let work = temp_dir("tamper");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    register_json(&mut service, "settings", &live, r#"{"theme":"dark"}"#);
    service.backup(&backup_dir, "0.1.0", "").expect("backup");

    // 篡改备份目录里的快照内容（模拟损坏 / 恶意改写）。
    write_file(&backup_dir.join("settings.json"), r#"{"theme":"light"}"#);

    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(!report.is_ok());
    assert!(!report.failed.is_empty());
    assert!(
        report.failed[0].contains("checksum mismatch"),
        "{:?}",
        report.failed
    );
    assert!(report.restored.is_empty());
    assert!(!report.integrity_ok);
    // 关键：被篡改的内容没有进入恢复目录。
    assert!(!restored.join("settings.json").exists());

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn restore_rejects_truncated_snapshot_even_if_length_matches_by_accident() {
    let work = temp_dir("length");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    register_json(&mut service, "settings", &live, "0123456789");
    service.backup(&backup_dir, "0.1.0", "").expect("backup");

    // 改成一个同样 10 字节但内容不同的快照：靠摘要拦住（不是长度）。
    write_file(&backup_dir.join("settings.json"), "abcdefghij");

    let report = service
        .restore(&backup_dir, &work.join("restored"))
        .expect("restore");
    assert!(!report.is_ok());
    assert!(
        report.failed[0].contains("checksum mismatch"),
        "{:?}",
        report.failed
    );

    let _ = fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// 3. 路径逃逸
// ---------------------------------------------------------------------------

#[test]
fn restore_refuses_parent_traversal_entry() {
    let work = temp_dir("traversal");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    let outside = work.join("outside");
    fs::create_dir_all(&backup_dir).expect("create backup dir");

    // 手工构造一个含 `../` 的清单（模拟被改写的清单）。
    let entries = vec![BackupEntry::new(
        "../outside/evil.json",
        devtoolbox_core::backup::EntryKind::Json,
        sha256_hex(b"{}"),
        2,
        false,
        false,
    )];
    let manifest = BackupManifest {
        entries,
        ..BackupManifest::default()
    };
    write_file(
        &backup_dir.join("manifest.json"),
        &manifest.to_json().expect("encode"),
    );

    // 清单条目指向的位置放一份「快照」，诱导逃逸。
    write_file(&backup_dir.join("../outside/evil.json"), "{}");
    fs::create_dir_all(&restored).expect("create restored");

    let service = BackupService::new();
    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(!report.is_ok());
    assert!(
        report.failed[0].contains("unsafe entry path"),
        "{:?}",
        report.failed
    );
    // 逃逸目标保持原样（内容未被恢复服务覆盖，也没有被删除）。
    assert_eq!(read_file(&outside.join("evil.json")), "{}");
    assert!(!restored.join("outside").exists());

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn restore_refuses_absolute_path_entry() {
    let work = temp_dir("absolute");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    let victim = work.join("victim.json");
    fs::create_dir_all(&backup_dir).expect("create backup dir");

    let victim_path = victim.display().to_string();
    let manifest = BackupManifest {
        entries: vec![BackupEntry::new(
            victim_path.clone(),
            devtoolbox_core::backup::EntryKind::Json,
            sha256_hex(b"payload"),
            7,
            false,
            false,
        )],
        ..BackupManifest::default()
    };
    write_file(
        &backup_dir.join("manifest.json"),
        &manifest.to_json().expect("encode"),
    );
    // 绝对路径在备份目录里的「快照」位置：直接以同名文件放在 backup 根（不写盘即拒）。
    write_file(&backup_dir.join("victim.json"), "payload");

    let service = BackupService::new();
    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(!report.is_ok());
    assert!(
        report.failed[0].contains("unsafe entry path"),
        "{:?}",
        report.failed
    );
    // 绝对路径受害者未被恢复服务写过。
    assert!(!victim.exists());

    let _ = fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// 4. 隔离
// ---------------------------------------------------------------------------

#[test]
fn restore_only_writes_manifest_entries_and_ignores_extra_files() {
    let work = temp_dir("isolation");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    register_json(&mut service, "settings", &live, r#"{"theme":"dark"}"#);
    service.backup(&backup_dir, "0.1.0", "").expect("backup");

    // 备份目录里混入清单外的文件（恶意 / 误放）——不得被带进恢复目录。
    write_file(&backup_dir.join("secrets.txt"), "should-not-be-restored");
    write_file(&backup_dir.join("nested/extra.bin"), "junk");
    // 恢复目录里预先存在的无关文件：不得被删除或改写。
    write_file(&restored.join("pre-existing.md"), "keep me");

    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    // 只恢复了清单里的那一条。
    assert_eq!(report.restored, vec!["settings.json"]);
    // 无关文件既没被复制进来，也没有被删掉。
    assert!(!restored.join("secrets.txt").exists());
    assert!(!restored.join("nested").exists());
    assert_eq!(read_file(&restored.join("pre-existing.md")), "keep me");

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn restore_overwrites_only_the_listed_path() {
    let work = temp_dir("overwrite");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    register_json(&mut service, "settings", &live, r#"{"theme":"dark"}"#);
    service.backup(&backup_dir, "0.1.0", "").expect("backup");

    // 目的目录已有旧的 settings 与一个无关文件。
    write_file(&restored.join("settings.json"), r#"{"theme":"light"}"#);
    write_file(&restored.join("keep.md"), "untouched");

    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(
        read_file(&restored.join("settings.json")),
        r#"{"theme":"dark"}"#
    );
    assert_eq!(read_file(&restored.join("keep.md")), "untouched");

    let _ = fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// 5. 部分失败
// ---------------------------------------------------------------------------

#[test]
fn single_source_failure_does_not_abort_the_whole_backup() {
    let work = temp_dir("partial");
    let live = work.join("live");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&live).expect("create live");

    let mut service = BackupService::new();
    // 第一个来源指向不存在的文件 → 失败。
    service
        .register(Arc::new(JsonSource::new(
            "missing",
            live.join("missing.json"),
            false,
        )))
        .expect("register missing");
    // 第二个来源正常。
    register_json(&mut service, "settings", &live, r#"{"theme":"dark"}"#);

    let manifest = service
        .backup(&backup_dir, "0.1.0", "drill")
        .expect("backup");
    assert_eq!(manifest.entries.len(), 1);
    assert!(manifest.note.contains("失败来源"));
    assert!(manifest.note.contains("missing"));

    // 恢复只涉及成功的那一条，不受失败源影响。
    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(report.restored, vec!["settings.json"]);

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn duplicate_source_id_is_rejected() {
    let mut service = BackupService::new();
    let base = std::env::temp_dir().join(format!("devtoolbox-backup-dup-{}", std::process::id()));
    fs::create_dir_all(&base).expect("create temp dir");
    write_file(&base.join("settings.json"), "{}");

    service
        .register(Arc::new(JsonSource::new(
            "settings",
            base.join("settings.json"),
            false,
        )))
        .expect("first");
    let error = service.register(Arc::new(JsonSource::new(
        "settings",
        base.join("settings.json"),
        false,
    )));
    assert!(error.is_err());
    assert!(error.unwrap_err().contains("already registered"));

    let _ = fs::remove_dir_all(&base);
}

// ---------------------------------------------------------------------------
// 6. 错误路径
// ---------------------------------------------------------------------------

#[test]
fn restore_without_manifest_fails_cleanly() {
    let work = temp_dir("no-manifest");
    let backup_dir = work.join("backup");
    fs::create_dir_all(&backup_dir).expect("create backup dir");

    let service = BackupService::new();
    let error = service.restore(&backup_dir, &work.join("restored"));
    assert!(error.is_err());
    assert!(error.unwrap_err().contains("read manifest"));

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn restore_with_broken_manifest_fails_cleanly() {
    let work = temp_dir("broken-manifest");
    let backup_dir = work.join("backup");
    fs::create_dir_all(&backup_dir).expect("create backup dir");
    write_file(&backup_dir.join("manifest.json"), "{not json");

    let service = BackupService::new();
    let error = service.restore(&backup_dir, &work.join("restored"));
    assert!(error.is_err());
    assert!(error.unwrap_err().contains("parse manifest"));

    let _ = fs::remove_dir_all(&work);
}

#[test]
fn nested_entry_path_is_restored_inside_destination() {
    let work = temp_dir("nested");
    let backup_dir = work.join("backup");
    let restored = work.join("restored");
    fs::create_dir_all(&backup_dir).expect("create backup dir");

    // 任务合法路径：`config/app.json`（普通组件、留在 dest 内 → 应恢复）。
    let payload = br#"{"nested":true}"#;
    let manifest = BackupManifest {
        entries: vec![BackupEntry::new(
            "config/app.json",
            devtoolbox_core::backup::EntryKind::Json,
            sha256_hex(payload),
            payload.len() as u64,
            false,
            false,
        )],
        ..BackupManifest::default()
    };
    write_file(
        &backup_dir.join("manifest.json"),
        &manifest.to_json().expect("encode"),
    );
    fs::create_dir_all(backup_dir.join("config")).expect("create nested");
    fs::write(backup_dir.join("config/app.json"), payload).expect("write snapshot");

    let service = BackupService::new();
    let report = service.restore(&backup_dir, &restored).expect("restore");
    assert!(report.is_ok(), "{:?}", report);
    assert_eq!(
        read_file(&restored.join("config/app.json")),
        String::from_utf8_lossy(payload)
    );
    // staging 临时文件已改名走，不残留。
    let leftovers: Vec<String> = fs::read_dir(restored.join("config"))
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
        "leftover staging files: {leftovers:?}"
    );

    let _ = fs::remove_dir_all(&work);
}
