//! 备份/恢复端口（V11 §64-§70、§160）。
//!
//! 依赖方向：core 持有契约（数据模型 + 端口），infrastructure 持有实现
//! （SQLite 安全快照 / 文件拷贝），application 持有用例（编排备份与恢复、
//! 演练报告）。core 本身不做任何 IO，也不依赖 rusqlite / std::fs。
//!
//! ## 备份目标（`BackupSource`）
//! 每个可备份的本机数据源（SQLite / DuckDB / JSON）实现本端口：描述自身，
//! 并把一份**自洽快照**落到备份目录，返回与清单字段同形的条目。
//!
//! ## 对 SQLite 快照的硬性要求（禁止裸拷活动库）
//! 实现**禁止**对活动库做 `std::fs::copy`：SQLite 的写事务与落盘是分步的，
//! 裸拷可能截断在「页已写、WAL / journal 未同步」的中间态，得到一个结构损坏、
//! 恢复后 `PRAGMA integrity_check` 失败的副本——比没有备份更危险（错误的安全感）。
//!
//! 必须二选一：
//! 1. **rusqlite Backup API**（`Connection::backup` / `Backup::run_to_completion`）：
//!    在线完成，只需源库的读锁，复制的是已提交页面，最安全；
//! 2. **`VACUUM INTO '<目标>'`**：SQLite 自己重写一份压缩库文件，天然避开中间态。
//!
//! 两种方式都由 SQLite 引擎保证快照是单一时间点、结构完整的。

pub mod model;

pub use model::{
    BackupEntry, BackupManifest, EntryKind, RestoreReport, is_safe_backup_path, sha256_hex,
};

use std::path::Path;

/// 备份来源描述（备份前展示 / 巡检用，不含文件内容）。
///
/// `id` 稳定且与 `BackupManifest::schema_versions` 的键一致，便于按来源核对兼容性。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupTargetDescriptor {
    /// 来源标识（如 `memory`、`files`、`settings`）。
    pub id: String,
    /// 活动库 / 文件的绝对路径（展示与定位；不写入清单条目）。
    pub path: String,
    /// 存储类型（决定快照方式）。
    pub kind: EntryKind,
    /// 是否可能含敏感数据（外发前需显式确认）。
    pub sensitive: bool,
    /// 是否可重建（索引 / 缓存类默认不进备份）。
    pub rebuildable: bool,
    /// 该来源当前 schema 版本（`None` = 不适用，例如 JSON 配置）。
    pub schema_version: Option<u32>,
}

impl BackupTargetDescriptor {
    /// 构造描述（路径以给定文本展示）。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        path: impl Into<String>,
        kind: EntryKind,
        sensitive: bool,
        rebuildable: bool,
        schema_version: Option<u32>,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            kind,
            sensitive,
            rebuildable,
            schema_version,
        }
    }
}

/// 备份来源端口：描述自身 + 产出一份安全快照。
///
/// 实现必须线程安全（备份编排可并发快照），并且**不修改活动数据**：
/// 纯文件来源只读，SQLite 来源用读事务快照（见模块文档）。
pub trait BackupSource: Send + Sync {
    /// 自身描述（路径展示、类型、敏感 / 可重建标记、schema 版本）。
    fn describe(&self) -> BackupTargetDescriptor;

    /// 把当前状态的安全快照写入 `dest_dir`，返回清单条目（相对路径）。
    ///
    /// 约定：
    /// - 目标文件名由实现决定，但必须是 `dest_dir` 内的相对路径（普通组件）；
    /// - 实现**不**写 `manifest.json`（由编排层统一生成，避免重复计摘要）；
    /// - 失败返回可读文本（缺文件 / 权限 / 快照中止），由编排层记入失败项。
    ///
    /// # Errors
    /// 快照失败（来源不存在、库无法打开、目标不可写、快照中止）时返回文本。
    fn snapshot(&self, dest_dir: &Path) -> Result<BackupEntry, String>;

    /// 备份时是否跳过（默认不跳过）。
    ///
    /// `rebuildable = true` 的来源可返回 `true`，让「只备份原文」的快速备份
    /// 自动排除索引 / 缓存。
    fn skippable(&self) -> bool {
        false
    }

    /// 该来源在备份目录里写出的文件名（默认按 `id` + `kind` 约定；实现可覆盖）。
    ///
    /// 恢复侧按**精确相等**把清单条目映射回来源（不用前缀匹配：`settings`
    /// 不应匹配到 `settings2.json`），因此默认值与实现内实际使用的名字必须一致。
    fn snapshot_file_name(&self) -> String {
        let descriptor = self.describe();
        match descriptor.kind {
            EntryKind::Sqlite => format!("{}.db.bak", descriptor.id),
            EntryKind::DuckDb => format!("{}.duckdb.bak", descriptor.id),
            EntryKind::Json => format!("{}.json", descriptor.id),
            EntryKind::Other => format!("{}.bin", descriptor.id),
        }
    }

    /// 恢复后校验条目（默认接受；SQLite 源可覆盖为跑 `integrity_check`）。
    ///
    /// # Errors
    /// 恢复后的文件打不开 / 校验失败时返回文本，由编排层记入失败项。
    fn verify_restored(&self, _restored_path: &Path, _entry: &BackupEntry) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_carries_all_relevant_metadata() {
        let descriptor = BackupTargetDescriptor::new(
            "memory",
            "D:/data/config/memory.db",
            EntryKind::Sqlite,
            true,
            false,
            Some(1),
        );
        assert_eq!(descriptor.id, "memory");
        assert_eq!(descriptor.path, "D:/data/config/memory.db");
        assert_eq!(descriptor.kind, EntryKind::Sqlite);
        assert!(descriptor.sensitive);
        assert!(!descriptor.rebuildable);
        assert_eq!(descriptor.schema_version, Some(1));
    }

    #[test]
    fn backup_source_defaults_accept_everything() {
        struct NoopSource;
        impl BackupSource for NoopSource {
            fn describe(&self) -> BackupTargetDescriptor {
                BackupTargetDescriptor::new("noop", "noop.json", EntryKind::Json, false, true, None)
            }
            fn snapshot(&self, _dest_dir: &Path) -> Result<BackupEntry, String> {
                Err("not implemented".to_string())
            }
        }

        let source = NoopSource;
        assert!(!source.skippable());
        let entry = BackupEntry::new(
            "noop.json",
            EntryKind::Json,
            sha256_hex(b"{}"),
            2,
            false,
            false,
        );
        assert!(
            source
                .verify_restored(Path::new("noop.json"), &entry)
                .is_ok()
        );
    }
}
