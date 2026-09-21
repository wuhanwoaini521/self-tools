//! Documents 索引 SQLite 仓储（V6 Track B）。
//!
//! - 文件：`config/documents.db`（gitignored）。
//! - 两张表：`documents`（元数据）+ `document_chunks`（正文分块）。
//!   与 Files 的 `config/files.db` **物理隔离**（§40：禁止混为一个表）。
//! - 幂等迁移：`CREATE TABLE IF NOT EXISTS` + `schema_meta.version`（§86）。
//! - 粗筛：`LIKE` 词法候选（V6 §49：P0 不引入向量库）；精排在 application。

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, params};

use devtoolbox_core::documents::{
    DocumentChunk, DocumentFingerprint, DocumentHit, DocumentIndexStats, DocumentLocation,
    DocumentMeta, DocumentType, DocumentVisibility,
};

use crate::error::InfrastructureError;

/// Documents 索引错误（infra 本地类型；桌面适配器转成应用端口错误）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct DocumentIndexError(pub String);

/// 当前 schema 版本。
pub const DOCUMENTS_SCHEMA_VERSION: u32 = 1;

/// SQLite 文档索引。
pub struct DocumentIndexSqliteStore {
    connection: Mutex<Connection>,
}

impl DocumentIndexSqliteStore {
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
                "CREATE TABLE IF NOT EXISTS documents (
                    document_id TEXT PRIMARY KEY,
                    root_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    document_type TEXT NOT NULL,
                    path TEXT NOT NULL,
                    relative_path TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    modified_at INTEGER NOT NULL,
                    indexed_at INTEGER NOT NULL,
                    chunk_count INTEGER NOT NULL,
                    content_available INTEGER NOT NULL,
                    visibility TEXT NOT NULL,
                    index_error TEXT
                );
                CREATE TABLE IF NOT EXISTS document_chunks (
                    document_id TEXT NOT NULL,
                    chunk_id TEXT NOT NULL,
                    ordinal INTEGER NOT NULL,
                    text TEXT NOT NULL,
                    section TEXT,
                    page INTEGER,
                    char_start INTEGER NOT NULL,
                    char_end INTEGER NOT NULL,
                    metadata_json TEXT,
                    PRIMARY KEY (document_id, chunk_id)
                );
                CREATE INDEX IF NOT EXISTS idx_documents_root ON documents(root_id);
                CREATE INDEX IF NOT EXISTS idx_chunks_document
                    ON document_chunks(document_id, ordinal);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(sqlite)?;
        write_version(&connection, DOCUMENTS_SCHEMA_VERSION)?;
        Ok(())
    }

    pub fn upsert(&self, meta: &DocumentMeta) -> Result<(), DocumentIndexError> {
        let connection = self.connection.lock();
        connection
            .execute(
                "INSERT INTO documents
                    (document_id, root_id, title, document_type, path, relative_path,
                     size_bytes, modified_at, indexed_at, chunk_count, content_available,
                     visibility, index_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(document_id) DO UPDATE SET
                    root_id = excluded.root_id,
                    title = excluded.title,
                    document_type = excluded.document_type,
                    path = excluded.path,
                    relative_path = excluded.relative_path,
                    size_bytes = excluded.size_bytes,
                    modified_at = excluded.modified_at,
                    indexed_at = excluded.indexed_at,
                    chunk_count = excluded.chunk_count,
                    content_available = excluded.content_available,
                    visibility = excluded.visibility,
                    index_error = excluded.index_error",
                params![
                    meta.document_id,
                    meta.root_id,
                    meta.title,
                    meta.document_type.as_str(),
                    meta.path,
                    meta.relative_path,
                    meta.size_bytes as i64,
                    meta.modified_at,
                    meta.indexed_at,
                    meta.chunk_count as i64,
                    i64::from(meta.content_available),
                    visibility_text(meta.visibility),
                    meta.index_error,
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    pub fn replace_chunks(
        &self,
        document_id: &str,
        chunks: &[DocumentChunk],
    ) -> Result<(), DocumentIndexError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(store_error)?;
        transaction
            .execute(
                "DELETE FROM document_chunks WHERE document_id = ?1",
                params![document_id],
            )
            .map_err(store_error)?;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO document_chunks
                        (document_id, chunk_id, ordinal, text, section, page,
                         char_start, char_end, metadata_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(store_error)?;
            for chunk in chunks {
                statement
                    .execute(params![
                        chunk.document_id,
                        chunk.chunk_id,
                        chunk.ordinal as i64,
                        chunk.text,
                        chunk.location.section,
                        chunk.location.page.map(i64::from),
                        chunk.location.char_start as i64,
                        chunk.location.char_end as i64,
                        chunk.metadata.to_string(),
                    ])
                    .map_err(store_error)?;
            }
        }
        transaction.commit().map_err(store_error)?;
        Ok(())
    }

    pub fn get(&self, document_id: &str) -> Result<Option<DocumentMeta>, DocumentIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(DOCUMENT_SELECT).map_err(store_error)?;
        let mut rows = statement.query(params![document_id]).map_err(store_error)?;
        match rows.next().map_err(store_error)? {
            Some(row) => Ok(Some(row_to_meta(row)?)),
            None => Ok(None),
        }
    }

    pub fn chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>, DocumentIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT document_id, chunk_id, ordinal, text, section, page, char_start,
                        char_end, metadata_json
                 FROM document_chunks WHERE document_id = ?1 ORDER BY ordinal ASC",
            )
            .map_err(store_error)?;
        let mut rows = statement.query(params![document_id]).map_err(store_error)?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            chunks.push(row_to_chunk(row)?);
        }
        Ok(chunks)
    }


    /// 词法粗筛：标题命中（每文档一条）+ 正文 chunk 命中。
    pub fn search_candidates(
        &self,
        keywords: &[String],
        document_type: Option<DocumentType>,
        limit: usize,
    ) -> Result<Vec<DocumentHit>, DocumentIndexError> {
        if keywords.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let connection = self.connection.lock();
        let mut hits: Vec<DocumentHit> = Vec::new();

        // 标题命中
        let mut sql = String::from(DOCUMENT_SELECT);
        sql.push_str(" WHERE (");
        sql.push_str(&like_clause(keywords, "lower(title)"));
        sql.push(')');
        if document_type.is_some() {
            sql.push_str(" AND document_type = ?");
        }
        sql.push_str(" ORDER BY modified_at DESC LIMIT ?");
        let mut values: Vec<String> = keywords
            .iter()
            .map(|keyword| format!("%{}%", keyword.to_lowercase()))
            .collect();
        if let Some(kind) = document_type {
            values.push(kind.as_str().to_string());
        }
        values.push(limit.to_string());
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let params = to_params(&values);
        let mut rows = statement.query(params.as_slice()).map_err(store_error)?;
        while let Some(row) = rows.next().map_err(store_error)? {
            hits.push(DocumentHit {
                meta: row_to_meta(row)?,
                chunk_id: None,
                location: None,
                snippet: String::new(),
                matched_in_title: true,
            });
        }

        // 正文 chunk 命中
        let mut sql = String::from(
            "SELECT d.document_id, d.root_id, d.title, d.document_type, d.path, d.relative_path,
                    d.size_bytes, d.modified_at, d.indexed_at, d.chunk_count,
                    d.content_available, d.visibility, d.index_error,
                    c.chunk_id, c.text, c.section, c.page, c.char_start, c.char_end
             FROM document_chunks c JOIN documents d ON d.document_id = c.document_id
             WHERE (",
        );
        sql.push_str(&like_clause(keywords, "lower(c.text)"));
        sql.push(')');
        if document_type.is_some() {
            sql.push_str(" AND d.document_type = ?");
        }
        sql.push_str(" ORDER BY d.modified_at DESC, c.ordinal ASC LIMIT ?");
        let mut values: Vec<String> = keywords
            .iter()
            .map(|keyword| format!("%{}%", keyword.to_lowercase()))
            .collect();
        if let Some(kind) = document_type {
            values.push(kind.as_str().to_string());
        }
        values.push(limit.to_string());
        let mut statement = connection.prepare(&sql).map_err(store_error)?;
        let params = to_params(&values);
        let mut rows = statement.query(params.as_slice()).map_err(store_error)?;
        while let Some(row) = rows.next().map_err(store_error)? {
            let meta = row_to_meta(row)?;
            let chunk_id: String = row.get(13).map_err(store_error)?;
            let text: String = row.get(14).map_err(store_error)?;
            let section: Option<String> = row.get(15).map_err(store_error)?;
            let page: Option<i64> = row.get(16).map_err(store_error)?;
            let char_start: i64 = row.get(17).map_err(store_error)?;
            let char_end: i64 = row.get(18).map_err(store_error)?;
            let location = DocumentLocation {
                section,
                page: page.map(|value| value as u32),
                char_start: char_start.max(0) as usize,
                char_end: char_end.max(0) as usize,
            };
            hits.push(DocumentHit {
                location: Some(location.describe()),
                meta,
                chunk_id: Some(chunk_id),
                snippet: text,
                matched_in_title: false,
            });
        }
        Ok(hits)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<DocumentMeta>, DocumentIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(&format!("{DOCUMENT_SELECT} ORDER BY indexed_at DESC, document_id ASC LIMIT ?1"))
            .map_err(store_error)?;
        let mut rows = statement.query(params![limit as i64]).map_err(store_error)?;
        let mut metas = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            metas.push(row_to_meta(row)?);
        }
        Ok(metas)
    }

    pub fn fingerprints(
        &self,
        root_id: &str,
    ) -> Result<Vec<DocumentFingerprint>, DocumentIndexError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT document_id, root_id, relative_path, size_bytes, modified_at
                 FROM documents WHERE root_id = ?1",
            )
            .map_err(store_error)?;
        let mut rows = statement.query(params![root_id]).map_err(store_error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(store_error)? {
            let size_bytes: i64 = row.get(3).map_err(store_error)?;
            out.push(DocumentFingerprint {
                document_id: row.get(0).map_err(store_error)?,
                root_id: row.get(1).map_err(store_error)?,
                relative_path: row.get(2).map_err(store_error)?,
                size_bytes: size_bytes.max(0) as u64,
                modified_at: row.get(4).map_err(store_error)?,
            });
        }
        Ok(out)
    }

    pub fn remove(&self, document_id: &str) -> Result<(), DocumentIndexError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(store_error)?;
        transaction
            .execute(
                "DELETE FROM document_chunks WHERE document_id = ?1",
                params![document_id],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "DELETE FROM documents WHERE document_id = ?1",
                params![document_id],
            )
            .map_err(store_error)?;
        transaction.commit().map_err(store_error)?;
        Ok(())
    }

    pub fn stats(&self) -> Result<DocumentIndexStats, DocumentIndexError> {
        let connection = self.connection.lock();
        let documents: i64 = connection
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .map_err(store_error)?;
        let chunks: i64 = connection
            .query_row("SELECT COUNT(*) FROM document_chunks", [], |row| row.get(0))
            .map_err(store_error)?;
        let content_available: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE content_available = 1",
                [],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        let failed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE index_error IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        Ok(DocumentIndexStats {
            documents: documents.max(0) as usize,
            chunks: chunks.max(0) as usize,
            content_available: content_available.max(0) as usize,
            metadata_only: (documents - content_available).max(0) as usize,
            failed: failed.max(0) as usize,
        })
    }
}

const DOCUMENT_SELECT: &str =
    "SELECT document_id, root_id, title, document_type, path, relative_path, size_bytes,
            modified_at, indexed_at, chunk_count, content_available, visibility, index_error
     FROM documents";

fn like_clause(keywords: &[String], column: &str) -> String {
    keywords
        .iter()
        .map(|_| format!("{column} LIKE ?"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn to_params(values: &[String]) -> Vec<&dyn rusqlite::ToSql> {
    values
        .iter()
        .map(|value| value as &dyn rusqlite::ToSql)
        .collect()
}

fn write_version(connection: &Connection, version: u32) -> Result<(), InfrastructureError> {
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
                    params![version.to_string()],
                )
                .map_err(sqlite)?;
        }
        Some(value) => {
            let stored: u32 = value.parse().unwrap_or(version);
            if stored > version {
                return Err(InfrastructureError::Sqlite(format!(
                    "documents schema version {stored} is newer than supported {version}"
                )));
            }
        }
    }
    Ok(())
}

fn visibility_text(visibility: DocumentVisibility) -> &'static str {
    match visibility {
        DocumentVisibility::Normal => "normal",
        DocumentVisibility::Private => "private",
    }
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> Result<DocumentMeta, DocumentIndexError> {
    let document_type: String = row.get(3).map_err(store_error)?;
    let size_bytes: i64 = row.get(6).map_err(store_error)?;
    let chunk_count: i64 = row.get(9).map_err(store_error)?;
    let content_available: i64 = row.get(10).map_err(store_error)?;
    let visibility: String = row.get(11).map_err(store_error)?;
    Ok(DocumentMeta {
        document_id: row.get(0).map_err(store_error)?,
        root_id: row.get(1).map_err(store_error)?,
        title: row.get(2).map_err(store_error)?,
        document_type: DocumentType::parse(&document_type).unwrap_or(DocumentType::Other),
        path: row.get(4).map_err(store_error)?,
        relative_path: row.get(5).map_err(store_error)?,
        size_bytes: size_bytes.max(0) as u64,
        modified_at: row.get(7).map_err(store_error)?,
        indexed_at: row.get(8).map_err(store_error)?,
        chunk_count: chunk_count.max(0) as usize,
        content_available: content_available != 0,
        visibility: if visibility == "private" {
            DocumentVisibility::Private
        } else {
            DocumentVisibility::Normal
        },
        index_error: row.get(12).map_err(store_error)?,
    })
}

fn row_to_chunk(row: &rusqlite::Row<'_>) -> Result<DocumentChunk, DocumentIndexError> {
    let ordinal: i64 = row.get(2).map_err(store_error)?;
    let page: Option<i64> = row.get(5).map_err(store_error)?;
    let char_start: i64 = row.get(6).map_err(store_error)?;
    let char_end: i64 = row.get(7).map_err(store_error)?;
    let metadata_json: Option<String> = row.get(8).map_err(store_error)?;
    Ok(DocumentChunk {
        document_id: row.get(0).map_err(store_error)?,
        chunk_id: row.get(1).map_err(store_error)?,
        ordinal: ordinal.max(0) as usize,
        text: row.get(3).map_err(store_error)?,
        location: DocumentLocation {
            section: row.get(4).map_err(store_error)?,
            page: page.map(|value| value as u32),
            char_start: char_start.max(0) as usize,
            char_end: char_end.max(0) as usize,
        },
        metadata: metadata_json
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(serde_json::Value::Null),
    })
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

fn store_error(error: rusqlite::Error) -> DocumentIndexError {
    DocumentIndexError(error.to_string())
}
