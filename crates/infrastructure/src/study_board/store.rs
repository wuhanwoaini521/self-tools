//! Study Board（学习板）SQLite 仓储（V11 §112）。
//!
//! - 文件：`config/study_board.db`（gitignored，本机数据）。
//! - 与 `MemorySqliteStore` 同构：rusqlite + `parking_lot::Mutex` 串行化、
//!   `CREATE TABLE IF NOT EXISTS` 幂等迁移、损坏行返回受控错误（不 panic）。
//! - **隐私铁律（V11 §113/§114）**：本模块只做存取，错误消息只含稳定 reason，
//!   绝不把 `strokes` 正文 / PNG 内容写进日志或错误文本。
//! - `snapshots.title` 不在 schema 内（V11 §112 冻结）：快照读取时由调用方
//!   用当前板标题填充该展示字段。

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot, StudyBoardSummary};

use crate::error::InfrastructureError;

/// 当前 schema 版本（新增列时递增并补非破坏性迁移）。
pub const STUDY_BOARD_SCHEMA_VERSION: u32 = 1;

/// 板列表默认/最大条数。
const MAX_LIST_LIMIT: i64 = 200;

/// Study Board SQLite 仓储（学习板 + 快照登记）。
pub struct StudyBoardSqliteStore {
    connection: Mutex<Connection>,
}

impl StudyBoardSqliteStore {
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
        store.migrate()?;
        Ok(store)
    }

    /// 内存库（测试用；不落盘）。
    pub fn open_in_memory() -> Result<Self, InfrastructureError> {
        let connection = Connection::open_in_memory()
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    /// 幂等迁移：建表 + `PRAGMA user_version` 单调校验（§86：重复打开不重建、
    /// 已有数据不丢）。版本比代码新 → 受控错误，不静默降级。
    pub fn migrate(&self) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS boards (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    strokes TEXT NOT NULL,
                    module_origin TEXT NOT NULL DEFAULT '',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_boards_updated_at
                    ON boards(updated_at DESC);
                CREATE TABLE IF NOT EXISTS snapshots (
                    id TEXT PRIMARY KEY,
                    board_id TEXT NOT NULL,
                    png BLOB,
                    summary TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_snapshots_board
                    ON snapshots(board_id, created_at DESC);",
            )
            .map_err(sqlite)?;

        let stored: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        if stored > STUDY_BOARD_SCHEMA_VERSION {
            return Err(InfrastructureError::Sqlite(format!(
                "study_board schema version {stored} is newer than supported \
                 {STUDY_BOARD_SCHEMA_VERSION}"
            )));
        }
        if stored < STUDY_BOARD_SCHEMA_VERSION {
            connection
                .execute_batch(&format!(
                    "PRAGMA user_version = {STUDY_BOARD_SCHEMA_VERSION}"
                ))
                .map_err(sqlite)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 学习板
    // -----------------------------------------------------------------------

    /// 插入或整行覆盖学习板（按 id 幂等）。`strokes` 以 JSON 文本原样存取。
    pub fn upsert_board(&self, board: &StudyBoard) -> Result<(), InfrastructureError> {
        let strokes = serde_json::to_string(&board.strokes).map_err(json_error)?;
        self.connection
            .lock()
            .execute(
                "INSERT INTO boards (id, title, strokes, module_origin, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    strokes = excluded.strokes,
                    module_origin = excluded.module_origin,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                params![
                    board.id,
                    board.title,
                    strokes,
                    board.module_origin,
                    board.created_at,
                    board.updated_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 按 id 读取学习板（含完整 `strokes`；未命中返回 `None`）。
    pub fn get_board(&self, id: &str) -> Result<Option<StudyBoard>, InfrastructureError> {
        self.connection
            .lock()
            .query_row(
                "SELECT id, title, strokes, module_origin, created_at, updated_at
                 FROM boards WHERE id = ?1",
                params![id],
                |row| row_to_board(row).map_err(convert_to_rusqlite),
            )
            .optional()
            .map_err(sqlite)
    }

    /// 学习板列表（`updated_at` 倒序）。`limit` 被夹到 `[1, 200]`。
    ///
    /// 只返回元数据摘要，**不含** `strokes`（笔迹正文只在显式 `get_board` 时返回）。
    pub fn list_boards(&self, limit: usize) -> Result<Vec<StudyBoardSummary>, InfrastructureError> {
        let limit = i64::try_from(limit)
            .unwrap_or(MAX_LIST_LIMIT)
            .clamp(1, MAX_LIST_LIMIT);
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT id, title, module_origin, created_at, updated_at
                 FROM boards ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(params![limit], |row| {
                Ok(StudyBoardSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    module_origin: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    stroke_count: 0,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    /// 板数量（观测；不含正文）。
    pub fn board_count(&self) -> Result<usize, InfrastructureError> {
        let count: i64 = self
            .connection
            .lock()
            .query_row("SELECT COUNT(*) FROM boards", [], |row| row.get(0))
            .map_err(sqlite)?;
        Ok(count.max(0) as usize)
    }

    // -----------------------------------------------------------------------
    // 快照
    // -----------------------------------------------------------------------

    /// 登记（或更新）一块板的快照（按 id 幂等）。
    ///
    /// `png_base64` 非空时解码为 BLOB 落库；非法 base64 → 受控错误。
    pub fn upsert_snapshot(
        &self,
        snapshot: &StudyBoardSnapshot,
    ) -> Result<(), InfrastructureError> {
        let png = snapshot
            .png_base64
            .as_deref()
            .map(base64_decode)
            .transpose()?;
        self.connection
            .lock()
            .execute(
                "INSERT INTO snapshots (id, board_id, png, summary, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    board_id = excluded.board_id,
                    png = excluded.png,
                    summary = excluded.summary,
                    created_at = excluded.created_at",
                params![
                    snapshot.id,
                    snapshot.board_id,
                    png,
                    snapshot.strokes_summary,
                    snapshot.created_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 按 id 读取快照（`title` 为空串 —— schema 不存 title，调用方补当前板标题）。
    pub fn get_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<StudyBoardSnapshot>, InfrastructureError> {
        self.connection
            .lock()
            .query_row(
                "SELECT id, board_id, png, summary, created_at FROM snapshots
                 WHERE id = ?1",
                params![id],
                |row| row_to_snapshot(row).map_err(convert_to_rusqlite),
            )
            .optional()
            .map_err(sqlite)
    }

    /// 某块板的最近快照（`created_at` 倒序）。
    pub fn latest_snapshot(
        &self,
        board_id: &str,
    ) -> Result<Option<StudyBoardSnapshot>, InfrastructureError> {
        self.connection
            .lock()
            .query_row(
                "SELECT id, board_id, png, summary, created_at FROM snapshots
                 WHERE board_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![board_id],
                |row| row_to_snapshot(row).map_err(convert_to_rusqlite),
            )
            .optional()
            .map_err(sqlite)
    }
}

/// 行映射的受控错误 → rusqlite 回调要求的错误类型。
///
/// `rusqlite::Error::FromSqlConversionFailure` 保留 domain 错误文本，
/// 使「损坏 strokes 行」在 `optional()` 之外仍能被上层识别为受控失败。
fn convert_to_rusqlite(error: InfrastructureError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn row_to_board(row: &rusqlite::Row<'_>) -> Result<StudyBoard, InfrastructureError> {
    // 损坏的 strokes JSON → 受控错误（调用方转 ToolResult::fail 兜底），不 panic。
    let strokes_raw: String = row.get(2).map_err(sqlite)?;
    let strokes = serde_json::from_str(&strokes_raw).map_err(|error| {
        InfrastructureError::Sqlite(format!("cannot decode board strokes: {error}"))
    })?;
    Ok(StudyBoard {
        id: row.get(0).map_err(sqlite)?,
        title: row.get(1).map_err(sqlite)?,
        strokes,
        module_origin: row.get(3).map_err(sqlite)?,
        created_at: row.get(4).map_err(sqlite)?,
        updated_at: row.get(5).map_err(sqlite)?,
    })
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> Result<StudyBoardSnapshot, InfrastructureError> {
    let png: Option<Vec<u8>> = row.get(2).map_err(sqlite)?;
    Ok(StudyBoardSnapshot {
        id: row.get(0).map_err(sqlite)?,
        board_id: row.get(1).map_err(sqlite)?,
        // title 不落库：读取方（应用层）按当前板标题回填。
        title: String::new(),
        png_base64: png.as_deref().map(base64_encode),
        strokes_summary: row.get(3).map_err(sqlite)?,
        created_at: row.get(4).map_err(sqlite)?,
    })
}

// ---------------------------------------------------------------------------
// base64（标准字母表 + 填充）
// ---------------------------------------------------------------------------

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 解码。跳过空白与 `=` 填充；坏字符 → 受控错误。
///
/// `=` 只出现在尾部，遇到即停：标准编码器对 1–2 字节尾组产生的 2/4 位残余
/// 不需要完整字节，因此不做「位数必须为 8 的倍数」校验（那会误杀合法填充）。
fn base64_decode(raw: &str) -> Result<Vec<u8>, InfrastructureError> {
    let invalid = || InfrastructureError::Sqlite("snapshot png is not valid base64".into());
    let mut out = Vec::with_capacity(raw.len() / 4 * 3 + 3);
    let mut buffer: u32 = 0;
    let mut bits = 0u8;
    for byte in raw.bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .ok_or_else(invalid)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

/// 编码（标准字母表；3 字节一组，尾部按 byte 数补 `=`）。
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut group = [0u8; 3];
        group[..chunk.len()].copy_from_slice(chunk);
        let buffer = (u32::from(group[0]) << 16) | (u32::from(group[1]) << 8) | u32::from(group[2]);
        let symbols = [
            ALPHABET[(buffer >> 18) as usize & 0x3f],
            ALPHABET[(buffer >> 12) as usize & 0x3f],
            ALPHABET[(buffer >> 6) as usize & 0x3f],
            ALPHABET[buffer as usize & 0x3f],
        ];
        let padding = 3 - chunk.len();
        for (index, symbol) in symbols.iter().enumerate() {
            // 尾部 1–2 字节 → 1–2 个 `=`；padding 用 i32 避免 usize 下溢。
            if index + padding >= 4 {
                out.push('=');
            } else {
                out.push(char::from(*symbol));
            }
        }
    }
    out
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

fn json_error(error: serde_json::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(format!("cannot encode board strokes: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::study_board::{STROKE_SUMMARY_MAX_CHARS, bounded_strokes_summary};
    use serde_json::json;

    fn board(id: &str, title: &str, updated_at: i64) -> StudyBoard {
        StudyBoard::new(
            id,
            title,
            json!({"strokes": [{"points": [[0, 0], [2, 2]], "color": "#000"}]}),
            1_000,
            "history",
        )
        .with_updated_at(updated_at)
    }

    fn snapshot(id: &str, board_id: &str, created_at: i64) -> StudyBoardSnapshot {
        StudyBoardSnapshot::new(
            id,
            board_id,
            "标题",
            None,
            "1 条矢量笔画（坐标由前端渲染，不在此展示）",
            created_at,
        )
    }

    #[test]
    fn schema_is_idempotent_and_preserves_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("study_board.db");
        {
            let store = StudyBoardSqliteStore::open(&path).unwrap();
            store.upsert_board(&board("b-1", "第一版", 1_000)).unwrap();
            store
                .upsert_snapshot(&snapshot("snap-1", "b-1", 1_100))
                .unwrap();
        }
        // 第二次打开：不重建、不丢数据（§86）。
        let store = StudyBoardSqliteStore::open(&path).unwrap();
        let loaded = store
            .get_board("b-1")
            .unwrap()
            .expect("row survives reopen");
        assert_eq!(loaded.title, "第一版");
        assert_eq!(loaded.stroke_count(), 1);
        assert_eq!(store.board_count().unwrap(), 1);
        assert!(store.get_snapshot("snap-1").unwrap().is_some());
    }

    #[test]
    fn board_round_trip_and_update_replaces_by_id() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        let mut first = board("b-1", "第一版", 1_000);
        store.upsert_board(&first).unwrap();
        first.update(
            Some("第二版".into()),
            Some(json!({"strokes": [{"points": [[5, 5]]}, {"points": [[6, 6]]}]})),
            2_000,
        );
        store.upsert_board(&first).unwrap();

        assert_eq!(store.board_count().unwrap(), 1, "按 id 覆盖，不新增行");
        let loaded = store.get_board("b-1").unwrap().expect("row");
        assert_eq!(loaded.title, "第二版");
        assert_eq!(loaded.created_at, 1_000);
        assert_eq!(loaded.updated_at, 2_000);
        assert_eq!(loaded.module_origin, "history");
        // strokes 原样存取（backend 不解释形状）。
        assert_eq!(loaded.strokes, first.strokes);
    }

    #[test]
    fn list_returns_metadata_ordered_by_recency_without_strokes() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        store.upsert_board(&board("b-1", "旧", 1_000)).unwrap();
        store.upsert_board(&board("b-2", "新", 2_000)).unwrap();
        store.upsert_board(&board("b-3", "中", 1_500)).unwrap();

        let items = store.list_boards(10).unwrap();
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b-2", "b-3", "b-1"]
        );
        // 列表不回传笔迹（隐私：正文只在 get_board 显式读取时返回）。
        let rendered = serde_json::to_string(&items).unwrap();
        assert!(!rendered.contains("points"));
        assert_eq!(store.list_boards(0).unwrap().len(), 1, "limit 下限 1");
        assert_eq!(store.list_boards(10_000).unwrap().len(), 3);
    }

    #[test]
    fn corrupt_strokes_row_is_controlled_error() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        {
            let connection = store.connection.lock();
            connection
                .execute(
                    "INSERT INTO boards (id, title, strokes, module_origin, created_at, updated_at)
                     VALUES ('b-bad', '坏行', 'not-json{', 'history', 1, 2)",
                    [],
                )
                .unwrap();
        }
        // 不 panic：受控错误。
        let error = store
            .get_board("b-bad")
            .expect_err("corrupt row must not panic");
        assert!(error.to_string().contains("cannot decode board strokes"));
        // 其他行不受影响。
        assert!(store.get_board("b-1").unwrap().is_none());
    }

    #[test]
    fn get_missing_returns_none() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        assert!(store.get_board("nope").unwrap().is_none());
        assert!(store.get_snapshot("nope").unwrap().is_none());
        assert!(store.latest_snapshot("nope").unwrap().is_none());
        assert_eq!(store.board_count().unwrap(), 0);
    }

    #[test]
    fn snapshot_png_round_trips_and_title_is_empty_from_store() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        let bytes = vec![0x89u8, b'P', b'N', 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let encoded = base64_encode(&bytes);
        let mut stored = snapshot("snap-1", "b-1", 1_100);
        stored.png_base64 = Some(encoded.clone());
        store.upsert_snapshot(&stored).unwrap();

        let loaded = store.get_snapshot("snap-1").unwrap().expect("snapshot");
        assert_eq!(loaded.png_base64.as_deref(), Some(encoded.as_str()));
        assert_eq!(loaded.board_id, "b-1");
        assert_eq!(loaded.title, "", "title 不落库");
        assert!(loaded.strokes_summary.chars().count() <= STROKE_SUMMARY_MAX_CHARS);
        assert_eq!(
            store
                .latest_snapshot("b-1")
                .unwrap()
                .map(|snapshot| snapshot.id),
            Some("snap-1".to_string())
        );

        // 更新同一快照 id：PNG 覆盖、行数不变。
        stored.png_base64 = None;
        stored.strokes_summary = "空学习板（无笔画）".into();
        store.upsert_snapshot(&stored).unwrap();
        let reloaded = store.get_snapshot("snap-1").unwrap().expect("snapshot");
        assert!(reloaded.png_base64.is_none());
        assert_eq!(reloaded.strokes_summary, "空学习板（无笔画）");
    }

    #[test]
    fn invalid_base64_png_is_controlled_error() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        let mut stored = snapshot("snap-1", "b-1", 1);
        stored.png_base64 = Some("###not-base64###".into());
        let error = store
            .upsert_snapshot(&stored)
            .expect_err("invalid payload must not panic");
        assert!(error.to_string().contains("not valid base64"));
    }

    #[test]
    fn base64_codec_round_trips_arbitrary_lengths() {
        for length in 0..40usize {
            let bytes: Vec<u8> = (0..length).map(|index| (index * 7 + 3) as u8).collect();
            let encoded = base64_encode(&bytes);
            assert_eq!(base64_decode(&encoded).unwrap(), bytes, "length {length}");
        }
        // 含空字节与高位字节的 payload。
        let bytes: Vec<u8> = (0..=u8::MAX).collect();
        let encoded = base64_encode(&bytes);
        assert_eq!(base64_decode(&encoded).unwrap(), bytes);
    }

    #[test]
    fn snapshot_summary_is_bounded_when_read_back() {
        let store = StudyBoardSqliteStore::open_in_memory().unwrap();
        let mut stored = snapshot("snap-2", "b-1", 1);
        stored.strokes_summary = bounded_strokes_summary(&"笔".repeat(9_999));
        store.upsert_snapshot(&stored).unwrap();
        let loaded = store.get_snapshot("snap-2").unwrap().expect("snapshot");
        assert_eq!(
            loaded.strokes_summary.chars().count(),
            STROKE_SUMMARY_MAX_CHARS
        );
        assert!(loaded.strokes_summary.ends_with('…'));
    }
}
