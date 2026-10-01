//! SQLite 连接统一配置与初始化辅助函数。
//!
//! 强制启用 WAL 模式、设置 busy_timeout 以及开启外键约束，
//! 避免多模块/并发访问时的写锁竞争与锁库问题。

use std::path::Path;
use std::time::Duration;
use rusqlite::Connection;

/// 默认的 SQLite 忙等待超时时间（5 秒）。
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// 以标准 WAL 配置打开指定路径的 SQLite 数据库。
pub fn open_sqlite<P: AsRef<Path>>(path: P) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    configure_sqlite(&connection)?;
    Ok(connection)
}

/// 以标准配置打开内存中的 SQLite 数据库。
pub fn open_sqlite_in_memory() -> rusqlite::Result<Connection> {
    let connection = Connection::open_in_memory()?;
    configure_sqlite_in_memory(&connection)?;
    Ok(connection)
}

/// 配置磁盘 SQLite 连接：开启 WAL 模式、设置 busy_timeout、开启外键。
pub fn configure_sqlite(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

/// 配置内存 SQLite 连接：设置 busy_timeout、开启外键。
pub fn configure_sqlite_in_memory(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
    connection.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_sqlite_is_configured() {
        let conn = open_sqlite_in_memory().expect("open in memory");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("query foreign_keys");
        assert_eq!(fk, 1);
    }

    #[test]
    fn file_sqlite_is_configured_with_wal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = open_sqlite(&db_path).expect("open file sqlite");
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("query journal_mode");
        assert_eq!(journal_mode.to_lowercase(), "wal");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("query foreign_keys");
        assert_eq!(fk, 1);
    }
}
