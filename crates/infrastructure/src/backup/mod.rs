//! 备份来源实现（V11 §64-§70、§160）。
//!
//! 每个来源实现 `devtoolbox_core::backup::BackupSource`：把当前数据产出一份
//! **自洽快照**到备份目录，返回清单条目（相对路径 + SHA-256 + 体积）。
//!
//! ## 数据库来源一律用引擎内快照，禁止 `std::fs::copy`
//! SQLite / DuckDB 的写事务与页落盘是分步的：裸拷活动库可能截断在
//! 「页已写、WAL / journal 未同步」的中间态，得到一个结构损坏、恢复后
//! `PRAGMA integrity_check` 失败的副本——比没有备份更危险（错误的安全感）。
//!
//! - SQLite：`VACUUM INTO '<dest>'`，SQLite 自己重写一份压缩库，天然避开中间态；
//! - DuckDB：`ATTACH '<dest>' AS __backup_snapshot` +
//!   `COPY FROM DATABASE <主库> TO __backup_snapshot` + `DETACH`（同样是引擎
//!   重写；DuckDB 1.10505 实测一行式 `COPY FROM DATABASE TO '...'` 解析报错）；
//! - 普通文件 / JSON：无写入并发，裸拷贝安全（`FileBackupSource`）。
//!
//! ## 摘要计算
//! 大库不整份进内存：流式分块读 + `Sha256` 更新，`bytes` 同步累计。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use devtoolbox_core::backup::{BackupEntry, BackupSource, BackupTargetDescriptor, EntryKind};
use sha2::{Digest, Sha256};

#[cfg(test)]
pub mod drill;

/// 确保快照目标所在目录存在（`VACUUM INTO` / DuckDB `COPY` / 普通写文件都要求
/// 父目录已就绪：引擎不会代为创建）。
fn ensure_dest_dir(dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create snapshot dir {}: {error}", parent.display()))?;
    }
    Ok(())
}

/// 删除已存在的旧快照（保证重复备份幂等：引擎都拒绝写已有目标）。
fn clear_stale(dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_file(dest)
            .map_err(|error| format!("remove stale snapshot {}: {error}", dest.display()))?;
    }
    Ok(())
}

/// SQLite 活动库的快照实现。
///
/// **不裸拷活动库**：用 `VACUUM INTO` 让 SQLite 引擎自己生成一份单一时间点、
/// 结构完整、并已压缩的副本。`VACUUM INTO` 是单条 SQL，SQLite 保证它读到的
/// 是一致快照（不接受 `?` 参数，目标路径以字面量拼入并转义单引号）。
pub struct SqliteBackupSource {
    id: String,
    path: PathBuf,
    sensitive: bool,
    rebuildable: bool,
    schema_version: Option<u32>,
}

impl SqliteBackupSource {
    /// 构造 SQLite 备份源（不在装配期打开库；打开发生在 `snapshot`）。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        sensitive: bool,
        rebuildable: bool,
        schema_version: Option<u32>,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            sensitive,
            rebuildable,
            schema_version,
        }
    }

    /// 备份目录内的目标文件名（`<id>.db.bak`；相对、普通组件）。
    fn snapshot_name(&self) -> String {
        self.snapshot_file_name()
    }
}

impl BackupSource for SqliteBackupSource {
    fn describe(&self) -> BackupTargetDescriptor {
        BackupTargetDescriptor::new(
            self.id.clone(),
            self.path.display().to_string(),
            EntryKind::Sqlite,
            self.sensitive,
            self.rebuildable,
            self.schema_version,
        )
    }

    fn snapshot(&self, dest_dir: &Path) -> Result<BackupEntry, String> {
        let name = self.snapshot_name();
        let dest = dest_dir.join(&name);
        ensure_dest_dir(&dest)?;
        // VACUUM INTO 不覆盖已有目标：先删旧的，保证重复备份幂等。
        clear_stale(&dest)?;
        let connection = rusqlite::Connection::open(&self.path)
            .map_err(|error| format!("open sqlite source {}: {error}", self.path.display()))?;
        sqlite_vacuum_into(&connection, &dest)?;
        drop(connection);

        entry_for(
            &dest,
            name,
            EntryKind::Sqlite,
            self.sensitive,
            self.rebuildable,
        )
    }

    fn skippable(&self) -> bool {
        // 可重建的库（缓存索引）不进备份：由编排层在「只备份原文」模式下跳过。
        self.rebuildable
    }

    fn verify_restored(&self, restored_path: &Path, entry: &BackupEntry) -> Result<(), String> {
        // 恢复件必须能打开并跑 integrity_check：证明快照不是半成品。
        let connection = rusqlite::Connection::open(restored_path).map_err(|error| {
            format!(
                "reopen restored sqlite {}: {error}",
                restored_path.display()
            )
        })?;
        let verdict: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|error| format!("integrity_check {}: {error}", restored_path.display()))?;
        drop(connection);
        if verdict != "ok" {
            return Err(format!(
                "restored sqlite {} failed integrity_check: {verdict}",
                restored_path.display()
            ));
        }
        // 复核摘要与体积（编排层已校过一次，这里针对来源再兜底）。
        let (bytes, digest) = read_digest(restored_path)?;
        if bytes != entry.bytes || digest != entry.sha256 {
            return Err(format!(
                "restored sqlite {} mismatch (bytes {bytes} != {}, sha {digest} != {})",
                restored_path.display(),
                entry.bytes,
                entry.sha256
            ));
        }
        Ok(())
    }
}

/// 活动 DuckDB 库的快照实现（引擎内 `COPY FROM DATABASE`）。
pub struct DuckDbBackupSource {
    id: String,
    path: PathBuf,
    sensitive: bool,
    rebuildable: bool,
    schema_version: Option<u32>,
}

impl DuckDbBackupSource {
    /// 构造 DuckDB 备份源。活动库不存在时创建空库，使「未配置」来源也能
    /// 参与备份与恢复（§160 宽容降级，不因缺文件整轮失败）。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        sensitive: bool,
        rebuildable: bool,
        schema_version: Option<u32>,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            sensitive,
            rebuildable,
            schema_version,
        }
    }
}

impl BackupSource for DuckDbBackupSource {
    fn describe(&self) -> BackupTargetDescriptor {
        BackupTargetDescriptor::new(
            self.id.clone(),
            self.path.display().to_string(),
            EntryKind::DuckDb,
            self.sensitive,
            self.rebuildable,
            self.schema_version,
        )
    }

    fn snapshot(&self, dest_dir: &Path) -> Result<BackupEntry, String> {
        let name = self.snapshot_file_name();
        let dest = dest_dir.join(&name);
        ensure_dest_dir(&dest)?;
        // DuckDB 的 COPY FROM DATABASE 不覆盖已有目标：先删旧快照保证幂等。
        clear_stale(&dest)?;
        if !self.path.exists() {
            let connection = duckdb::Connection::open(&self.path)
                .map_err(|error| format!("create empty duckdb {}: {error}", self.path.display()))?;
            drop(connection);
        }
        let connection = duckdb::Connection::open(&self.path)
            .map_err(|error| format!("open duckdb source {}: {error}", self.path.display()))?;
        duckdb_copy_into(&connection, &dest)?;
        drop(connection);

        entry_for(
            &dest,
            name,
            EntryKind::DuckDb,
            self.sensitive,
            self.rebuildable,
        )
    }

    fn skippable(&self) -> bool {
        self.rebuildable
    }

    fn verify_restored(&self, restored_path: &Path, _entry: &BackupEntry) -> Result<(), String> {
        // DuckDB 恢复件只需能被引擎重新打开（引擎内元数据校验）；
        // 摘要与体积由编排层统一复核。
        let connection = duckdb::Connection::open(restored_path).map_err(|error| {
            format!(
                "reopen restored duckdb {}: {error}",
                restored_path.display()
            )
        })?;
        let count: i64 = connection
            .query_row("SELECT count(*) FROM duckdb_tables()", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| {
                format!(
                    "inspect restored duckdb {}: {error}",
                    restored_path.display()
                )
            })?;
        drop(connection);
        if count == 0 {
            return Err(format!(
                "restored duckdb {} contains no tables",
                restored_path.display()
            ));
        }
        Ok(())
    }
}

/// 普通文件 / JSON 的快照实现（无写入并发，裸拷贝安全）。
///
/// **不**用于活动 SQLite / DuckDB：那种情况必须走对应的引擎内快照。
pub struct FileBackupSource {
    id: String,
    path: PathBuf,
    sensitive: bool,
    rebuildable: bool,
}

impl FileBackupSource {
    /// 构造文件备份源。`path` 必须存在，否则 `snapshot` 返回受控错误。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        sensitive: bool,
        rebuildable: bool,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            sensitive,
            rebuildable,
        }
    }
}

impl BackupSource for FileBackupSource {
    fn describe(&self) -> BackupTargetDescriptor {
        BackupTargetDescriptor::new(
            self.id.clone(),
            self.path.display().to_string(),
            EntryKind::Json,
            self.sensitive,
            self.rebuildable,
            None,
        )
    }

    fn snapshot(&self, dest_dir: &Path) -> Result<BackupEntry, String> {
        if !self.path.is_file() {
            return Err(format!("backup source missing: {}", self.path.display()));
        }
        let name = self.snapshot_file_name();
        let dest = dest_dir.join(&name);
        ensure_dest_dir(&dest)?;
        let mut source = File::open(&self.path)
            .map_err(|error| format!("open backup source {}: {error}", self.path.display()))?;
        let mut target = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest)
            .map_err(|error| format!("create snapshot {}: {error}", dest.display()))?;
        let copied = stream_copy(&mut source, &mut target)?;
        drop(source);
        target
            .flush()
            .map_err(|error| format!("flush snapshot {}: {error}", dest.display()))?;
        drop(target);

        entry_for(
            &dest,
            name,
            EntryKind::Json,
            self.sensitive,
            self.rebuildable,
        )
        .inspect(|entry| {
            debug_assert_eq!(entry.bytes, copied);
        })
    }

    fn skippable(&self) -> bool {
        self.rebuildable
    }
}

/// 分块读取 + SHA-256 + 体积累计（大文件不整份进内存）。
pub(crate) fn read_digest(path: &Path) -> Result<(u64, String), String> {
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

/// 分块复制文件内容（不整份进内存），返回写入字节数。
pub(crate) fn stream_copy(source: &mut File, target: &mut File) -> Result<u64, String> {
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| format!("read source: {error}"))?;
        if read == 0 {
            break;
        }
        target
            .write_all(&buffer[..read])
            .map_err(|error| format!("write target: {error}"))?;
        total += read as u64;
    }
    Ok(total)
}

/// 依据快照文件构造清单条目（重新读一遍算摘要，保证与内容一致）。
fn entry_for(
    path: &Path,
    relative: impl Into<String>,
    kind: EntryKind,
    sensitive: bool,
    rebuildable: bool,
) -> Result<BackupEntry, String> {
    let (bytes, digest) = read_digest(path)?;
    Ok(BackupEntry::new(
        relative,
        kind,
        digest,
        bytes,
        sensitive,
        rebuildable,
    ))
}

/// 执行 `VACUUM INTO '<dest>'`（SQLite 引擎内一致快照）。
///
/// 语句不接受绑定参数，目标路径以字面量拼入；`'` 按 SQL 字符串规则双写转义。
fn sqlite_vacuum_into(connection: &rusqlite::Connection, dest: &Path) -> Result<(), String> {
    let literal = sql_literal(dest);
    connection
        .execute_batch(&format!("VACUUM INTO '{literal}'"))
        .map_err(|error| format!("vacuum into {}: {error}", dest.display()))
}

/// 执行 DuckDB 引擎内快照。
///
/// DuckDB 没有 `VACUUM INTO` 的一行等价物，可用的是
/// `ATTACH '<dest>' AS __backup_snapshot` +
/// `COPY FROM DATABASE <主库> TO __backup_snapshot` + `DETACH`：同样由引擎
/// 重写整库，避开「裸拷可能截在写事务中间态」的问题（一行式
/// `COPY FROM DATABASE TO '...'` 在 duckdb 1.10505 实测解析报错，不可用）。
///
/// 源库就是连接打开时的主库（`current_database()`），目标库以别名 ATTACH 到
/// 同一连接，因此不额外持有源库的写锁。
///
/// DuckDB 默认关闭外部文件访问，先用 `SET external_access = true` 放开，
/// 并在结束时恢复为 `false`。
fn duckdb_copy_into(connection: &duckdb::Connection, dest: &Path) -> Result<(), String> {
    let dest_literal = sql_literal(dest);
    let source = duckdb_alias(connection);
    let guard = ExternalAccessGuard::enable(connection);

    let script = format!(
        "ATTACH '{dest_literal}' AS __backup_snapshot;\n\
         COPY FROM DATABASE {source} TO __backup_snapshot;\n\
         DETACH __backup_snapshot;"
    );
    let result = connection
        .execute_batch(&script)
        .map_err(|error| format!("duckdb copy into {}: {error}", dest.display()));

    // 无论成败都 DETACH，避免快照文件被本进程继续占用（后续 read_digest 会打不开）。
    let _ = connection.execute_batch("DETACH __backup_snapshot");
    drop(guard);
    result
}

/// 取主库（活动库）的 ATTACH 别名：`current_database()` 即连接打开时用的库。
fn duckdb_alias(connection: &duckdb::Connection) -> String {
    connection
        .query_row("SELECT current_database()", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap_or_else(|_| "memory".to_string())
}

/// 路径 → SQL 字符串字面量（双写单引号）。
fn sql_literal(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

/// DDL 前临时放开 DuckDB 外部文件访问，析构时恢复关闭。
struct ExternalAccessGuard<'a> {
    connection: &'a duckdb::Connection,
}

impl<'a> ExternalAccessGuard<'a> {
    fn enable(connection: &'a duckdb::Connection) -> Self {
        // 失败不致命：仅放开超时/权限不足时 COPY 会自己报错。
        let _ = connection.execute_batch("SET external_access = true");
        Self { connection }
    }
}

impl Drop for ExternalAccessGuard<'_> {
    fn drop(&mut self) {
        let _ = self.connection.execute_batch("SET external_access = false");
    }
}
