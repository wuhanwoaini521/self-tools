//! Files 索引 SQLite 仓储（V6 Track C）。
//!
//! - 文件：`config/files.db`（gitignored），与 `config/documents.db` **物理隔离**（§40）。
//! - 只存元数据（文件名/扩展名/大小/时间/内容类别/是否受限），**绝不缓存正文**（§47/§94）。
//! - 幂等迁移：`CREATE TABLE IF NOT EXISTS` + `schema_meta.version`（§86）。

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, params};

use devtoolbox_core::files::{
    FileContentKind, FileFingerprint, FileIndexStats, FileMetadata, FileQuery,
};

use crate::error::InfrastructureError;

/// Files 索引错误（infra 本地类型；桌面适配器转成应用端口错误）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FileIndexError(pub String);

/// 当前 schema 版本。
pub const FILES_SCHEMA_VERSION: u32 = 1;

/// SQLite 文件索引。
pub struct FileIndexSqliteStore {
    connection: Mutex<Connection>,
}

impl FileIndexSqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, InfrastructureError> {
        let connection = Connection::open_in_memory()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS file_entries (
                    file_id TEXT PRIMARY KEY,
                    root_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    relative_path TEXT NOT NULL,
                    file_name TEXT NOT NULL,
                    extension TEXT,
                    size_bytes INTEGER NOT NULL,
                    modified_at INTEGER NOT NULL,
                    indexed_at INTEGER NOT NULL,
                    content_kind TEXT NOT NULL,
                    restricted INTEGER NOT NULL DEFAULT 0,
                    index_error TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_files_root ON file_entries(root_id);
                CREATE INDEX IF NOT EXISTS idx_files_modified ON file_entries(modified_at);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(sqlite)?;
        let stored: Option<String> = connection
            .query_row("SELECT value FROM schema_meta WHERE key = 'version'", [], |row| {
                row.get(0)
            })
            .ok();
        match stored {
            None => {
                connection
                    .execute(
                        "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)",
                        params![FILES_SCHEMA_VERSION.to_string()],
                    )
                    .map_err(sqlite)?;
            }
            Some(value) => {
                let stored: u32 = value.parse().unwrap_or(FILES_SCHEMA_VERSION);
                if stored > FILES_SCHEMA_VERSION {
                    return Err(InfrastructureError::Sqlite(format!(
                        "files schema version {stored} is newer than supported {FILES_SCHEMA_VERSION}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn upsert_many(&self, entries: &[FileMetadata]) -> Result<(), FileIndexError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(store_error)?;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO file_entries
                        (file_id, root_id, path, relative_path, file_name, extension, size_bytes,
                         modified_at, indexed_at, content_kind, restricted, index_error)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                     ON CONFLICT(file_id) DO UPDATE SET
                        root_id = excluded.root_id,
                        path = excluded.path,
                        relative_path = excluded.relative_path,
                        file_name = excluded.file_name,
                        extension = excluded.extension,
                        size_bytes = excluded.size_bytes,
                        modified_at = excluded.modified_at,
                        indexed_at = excluded.indexed_at,
                        content_kind = excluded.content_kind,
                        restricted = excluded.restricted,
                        index_error = excluded.index_error",
                )
                .map_err(store_error)?;
            for entry in entries {
                statement
                    .execute(params![
                        entry.file_id,
                        entry.root_id,
                        entry.path,
                        entry.relative_path,
                        entry.file_name,
                        entry.extension,
                        entry.size_bytes as i64,
                        entry.modified_at,
                        entry.indexed_at,
                        content_kind_text(entry.content_kind),
                        i64::from(entry.restricted),
                        entry.index_error,
                    ])
                    .map_err(store_error)?;
            }
        }
        transaction.commit().map_err(store_error)?;
        Ok(())
    }

    pub fn get(&self, file_id: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(&format!("{FILE_SELECT} WHERE file_id = ?1"))
            .map_err(store_error)?;
        let mut rows = statement.query(params![file_id]).map_err(store_error)?;
        match rows.next().map_err(store_error)? {
            Some(row) => Ok(Some(row_to_entry(row)?)),
            None => Ok(None),
        }
    }

    pub fn find_by_path(&self, path: &str) -> Result<Option<FileMetadata>, FileIndexError> {
        let connection = self.connection.lock();
        let candidates = [
            path.to_string(),
            devtoolbox_core::files::display_path(path),
        ];
        for candidate in candidates {
            let mut statement = connection
                .prepare(&format!("{FILE_SELECT} WHERE path = ?1"))
                .map_err(store_error)?;
            let mut rows = statement.query(params![candidate]).map_err(store_error)?;
            if let Some(row) = rows.next().map_err(store_error)? {
                return Ok(Some(row_to_entry(row)?));
            }
        }
        Ok(None)
    }

    pub fn search(&self, spec: &FileQuery) -> Result<Vec<FileMetadata>, FileIndexError> {
        let mut sql = String::from(FILE_SELECT);
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for keyword in keywords_for(&spec.query) {
            conditions.push("lower(relative_path) LIKE ?".to_string());
            values.push(Box::new(format!("%{keyword}%")));
        }
        if let Some(extension) = spec.extension.as_deref().filter(|value| !value.is_empty()) {
            conditions.push("lower(extension) = ?".to_string());
            values.push(Box::new(extension.trim_start_matches('.').to_ascii_lowercase()));
        }
        if let Some(root_id) = spec.root_id.as_deref().filter(|value| !value.is_empty()) {
            conditions.push("root_id = ?".to_string());
            values.push(Box::new(root_id.to_string()));
        }
        if let Some(threshold) = spec.modified_after {
            conditions.push("modified_at >= ?".to_string());
            values.push(Box::new(threshold));
        }
        if !spec.include_restricted {
            conditions.push("restricted = 0".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY modified_at DESC, relative_path ASC LIMIT ?");
        values.push(Box::new(spec.limit.max(1) as i64));

        let connection = self.connection.lock();
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let params: Vec<&dyn rusqlite::ToSql> = values
            .iter()
            .map(|value| value.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let mut rows = statement.query(params.as_slice()).map_err(store_error)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            entries.push(row_to_entry(row)?);
        }
        Ok(entries)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, FileIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(&format!(
                "{FILE_SELECT} ORDER BY indexed_at DESC, relative_path ASC LIMIT ?1"
            ))
            .map_err(store_error)?;
        let mut rows = statement.query(params![limit.max(1) as i64]).map_err(store_error)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            entries.push(row_to_entry(row)?);
        }
        Ok(entries)
    }

    pub fn fingerprints(&self, root_id: &str) -> Result<Vec<FileFingerprint>, FileIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT file_id, root_id, relative_path, size_bytes, modified_at
                 FROM file_entries WHERE root_id = ?1",
            )
            .map_err(store_error)?;
        let mut rows = statement.query(params![root_id]).map_err(store_error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            let size_bytes: i64 = row.get(3).map_err(store_error)?;
            out.push(FileFingerprint {
                file_id: row.get(0).map_err(store_error)?,
                root_id: row.get(1).map_err(store_error)?,
                relative_path: row.get(2).map_err(store_error)?,
                size_bytes: size_bytes.max(0) as u64,
                modified_at: row.get(4).map_err(store_error)?,
            });
        }
        Ok(out)
    }

    pub fn remove(&self, file_id: &str) -> Result<(), FileIndexError> {
        self.connection
            .lock()
            .execute("DELETE FROM file_entries WHERE file_id = ?1", params![file_id])
            .map_err(store_error)?;
        Ok(())
    }

    pub fn stats(&self) -> Result<FileIndexStats, FileIndexError> {
        let connection = self.connection.lock();
        let count = |sql: &str| -> Result<usize, FileIndexError> {
            let value: i64 = connection
                .query_row(sql, [], |row| row.get(0))
                .map_err(store_error)?;
            Ok(value.max(0) as usize)
        };
        Ok(FileIndexStats {
            files: count("SELECT COUNT(*) FROM file_entries")?,
            text_files: count("SELECT COUNT(*) FROM file_entries WHERE content_kind = 'text'")?,
            binary_files: count("SELECT COUNT(*) FROM file_entries WHERE content_kind = 'binary'")?,
            restricted: count("SELECT COUNT(*) FROM file_entries WHERE restricted = 1")?,
            failed: count("SELECT COUNT(*) FROM file_entries WHERE index_error IS NOT NULL")?,
        })
    }
}

const FILE_SELECT: &str =
    "SELECT file_id, root_id, path, relative_path, file_name, extension, size_bytes,
            modified_at, indexed_at, content_kind, restricted, index_error
     FROM file_entries";

/// 关键词拆分（复用 application 的规则；此处保持最简：空白 + 大小写归一）。
pub(crate) fn keywords_for(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

fn content_kind_text(kind: FileContentKind) -> &'static str {
    match kind {
        FileContentKind::Text => "text",
        FileContentKind::Binary => "binary",
        FileContentKind::Unknown => "unknown",
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<FileMetadata, FileIndexError> {
    let size_bytes: i64 = row.get(6).map_err(store_error)?;
    let content_kind: String = row.get(9).map_err(store_error)?;
    let restricted: i64 = row.get(10).map_err(store_error)?;
    Ok(FileMetadata {
        file_id: row.get(0).map_err(store_error)?,
        root_id: row.get(1).map_err(store_error)?,
        path: row.get(2).map_err(store_error)?,
        relative_path: row.get(3).map_err(store_error)?,
        file_name: row.get(4).map_err(store_error)?,
        extension: row.get(5).map_err(store_error)?,
        size_bytes: size_bytes.max(0) as u64,
        modified_at: row.get(7).map_err(store_error)?,
        indexed_at: row.get(8).map_err(store_error)?,
        content_kind: match content_kind.as_str() {
            "text" => FileContentKind::Text,
            "binary" => FileContentKind::Binary,
            _ => FileContentKind::Unknown,
        },
        restricted: restricted != 0,
        index_error: row.get(11).map_err(store_error)?,
    })
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

fn store_error(error: rusqlite::Error) -> FileIndexError {
    FileIndexError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::files::file_id;

    fn entry(root_id: &str, relative: &str, restricted: bool) -> FileMetadata {
        FileMetadata {
            file_id: file_id(root_id, relative),
            root_id: root_id.to_string(),
            path: format!("/data/{relative}"),
            relative_path: relative.to_string(),
            file_name: devtoolbox_core::files::file_name_of(relative),
            extension: devtoolbox_core::files::extension_of(relative),
            size_bytes: 100,
            modified_at: 1_000,
            indexed_at: 2_000,
            content_kind: FileContentKind::Text,
            restricted,
            index_error: None,
        }
    }

    #[test]
    fn schema_is_idempotent_and_rows_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("files.db");
        {
            let store = FileIndexSqliteStore::open(&path).unwrap();
            store
                .upsert_many(&[entry("root", "notes/a.md", false)])
                .unwrap();
        }
        let store = FileIndexSqliteStore::open(&path).unwrap();
        assert_eq!(store.stats().unwrap().files, 1);
        assert!(store.get(&file_id("root", "notes/a.md")).unwrap().is_some());
    }

    #[test]
    fn upsert_is_idempotent_by_file_id() {
        let store = FileIndexSqliteStore::open_in_memory().unwrap();
        let mut first = entry("root", "notes/a.md", false);
        store.upsert_many(&[first.clone()]).unwrap();
        first.size_bytes = 999;
        first.indexed_at = 3_000;
        store.upsert_many(&[first.clone()]).unwrap();
        assert_eq!(store.stats().unwrap().files, 1);
        assert_eq!(store.get(&first.file_id).unwrap().unwrap().size_bytes, 999);
    }

    #[test]
    fn search_filters_and_never_leaks_restricted_by_default() {
        let store = FileIndexSqliteStore::open_in_memory().unwrap();
        store
            .upsert_many(&[
                entry("root", "notes/korea-travel.pdf", false),
                entry("root", "notes/docker.md", false),
                entry("root", "secrets/.env", true),
            ])
            .unwrap();

        let by_name = store
            .search(&FileQuery {
                query: "korea".into(),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].relative_path, "notes/korea-travel.pdf");

        let by_extension = store
            .search(&FileQuery {
                extension: Some("pdf".into()),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert_eq!(by_extension.len(), 1);

        // 受限文件默认不可见；显式要求才出现（且仍被上层拒绝读取）。
        let default_hits = store
            .search(&FileQuery {
                query: ".env".into(),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert!(default_hits.is_empty());
        let with_restricted = store
            .search(&FileQuery {
                query: ".env".into(),
                limit: 10,
                include_restricted: true,
                ..FileQuery::default()
            })
            .unwrap();
        assert_eq!(with_restricted.len(), 1);
        assert!(with_restricted[0].restricted);
        assert_eq!(store.stats().unwrap().restricted, 1);
    }

    #[test]
    fn search_respects_root_extension_and_time_filters() {
        let store = FileIndexSqliteStore::open_in_memory().unwrap();
        let mut older = entry("root", "old/a.md", false);
        older.modified_at = 10;
        store
            .upsert_many(&[entry("root", "new/b.md", false), older, entry("other", "c.md", false)])
            .unwrap();

        let by_root = store
            .search(&FileQuery {
                root_id: Some("root".into()),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert_eq!(by_root.len(), 2);

        let recent = store
            .search(&FileQuery {
                modified_after: Some(100),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert_eq!(recent.len(), 2);

        let none = store
            .search(&FileQuery {
                extension: Some("docx".into()),
                limit: 10,
                ..FileQuery::default()
            })
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn fingerprints_remove_and_recent_work() {
        let store = FileIndexSqliteStore::open_in_memory().unwrap();
        store
            .upsert_many(&[entry("root", "a.md", false), entry("root", "b.md", false)])
            .unwrap();
        let fingerprints = store.fingerprints("root").unwrap();
        assert_eq!(fingerprints.len(), 2);
        assert!(fingerprints.iter().all(|entry| entry.root_id == "root"));

        store.remove(&file_id("root", "a.md")).unwrap();
        assert_eq!(store.stats().unwrap().files, 1);

        let recent = store.recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].relative_path, "b.md");
    }
}
