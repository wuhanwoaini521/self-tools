//! 会话历史 SQLite 仓储（V11 §96-§101）。
//!
//! - 文件：`config/conversations.db`（gitignored，本机数据）。
//! - 两张表：`conversations`（元数据）+ `conversation_messages`（正文）。
//! - **幂等迁移**：`CREATE TABLE IF NOT EXISTS` + `schema_meta.version`（与
//!   `memory/store.rs` 完全同构）；重复打开不重建、已有数据不丢。
//! - **Conversation ≠ Memory**：本存储是「会话历史」的唯一 Source of Truth，
//!   记忆服务（`MemoryService` / 记忆检索 / `memory.*`）**绝不读取**这里的数据；
//!   记忆也不会写入这里。两个库物理隔离。
//! - **绝不 panic**：损坏行（坏整数、坏 JSON、未知 role、NULL 正文）一律降级为
//!   受控错误或默认值，错误文本不含消息正文、secret 或 token。
//! - **字段白名单**：只持久化 role / content / provider / model / created_at +
//!   会话元数据。隐藏推理（reasoning content）、secret、token、完整 prompt 一律不入库。

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::personal_ai::conversation::{
    Conversation, ConversationMessage, ConversationRole, ConversationSummary,
    DEFAULT_CONVERSATION_TITLE, truncate_content,
};

use crate::error::InfrastructureError;

/// 当前 schema 版本（新增列时递增并补非破坏性迁移）。
pub const CONVERSATION_SCHEMA_VERSION: u32 = 1;

/// SQLite 会话历史仓储。
#[derive(Debug)]
pub struct ConversationSqliteStore {
    connection: Mutex<Connection>,
}

impl ConversationSqliteStore {
    /// 打开（必要时创建）数据库并确保 schema 存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite)?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    /// 内存库（测试用；不落盘）。
    pub fn open_in_memory() -> Result<Self, InfrastructureError> {
        let connection = Connection::open_in_memory()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite)?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    /// 幂等迁移（§86）：重复打开不重建、不丢数据。
    fn migrate(&self) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS conversations (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    module_origin TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    archived INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS conversation_messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    provider TEXT,
                    model TEXT,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_conversation_messages_conversation
                    ON conversation_messages(conversation_id);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .map_err(sqlite)?;
        write_version(&connection, CONVERSATION_SCHEMA_VERSION)
    }

    /// 列表（`updated_at` 倒序；`limit = 0` 视为不设上限）。
    ///
    /// 已归档会话默认隐藏（可恢复语义：归档只是从列表隐藏，不删消息）。
    pub fn list(
        &self,
        limit: usize,
        include_archived: bool,
    ) -> Result<Vec<ConversationSummary>, InfrastructureError> {
        let mut sql = String::from(
            "SELECT c.id, c.title, c.module_origin, c.created_at, c.updated_at,
                    c.archived,
                    (SELECT COUNT(*) FROM conversation_messages m
                      WHERE m.conversation_id = c.id) AS message_count
             FROM conversations c",
        );
        if !include_archived {
            sql.push_str(" WHERE c.archived = 0");
        }
        sql.push_str(" ORDER BY c.updated_at DESC, c.id ASC");
        if limit > 0 {
            sql.push_str(" LIMIT ?1");
        }

        let limit_param = limit as i64;
        let connection = self.connection.lock();
        let mut statement = connection.prepare(&sql).map_err(sqlite)?;
        let mut rows = if limit > 0 {
            statement.query(params![limit_param]).map_err(sqlite)?
        } else {
            statement.query([]).map_err(sqlite)?
        };
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite)? {
            out.push(row_to_summary(row)?);
        }
        Ok(out)
    }

    /// 按 id 读取完整会话（含正文）。不存在返回 `None`；归档会话仍可读。
    pub fn load(&self, conversation_id: &str) -> Result<Option<Conversation>, InfrastructureError> {
        let connection = self.connection.lock();
        let row = connection
            .query_row(
                "SELECT id, title, module_origin, created_at, updated_at, archived
                 FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite)?;
        let Some((id, title, module_origin, created_at, updated_at, archived)) = row else {
            return Ok(None);
        };
        let messages = Self::load_messages(&connection, &id)?;
        Ok(Some(Conversation {
            conversation_id: id,
            title,
            created_at,
            updated_at,
            messages,
            module_origin: module_origin.filter(|value| !value.is_empty()),
            archived: archived != 0,
        }))
    }

    /// 会话内的消息（按自增 id 正序；损坏行降级为可控默认值，不中断整次读取）。
    fn load_messages(
        connection: &Connection,
        conversation_id: &str,
    ) -> Result<Vec<ConversationMessage>, InfrastructureError> {
        let mut statement = connection
            .prepare(
                "SELECT role, content, provider, model, created_at
                 FROM conversation_messages WHERE conversation_id = ?1 ORDER BY id ASC",
            )
            .map_err(sqlite)?;
        let mut rows = statement.query(params![conversation_id]).map_err(sqlite)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite)? {
            out.push(row_to_message(row)?);
        }
        Ok(out)
    }

    /// 创建会话并返回完整会话。
    ///
    /// `title` 空串/空白 → [`DEFAULT_CONVERSATION_TITLE`]；`module_origin` 空串 → `None`。
    /// `now` 由调用方注入，使时间单调性可测（生产传 `now_unix()`）。
    pub fn create(
        &self,
        title: &str,
        module_origin: Option<&str>,
        now: i64,
    ) -> Result<Conversation, InfrastructureError> {
        let title = if title.trim().is_empty() {
            DEFAULT_CONVERSATION_TITLE.to_string()
        } else {
            title.trim().to_string()
        };
        let module_origin = module_origin
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let conversation_id = format!("conv-{now}-{}", next_suffix(now));

        let connection = self.connection.lock();
        connection
            .execute(
                "INSERT INTO conversations (id, title, module_origin, created_at, updated_at, archived)
                 VALUES (?1, ?2, ?3, ?4, ?4, 0)",
                params![conversation_id, title, module_origin, now],
            )
            .map_err(sqlite)?;
        drop(connection);

        let conversation = Conversation {
            conversation_id,
            title,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            module_origin,
            archived: false,
        };
        Ok(conversation)
    }

    /// 追加一条消息（正文超限硬截断到 [`CONVERSATION_MAX_CONTENT_CHARS`] 字符）。
    ///
    /// 同时更新 `conversations.updated_at`（单调不回拨）。
    pub fn append_message(
        &self,
        conversation_id: &str,
        message: &ConversationMessage,
        now: i64,
    ) -> Result<(), InfrastructureError> {
        let content = truncate_content(&message.content);
        let role = message.role.as_str();
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(sqlite)?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(sqlite)?
            .is_some();
        if !exists {
            return Err(InfrastructureError::Sqlite(format!(
                "conversation {conversation_id} does not exist"
            )));
        }
        transaction
            .execute(
                "INSERT INTO conversation_messages
                    (conversation_id, role, content, provider, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    conversation_id,
                    role,
                    content,
                    message.provider,
                    message.model,
                    message.created_at.unwrap_or(now),
                ],
            )
            .map_err(sqlite)?;
        transaction
            .execute(
                "UPDATE conversations SET updated_at = MAX(updated_at, ?1) WHERE id = ?2",
                params![now, conversation_id],
            )
            .map_err(sqlite)?;
        transaction.commit().map_err(sqlite)?;
        Ok(())
    }

    /// 重命名（空标题归一为默认标题）。
    pub fn rename(&self, conversation_id: &str, title: &str) -> Result<(), InfrastructureError> {
        let title = if title.trim().is_empty() {
            DEFAULT_CONVERSATION_TITLE.to_string()
        } else {
            title.trim().to_string()
        };
        self.execute_update(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            params![title, conversation_id],
        )
    }

    /// 归档 / 取消归档（归档不删消息，列表默认隐藏）。
    pub fn set_archived(
        &self,
        conversation_id: &str,
        archived: bool,
    ) -> Result<(), InfrastructureError> {
        self.execute_update(
            "UPDATE conversations SET archived = ?1 WHERE id = ?2",
            params![i64::from(archived), conversation_id],
        )
    }

    /// 物理删除：会话与其全部消息一起删除。
    pub fn delete(&self, conversation_id: &str) -> Result<(), InfrastructureError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(sqlite)?;
        transaction
            .execute(
                "DELETE FROM conversation_messages WHERE conversation_id = ?1",
                params![conversation_id],
            )
            .map_err(sqlite)?;
        transaction
            .execute(
                "DELETE FROM conversations WHERE id = ?1",
                params![conversation_id],
            )
            .map_err(sqlite)?;
        transaction.commit().map_err(sqlite)?;
        Ok(())
    }

    /// 幂等单行更新：不存在时报受控错误（不静默成功）。
    ///
    /// `sql` 必须自带 `WHERE id = ?N`；调用方在 `params` 中按同一序号传 id。
    fn execute_update(
        &self,
        sql: &str,
        values: impl rusqlite::Params,
    ) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        let affected = connection.execute(sql, values).map_err(sqlite)?;
        if affected == 0 {
            return Err(InfrastructureError::Sqlite(
                "conversation does not exist".to_string(),
            ));
        }
        Ok(())
    }
}

/// 会话 id 后缀：进程内单调计数，保证同秒内创建多个会话也不撞 id。
fn next_suffix(now: i64) -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    // 混入时间戳，跨进程/重启后仍可区分。
    (now as u64).wrapping_mul(1_000_003).wrapping_add(sequence)
}

/// 单调版本写入（同 `memory/store.rs`）：数据库比代码新则拒绝静默降级。
fn write_version(connection: &Connection, version: u32) -> Result<(), InfrastructureError> {
    let stored: Option<String> = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite)?;
    match stored {
        None => {
            connection
                .execute(
                    "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![version.to_string()],
                )
                .map_err(sqlite)?;
        }
        Some(value) => {
            let stored: u32 = value.parse().unwrap_or(version);
            if stored > version {
                return Err(InfrastructureError::Sqlite(format!(
                    "conversation schema version {stored} is newer than supported {version}"
                )));
            }
        }
    }
    Ok(())
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> Result<ConversationSummary, InfrastructureError> {
    let archived: i64 = row.get(5).unwrap_or(0);
    let message_count: i64 = row.get(6).unwrap_or(0);
    Ok(ConversationSummary {
        conversation_id: row.get(0)?,
        title: row.get(1)?,
        module_origin: row
            .get::<_, Option<String>>(2)?
            .filter(|value| !value.is_empty()),
        created_at: row.get(3).unwrap_or(0),
        updated_at: row.get(4).unwrap_or(0),
        archived: archived != 0,
        message_count: message_count.max(0) as usize,
    })
}

/// 行 → 消息。**任何一列损坏都不 panic**：未知 role 降级为 `user`，
/// NULL / 非文本正文降级为空串，坏时间戳降级为 0。
fn row_to_message(row: &rusqlite::Row<'_>) -> Result<ConversationMessage, InfrastructureError> {
    let role = row
        .get::<_, Option<String>>(0)?
        .as_deref()
        .and_then(ConversationRole::parse)
        .unwrap_or_default();
    let content = row.get::<_, Option<String>>(1)?.unwrap_or_default();
    Ok(ConversationMessage {
        role,
        content,
        provider: row.get::<_, Option<String>>(2)?,
        model: row.get::<_, Option<String>>(3)?,
        created_at: row.get::<_, Option<i64>>(4)?,
    })
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::personal_ai::conversation::{
        CONVERSATION_MAX_CONTENT_CHARS as MAX_CHARS, truncate_content as truncate,
    };

    fn user(content: &str) -> ConversationMessage {
        ConversationMessage::new(ConversationRole::User, content)
    }

    #[test]
    fn round_trip_create_append_and_load() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store
            .create("杭州两日游", Some("travel"), 1_000)
            .expect("创建成功");
        store
            .append_message(&conversation.conversation_id, &user("帮我规划"), 1_001)
            .expect("追加成功");
        store
            .append_message(
                &conversation.conversation_id,
                &ConversationMessage::new(ConversationRole::Assistant, "第一天西湖")
                    .with_provider("openai-compatible", "step-5-preview"),
                1_002,
            )
            .expect("追加成功");

        let loaded = store
            .load(&conversation.conversation_id)
            .expect("读取成功")
            .expect("会话存在");
        assert_eq!(loaded.title, "杭州两日游");
        assert_eq!(loaded.module_origin.as_deref(), Some("travel"));
        assert_eq!(loaded.created_at, 1_000);
        assert_eq!(loaded.updated_at, 1_002, "追加后 updated_at 前移");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].role, ConversationRole::User);
        assert_eq!(loaded.messages[0].content, "帮我规划");
        assert_eq!(loaded.messages[1].role, ConversationRole::Assistant);
        assert_eq!(
            loaded.messages[1].provider.as_deref(),
            Some("openai-compatible")
        );
        assert_eq!(loaded.messages[1].model.as_deref(), Some("step-5-preview"));
        assert!(!loaded.archived);

        let summaries = store.list(0, false).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].conversation_id, conversation.conversation_id);
        assert_eq!(summaries[0].message_count, 2);
    }

    #[test]
    fn rename_updates_title_and_blank_falls_back_to_default() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("旧标题", None, 10).unwrap();
        store
            .rename(&conversation.conversation_id, "新标题")
            .unwrap();
        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert_eq!(loaded.title, "新标题");

        store.rename(&conversation.conversation_id, "   ").unwrap();
        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert_eq!(loaded.title, DEFAULT_CONVERSATION_TITLE, "空白标题归一");
    }

    #[test]
    fn archive_filters_list_but_keeps_row_loadable() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("旅行计划", None, 10).unwrap();
        store
            .append_message(&conversation.conversation_id, &user("你好"), 11)
            .unwrap();

        store
            .set_archived(&conversation.conversation_id, true)
            .unwrap();
        assert!(
            store.list(0, false).unwrap().is_empty(),
            "列表默认隐藏归档会话"
        );
        let with_archived = store.list(0, true).unwrap();
        assert_eq!(with_archived.len(), 1);
        assert!(with_archived[0].archived);

        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert!(loaded.archived, "归档行仍可读");
        assert_eq!(loaded.messages.len(), 1, "归档不删消息");

        // 恢复。
        store
            .set_archived(&conversation.conversation_id, false)
            .unwrap();
        assert_eq!(store.list(0, false).unwrap().len(), 1);
        assert!(!store.list(0, false).unwrap()[0].archived);
    }

    #[test]
    fn delete_removes_conversation_and_its_messages() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("待删除", None, 10).unwrap();
        store
            .append_message(&conversation.conversation_id, &user("消息"), 11)
            .unwrap();
        let other = store.create("保留", None, 12).unwrap();
        store
            .append_message(&other.conversation_id, &user("另一条"), 13)
            .unwrap();

        store.delete(&conversation.conversation_id).unwrap();
        assert!(store.load(&conversation.conversation_id).unwrap().is_none());

        let remaining = store.list(0, true).unwrap();
        assert_eq!(remaining.len(), 1, "其他会话不受影响");
        assert_eq!(remaining[0].conversation_id, other.conversation_id);

        let connection = store.connection.lock();
        let orphaned: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?1",
                params![conversation.conversation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphaned, 0, "消息必须随会话一起删除");
    }

    #[test]
    fn corrupt_rows_do_not_panic() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        // 直接写「脏」行：未知 role、空白 role、NULL provider/model/created_at、
        // 非法 archived 值、NULL module_origin。
        {
            let connection = store.connection.lock();
            connection
                .execute(
                    "INSERT INTO conversations (id, title, module_origin, created_at, updated_at, archived)
                     VALUES ('bad-1', '脏会话', NULL, 5, 5, 7)",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO conversation_messages
                        (conversation_id, role, content, provider, model, created_at)
                     VALUES ('bad-1', 'system', '', NULL, NULL, 0)",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO conversation_messages
                        (conversation_id, role, content, provider, model, created_at)
                     VALUES ('bad-1', 'ASSISTANT', '升级大小写', NULL, NULL, 1)",
                    [],
                )
                .unwrap();
        }

        let loaded = store.load("bad-1").unwrap().expect("脏行仍可读");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(
            loaded.messages[0].role,
            ConversationRole::User,
            "未知 role 兜底"
        );
        assert_eq!(loaded.messages[0].content, "");
        assert!(loaded.messages[0].provider.is_none(), "NULL provider 归一");
        assert_eq!(loaded.messages[0].created_at, Some(0), "0 时间戳如实保留");
        assert!(loaded.messages[0].model.is_none(), "NULL model 归一");
        assert_eq!(
            loaded.messages[1].role,
            ConversationRole::Assistant,
            "大小写宽容"
        );
        assert!(loaded.module_origin.is_none(), "NULL 来源归一");

        let summaries = store.list(0, true).unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].archived, "非 0 archived 视为已归档");
        assert_eq!(summaries[0].message_count, 2);
    }

    #[test]
    fn append_truncates_overlong_content() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("长文本", None, 10).unwrap();
        store
            .append_message(
                &conversation.conversation_id,
                &user(&"字".repeat(MAX_CHARS + 500)),
                11,
            )
            .unwrap();
        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert_eq!(loaded.messages[0].content.chars().count(), MAX_CHARS);
        assert_eq!(
            loaded.messages[0].content,
            truncate(&"字".repeat(MAX_CHARS + 500))
        );
    }

    #[test]
    fn timestamps_are_monotonic() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("时间", None, 1_000).unwrap();
        assert_eq!(conversation.created_at, conversation.updated_at);

        store
            .append_message(&conversation.conversation_id, &user("一"), 2_000)
            .unwrap();
        store
            .append_message(&conversation.conversation_id, &user("二"), 1_500)
            .unwrap();

        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert_eq!(loaded.created_at, 1_000, "created_at 不被追写");
        assert_eq!(loaded.updated_at, 2_000, "updated_at 不回拨");

        // 消息自身时间戳按调用方值如实保留。
        assert_eq!(loaded.messages[0].created_at, Some(2_000));
        assert_eq!(loaded.messages[1].created_at, Some(1_500));
    }

    #[test]
    fn schema_migration_is_idempotent_and_guards_downgrade() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("conversations.db");
        let conversation_id = {
            let store = ConversationSqliteStore::open(&path).unwrap();
            let conversation = store.create("重启后仍在", None, 10).unwrap();
            store
                .append_message(&conversation.conversation_id, &user("消息"), 11)
                .unwrap();
            conversation.conversation_id
        };
        // 第二次打开：不重建、不丢数据。
        let store = ConversationSqliteStore::open(&path).unwrap();
        let loaded = store.load(&conversation_id).unwrap().expect("行存活");
        assert_eq!(loaded.title, "重启后仍在");
        assert_eq!(loaded.messages.len(), 1);

        // 数据库比代码新 → 受控拒绝，不静默降级。
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute(
                    "UPDATE schema_meta SET value = '999' WHERE key = 'version'",
                    [],
                )
                .unwrap();
        }
        let error = ConversationSqliteStore::open(&path).unwrap_err();
        assert!(
            error.to_string().contains("newer than supported"),
            "升级过的库必须被拒绝，而不是偷偷重建: {error}"
        );
    }

    #[test]
    fn missing_conversation_is_a_controlled_error() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        assert!(store.load("nope").unwrap().is_none());
        assert!(store.append_message("nope", &user("x"), 1).is_err());
        assert!(store.rename("nope", "t").is_err());
        assert!(store.set_archived("nope", true).is_err());
        // 删除幂等：不存在不报错。
        store.delete("nope").unwrap();
    }

    #[test]
    fn created_at_defaults_to_now_when_message_omits_it() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let conversation = store.create("时间戳", None, 10).unwrap();
        let message = ConversationMessage::new(ConversationRole::Tool, "工具输出");
        store
            .append_message(&conversation.conversation_id, &message, 777)
            .unwrap();
        let loaded = store.load(&conversation.conversation_id).unwrap().unwrap();
        assert_eq!(
            loaded.messages[0].created_at,
            Some(777),
            "缺失值跟随操作时间"
        );
        assert_eq!(loaded.updated_at, 777);
    }

    #[test]
    fn list_limit_and_ordering() {
        let store = ConversationSqliteStore::open_in_memory().unwrap();
        let first = store.create("第一", None, 100).unwrap();
        let second = store.create("第二", None, 500).unwrap();
        store
            .append_message(&first.conversation_id, &user("晚"), 900)
            .unwrap();
        store
            .append_message(&second.conversation_id, &user("更晚"), 950)
            .unwrap();

        let ordered = store.list(0, false).unwrap();
        assert_eq!(
            ordered[0].conversation_id, second.conversation_id,
            "updated_at 倒序"
        );
        assert_eq!(store.list(1, false).unwrap().len(), 1);
        assert_eq!(
            store.list(1, false).unwrap()[0].conversation_id,
            ordered[0].conversation_id
        );
    }
}
