//! Personal Memory SQLite 仓储（V6 Track A）。
//!
//! - 文件：`config/memory.db`（gitignored，本机数据）。
//! - 语义与既有域一致：每域一个 Source of Truth，rusqlite，`Mutex` 串行化。
//! - **幂等迁移**：`CREATE TABLE IF NOT EXISTS` + `schema_meta.version` 单调校验；
//!   重复打开不重建、已有数据不丢（§86）。
//! - **无物理删除**：只提供 upsert / get / query / touch_used / count（§20）。

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, params};

use devtoolbox_core::memory::{
    MemoryCategory, MemoryItem, MemoryQuery, MemorySensitivity, MemorySourceType, MemoryStatus,
};

use crate::error::InfrastructureError;

/// 当前 schema 版本（新增列时递增并补非破坏性迁移）。
pub const MEMORY_SCHEMA_VERSION: u32 = 1;

/// SQLite Memory 仓储。
pub struct MemorySqliteStore {
    connection: Mutex<Connection>,
}

impl MemorySqliteStore {
    /// 打开（必要时创建）数据库并确保 schema 存在。
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

    /// 内存库（测试用；不落盘）。
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
                "CREATE TABLE IF NOT EXISTS memory_items (
                    id TEXT PRIMARY KEY,
                    category TEXT NOT NULL,
                    content TEXT NOT NULL,
                    status TEXT NOT NULL,
                    source_type TEXT NOT NULL,
                    source_reference TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    last_used_at INTEGER,
                    expires_at INTEGER,
                    confidence REAL NOT NULL,
                    sensitivity TEXT NOT NULL,
                    metadata_json TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_memory_status ON memory_items(status);
                CREATE INDEX IF NOT EXISTS idx_memory_category ON memory_items(category);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(sqlite)?;

        let stored: Option<String> = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |row| row.get(0),
            )
            .ok();
        match stored {
            None => {
                connection
                    .execute(
                        "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        params![MEMORY_SCHEMA_VERSION.to_string()],
                    )
                    .map_err(sqlite)?;
            }
            Some(value) => {
                let stored: u32 = value.parse().unwrap_or(MEMORY_SCHEMA_VERSION);
                if stored > MEMORY_SCHEMA_VERSION {
                    // 向前兼容：更新的库不被旧代码静默降级（避免破坏数据）。
                    return Err(InfrastructureError::Sqlite(format!(
                        "memory schema version {stored} is newer than supported {MEMORY_SCHEMA_VERSION}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn upsert(&self, item: &MemoryItem) -> Result<(), InfrastructureError> {
        let metadata_json = serde_json::to_string(&item.metadata).map_err(json_error)?;
        self.connection
            .lock()
            .execute(
                "INSERT INTO memory_items
                    (id, category, content, status, source_type, source_reference,
                     created_at, updated_at, last_used_at, expires_at, confidence,
                     sensitivity, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                    category = excluded.category,
                    content = excluded.content,
                    status = excluded.status,
                    source_type = excluded.source_type,
                    source_reference = excluded.source_reference,
                    updated_at = excluded.updated_at,
                    last_used_at = excluded.last_used_at,
                    expires_at = excluded.expires_at,
                    confidence = excluded.confidence,
                    sensitivity = excluded.sensitivity,
                    metadata_json = excluded.metadata_json",
                params![
                    item.id,
                    item.category.as_str(),
                    item.content,
                    item.status.as_str(),
                    item.source_type.as_str(),
                    item.source_reference,
                    item.created_at,
                    item.updated_at,
                    item.last_used_at,
                    item.expires_at,
                    f64::from(item.confidence),
                    item.sensitivity.as_str(),
                    metadata_json,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryItem>, InfrastructureError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(&format!("{ROW_SELECT} WHERE id = ?1"))
            .map_err(sqlite)?;
        let mut rows = statement.query(params![id]).map_err(sqlite)?;
        match rows.next().map_err(sqlite)? {
            Some(row) => Ok(Some(row_to_item(row)?)),
            None => Ok(None),
        }
    }

    /// 关键词粗筛 + category/status 过滤 + 敏感过滤；`updated_at` 倒序。
    ///
    /// `MemoryQuery::include_sensitive = false` → SQL 层就排除 `sensitive`
    /// （纵深防御，§69）。
    pub fn query(&self, spec: &MemoryQuery) -> Result<Vec<MemoryItem>, InfrastructureError> {
        let mut sql = String::from(ROW_SELECT);
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // 空白分隔关键词，命中任一即候选（与端口文档一致）。
        let tokens: Vec<String> = spec
            .query
            .split_whitespace()
            .map(|token| token.to_lowercase())
            .filter(|token| !token.is_empty())
            .collect();
        if !tokens.is_empty() {
            conditions.push(
                tokens
                    .iter()
                    .map(|_| "lower(content) LIKE ?".to_string())
                    .collect::<Vec<_>>()
                    .join(" OR "),
            );
            for token in &tokens {
                values.push(Box::new(format!("%{token}%")));
            }
        }
        if let Some(category) = spec.category {
            conditions.push("category = ?".to_string());
            values.push(Box::new(category.as_str().to_string()));
        }
        if let Some(status) = spec.status {
            conditions.push("status = ?".to_string());
            values.push(Box::new(status.as_str().to_string()));
        }
        if !spec.include_sensitive {
            conditions.push("sensitivity != 'sensitive'".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY updated_at DESC, id ASC");
        if spec.limit > 0 {
            sql.push_str(" LIMIT ?");
            values.push(Box::new(spec.limit as i64));
        }

        let connection = self.connection.lock();
        let mut statement = connection.prepare(&sql).map_err(sqlite)?;
        let params: Vec<&dyn rusqlite::ToSql> = values
            .iter()
            .map(|value| value.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let mut rows = statement.query(params.as_slice()).map_err(sqlite)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite)? {
            items.push(row_to_item(row)?);
        }
        Ok(items)
    }

    pub fn touch_used(&self, ids: &[String], now: i64) -> Result<(), InfrastructureError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(sqlite)?;
        {
            let mut statement = transaction
                .prepare("UPDATE memory_items SET last_used_at = ?1 WHERE id = ?2")
                .map_err(sqlite)?;
            for id in ids {
                statement.execute(params![now, id]).map_err(sqlite)?;
            }
        }
        transaction.commit().map_err(sqlite)?;
        Ok(())
    }

    pub fn count_by_status(&self) -> Result<Vec<(MemoryStatus, usize)>, InfrastructureError> {
        self.group_counts("status", MemoryStatus::parse)
    }

    pub fn count_by_category(&self) -> Result<Vec<(MemoryCategory, usize)>, InfrastructureError> {
        self.group_counts("category", MemoryCategory::parse)
    }

    fn group_counts<T: Copy>(
        &self,
        column: &str,
        parse: impl Fn(&str) -> Option<T>,
    ) -> Result<Vec<(T, usize)>, InfrastructureError> {
        let sql = format!("SELECT {column}, COUNT(*) FROM memory_items GROUP BY {column}");
        let connection = self.connection.lock();
        let mut statement = connection.prepare(&sql).map_err(sqlite)?;
        let mut rows = statement.query([]).map_err(sqlite)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite)? {
            let raw: String = row.get(0).map_err(sqlite)?;
            let count: i64 = row.get(1).map_err(sqlite)?;
            if let Some(value) = parse(&raw) {
                out.push((value, count.max(0) as usize));
            }
        }
        Ok(out)
    }

    /// 总行数（观测；不含正文）。
    pub fn count(&self) -> Result<usize, InfrastructureError> {
        let connection = self.connection.lock();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM memory_items", [], |row| row.get(0))
            .map_err(sqlite)?;
        Ok(count.max(0) as usize)
    }
}

const ROW_SELECT: &str = "SELECT id, category, content, status, source_type, source_reference,
        created_at, updated_at, last_used_at, expires_at, confidence, sensitivity, metadata_json
    FROM memory_items";

fn row_to_item(row: &rusqlite::Row<'_>) -> Result<MemoryItem, InfrastructureError> {
    let category: String = row.get(1).map_err(sqlite)?;
    let status: String = row.get(3).map_err(sqlite)?;
    let source_type: String = row.get(4).map_err(sqlite)?;
    let sensitivity: String = row.get(11).map_err(sqlite)?;
    let confidence: f64 = row.get(10).map_err(sqlite)?;
    let metadata_json: Option<String> = row.get(12).map_err(sqlite)?;
    Ok(MemoryItem {
        id: row.get(0).map_err(sqlite)?,
        category: MemoryCategory::parse(&category).unwrap_or(MemoryCategory::PersonalFact),
        content: row.get(2).map_err(sqlite)?,
        status: MemoryStatus::parse(&status).unwrap_or(MemoryStatus::Candidate),
        source_type: MemorySourceType::parse(&source_type).unwrap_or(MemorySourceType::System),
        source_reference: row.get(5).map_err(sqlite)?,
        created_at: row.get(6).map_err(sqlite)?,
        updated_at: row.get(7).map_err(sqlite)?,
        last_used_at: row.get(8).map_err(sqlite)?,
        expires_at: row.get(9).map_err(sqlite)?,
        confidence: confidence as f32,
        sensitivity: MemorySensitivity::parse(&sensitivity).unwrap_or(MemorySensitivity::Normal),
        metadata: metadata_json
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(serde_json::Value::Null),
    })
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

fn json_error(error: serde_json::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(format!("cannot encode memory metadata: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::memory::MemoryDraft;

    fn item(
        id: &str,
        content: &str,
        status: MemoryStatus,
        sensitivity: MemorySensitivity,
    ) -> MemoryItem {
        let draft =
            MemoryDraft::new(MemoryCategory::Preference, content).with_sensitivity(sensitivity);
        let mut item = MemoryItem::candidate(id, &draft, 1_000);
        item.status = status;
        item.metadata
            .clone_from(&serde_json::json!({"origin": "test"}));
        item
    }

    #[test]
    fn schema_is_idempotent_and_preserves_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.db");
        {
            let store = MemorySqliteStore::open(&path).unwrap();
            store
                .upsert(&item(
                    "m1",
                    "喜欢历史旅行",
                    MemoryStatus::Active,
                    MemorySensitivity::Normal,
                ))
                .unwrap();
        }
        // 第二次打开：不重建、不丢数据（§86）。
        let store = MemorySqliteStore::open(&path).unwrap();
        let loaded = store.get("m1").unwrap().expect("row survives reopen");
        assert_eq!(loaded.content, "喜欢历史旅行");
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(
            store.count_by_status().unwrap(),
            vec![(MemoryStatus::Active, 1)]
        );
        assert_eq!(
            store.count_by_category().unwrap(),
            vec![(MemoryCategory::Preference, 1)]
        );
    }

    #[test]
    fn upsert_replaces_by_id_without_duplicating() {
        let store = MemorySqliteStore::open_in_memory().unwrap();
        let mut first = item(
            "m1",
            "第一版",
            MemoryStatus::Candidate,
            MemorySensitivity::Normal,
        );
        store.upsert(&first).unwrap();
        first.content = "第二版".to_string();
        first.status = MemoryStatus::Active;
        first.updated_at = 2_000;
        store.upsert(&first).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        let loaded = store.get("m1").unwrap().unwrap();
        assert_eq!(loaded.content, "第二版");
        assert_eq!(loaded.status, MemoryStatus::Active);
        assert_eq!(loaded.metadata["origin"], "test");
        assert_eq!(loaded.confidence, first.confidence);
    }

    #[test]
    fn query_filters_keyword_status_category_and_sensitivity() {
        let store = MemorySqliteStore::open_in_memory().unwrap();
        store
            .upsert(&item(
                "m1",
                "Docker 数据目录是 /Volumes/Data/docker",
                MemoryStatus::Active,
                MemorySensitivity::Normal,
            ))
            .unwrap();
        store
            .upsert(&item(
                "m2",
                "喜欢历史旅行",
                MemoryStatus::Active,
                MemorySensitivity::Normal,
            ))
            .unwrap();
        store
            .upsert(&item(
                "m3",
                "体检记录在协和",
                MemoryStatus::Active,
                MemorySensitivity::Sensitive,
            ))
            .unwrap();
        store
            .upsert(&item(
                "m4",
                "Docker 版本注意升级",
                MemoryStatus::Archived,
                MemorySensitivity::Normal,
            ))
            .unwrap();

        let hits = store
            .query(&MemoryQuery {
                query: "docker".into(),
                status: Some(MemoryStatus::Active),
                ..MemoryQuery::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "m1");

        // 敏感项默认不可见（SQL 层过滤）。
        let all = store.query(&MemoryQuery::default()).unwrap();
        assert_eq!(all.len(), 3);
        assert!(!all.iter().any(|memory| memory.id == "m3"));
        let with_sensitive = store
            .query(&MemoryQuery {
                include_sensitive: true,
                ..MemoryQuery::default()
            })
            .unwrap();
        assert_eq!(with_sensitive.len(), 4);

        // 大小写不敏感（ASCII）。
        let upper = store
            .query(&MemoryQuery {
                query: "DOCKER".into(),
                ..MemoryQuery::default()
            })
            .unwrap();
        assert_eq!(upper.len(), 2);

        // limit 生效。
        let limited = store
            .query(&MemoryQuery {
                limit: 1,
                ..MemoryQuery::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn touch_used_records_timestamp() {
        let store = MemorySqliteStore::open_in_memory().unwrap();
        store
            .upsert(&item(
                "m1",
                "喜欢历史旅行",
                MemoryStatus::Active,
                MemorySensitivity::Normal,
            ))
            .unwrap();
        store.touch_used(&["m1".to_string()], 4_242).unwrap();
        assert_eq!(store.get("m1").unwrap().unwrap().last_used_at, Some(4_242));
        // 不存在的 id 不报错（幂等）。
        store.touch_used(&["missing".to_string()], 5).unwrap();
    }

    #[test]
    fn get_missing_returns_none() {
        let store = MemorySqliteStore::open_in_memory().unwrap();
        assert!(store.get("nope").unwrap().is_none());
    }
}
