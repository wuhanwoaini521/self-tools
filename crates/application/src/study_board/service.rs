//! Study Board 用例服务（V11 §110-§114）。
//!
//! ## 为什么要有这一层
//!
//! 此前「保存一块学习板」这件事**只有 agent 工具一条路**：Personal AI 的
//! `study-board.save`。前端画板因此只能自己往 `localStorage` 里塞一份 ——
//! 换浏览器就没了、桌面端与网页端各存各的、AI 看得到的那块板和用户画的那块板
//! 并不是同一块。
//!
//! 现在编排逻辑收在这里：
//! - agent 工具（`personal_ai::study_board`）与 HTTP / Tauri 命令**共用**同一份用例；
//! - 端口（`StudyBoardStorePort`）仍由组合根装配 SQLite 实现，本模块不含 SQL。
//!
//! 铁律不变（§108/§109/§111/§113）：
//! - **后端不执行绘画**：`strokes` 是不透明 JSON，原样存取；
//! - **后端不解释笔迹**：面向模型/日志的摘要一律走 `strokes_summary`（有界）；
//! - 错误文本只含稳定 reason，不含笔迹正文；
//! - 板与快照是用户私有数据 —— 本服务**不**触碰 memory 服务、不写日志。

use std::sync::Arc;

use devtoolbox_core::study_board::{
    STUDY_BOARD_MAX_TITLE_CHARS, StudyBoard, StudyBoardSnapshot, StudyBoardSummary,
    is_valid_board_id, strokes_summary,
};

use super::ports::{StudyBoardStoreError, StudyBoardStorePort};
use crate::time::now_unix;

/// 列表默认条数与上限（与 agent 工具一致：模型与前端都不能一次拉走全部）。
pub const STUDY_BOARD_LIST_DEFAULT_LIMIT: usize = 20;
/// 列表条数上限。
pub const STUDY_BOARD_LIST_MAX_LIMIT: usize = 200;

/// 用例层错误：`reason` 稳定、可映射到前端错误码；文本不含笔迹正文。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StudyBoardError {
    /// 板 id 形态非法（注入形态 / 超长 / 前后空白）。
    #[error("invalid board_id `{0}`（只允许小写英数字与 . _ -，1-64 字符）")]
    InvalidBoardId(String),
    /// 一次保存既没给标题也没给笔迹 —— 等于什么都没写。
    #[error("至少提供 title 或 strokes 之一")]
    MissingContent,
    /// 新板必须带标题（否则列表里是一行无法辨认的空白）。
    #[error("新学习板「{0}」必须提供 title")]
    MissingTitle(String),
    /// 标题超长。
    #[error("title 超过 {STUDY_BOARD_MAX_TITLE_CHARS} 字符")]
    TitleTooLong,
    /// 目标板不存在。
    #[error("学习板「{0}」不存在")]
    NotFound(String),
    /// 存储失败。`reason` 是稳定分类（供前端/HTTP 归类），`message` 是底层文本。
    #[error("{reason}: {message}")]
    Store {
        reason: &'static str,
        message: String,
    },
}

impl StudyBoardError {
    /// 是否为「参数不合法」（前端可提示改输入，而不是报服务故障）。
    #[must_use]
    pub fn is_invalid_argument(&self) -> bool {
        matches!(
            self,
            Self::InvalidBoardId(_)
                | Self::MissingContent
                | Self::MissingTitle(_)
                | Self::TitleTooLong
        )
    }

    /// 是否为「目标不存在」。
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

/// 保存结果（区分新建与更新：前端据此提示「已创建」而不是「已覆盖」）。
#[derive(Debug, Clone)]
pub struct SavedBoard {
    pub board: StudyBoard,
    /// 本次调用是否新建了这块板。
    pub created: bool,
}

/// 学习板用例服务（板 + 快照）。
pub struct StudyBoardService {
    store: Arc<dyn StudyBoardStorePort>,
}

impl StudyBoardService {
    #[must_use]
    pub fn new(store: Arc<dyn StudyBoardStorePort>) -> Self {
        Self { store }
    }

    /// 底层端口（ContextProvider / 组合根需要读同一份存储时用）。
    #[must_use]
    pub fn store(&self) -> Arc<dyn StudyBoardStorePort> {
        Arc::clone(&self.store)
    }

    /// 板元数据列表（不含笔迹正文）。
    pub fn list(&self, limit: Option<usize>) -> Result<Vec<StudyBoardSummary>, StudyBoardError> {
        let limit = limit
            .unwrap_or(STUDY_BOARD_LIST_DEFAULT_LIMIT)
            .clamp(1, STUDY_BOARD_LIST_MAX_LIMIT);
        self.store
            .list_boards(limit)
            .map_err(|error| store_error("board_list_failed", &error))
    }

    /// 读取一块板（含笔迹正文；调用方是渲染器，不是模型）。
    pub fn get(&self, board_id: &str) -> Result<Option<StudyBoard>, StudyBoardError> {
        let board_id = valid_board_id(board_id)?;
        self.store
            .get_board(&board_id)
            .map_err(|error| store_error("board_read_failed", &error))
    }

    /// 幂等保存（按 id upsert）：任一字段缺省即保留原值。
    pub fn save(
        &self,
        board_id: &str,
        title: Option<String>,
        strokes: Option<serde_json::Value>,
        module_origin: Option<String>,
    ) -> Result<SavedBoard, StudyBoardError> {
        let board_id = valid_board_id(board_id)?;
        let title = title
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(value) = &title
            && value.chars().count() > STUDY_BOARD_MAX_TITLE_CHARS
        {
            return Err(StudyBoardError::TitleTooLong);
        }
        if title.is_none() && strokes.is_none() {
            return Err(StudyBoardError::MissingContent);
        }

        let now = now_unix();
        let existing = self
            .store
            .get_board(&board_id)
            .map_err(|error| store_error("board_read_failed", &error))?;
        let (board, created) = match existing {
            Some(mut board) => {
                board.update(title, strokes, now);
                if let Some(origin) = module_origin {
                    board.module_origin = origin;
                }
                (board, false)
            }
            None => {
                // 新板必须有标题：没有标题的板在列表里只是一行空白。
                let Some(title) = title else {
                    return Err(StudyBoardError::MissingTitle(board_id));
                };
                let strokes = strokes.unwrap_or_else(|| serde_json::json!({"strokes": []}));
                (
                    StudyBoard::new(
                        board_id,
                        title,
                        strokes,
                        now,
                        module_origin.unwrap_or_default(),
                    ),
                    true,
                )
            }
        };
        self.store
            .upsert_board(&board)
            .map_err(|error| store_error("board_save_failed", &error))?;
        Ok(SavedBoard { board, created })
    }

    /// 登记快照（前端渲染 PNG 后传入 base64；不传则只登记有界摘要）。
    ///
    /// 后端**不生成图像**：这里只保存引用与摘要。
    pub fn save_snapshot(
        &self,
        board_id: &str,
        png_base64: Option<String>,
    ) -> Result<StudyBoardSnapshot, StudyBoardError> {
        let board_id = valid_board_id(board_id)?;
        let board = self
            .store
            .get_board(&board_id)
            .map_err(|error| store_error("board_read_failed", &error))?
            .ok_or_else(|| StudyBoardError::NotFound(board_id.clone()))?;
        let summary = strokes_summary(&board.strokes);
        let snapshot = StudyBoardSnapshot::new(
            snapshot_id(),
            &board.id,
            &board.title,
            png_base64,
            summary,
            now_unix(),
        );
        self.store
            .upsert_snapshot(&snapshot)
            .map_err(|error| store_error("snapshot_save_failed", &error))?;
        Ok(snapshot)
    }

    /// 最近快照引用（用于「上次画的是什么」；不含 PNG 正文）。
    pub fn latest_snapshot_id(&self, board_id: &str) -> Result<Option<String>, StudyBoardError> {
        let board_id = valid_board_id(board_id)?;
        self.store
            .latest_snapshot(&board_id)
            .map(|snapshot| snapshot.map(|item| item.id))
            .map_err(|error| store_error("snapshot_read_failed", &error))
    }

    /// 读快照（含 PNG base64；只给渲染快照图用）。
    pub fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<StudyBoardSnapshot>, StudyBoardError> {
        self.store
            .get_snapshot(snapshot_id)
            .map_err(|error| store_error("snapshot_read_failed", &error))
    }
}

/// 板 id 校验（§110：只允许稳定标识，封死注入形态）。
fn valid_board_id(raw: &str) -> Result<String, StudyBoardError> {
    let trimmed = raw.trim();
    if !is_valid_board_id(trimmed) {
        return Err(StudyBoardError::InvalidBoardId(raw.to_string()));
    }
    Ok(trimmed.to_string())
}

fn store_error(reason: &'static str, error: &StudyBoardStoreError) -> StudyBoardError {
    StudyBoardError::Store {
        reason,
        message: error.0.clone(),
    }
}

/// 快照 id（`snap-*`；无 crypto 依赖，与 agent 会话 id 同手法）。
fn snapshot_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("snap-{:016x}", nanos % u128::from(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study_board::ports::StudyBoardStoreError;
    use parking_lot::Mutex;
    use serde_json::json;
    use std::collections::HashMap;

    /// 测试用端口：内存 Map + 可注入失败。
    #[derive(Default)]
    struct FakeStore {
        boards: Mutex<HashMap<String, StudyBoard>>,
        snapshots: Mutex<HashMap<String, StudyBoardSnapshot>>,
        fail: Mutex<Option<&'static str>>,
    }

    impl FakeStore {
        fn seed(&self, board: StudyBoard) {
            self.boards.lock().insert(board.id.clone(), board);
        }

        fn inject_failure(&self, reason: &'static str) {
            *self.fail.lock() = Some(reason);
        }

        fn check(&self, reason: &'static str) -> Result<(), StudyBoardStoreError> {
            match *self.fail.lock() {
                Some(active) if active == reason => {
                    Err(StudyBoardStoreError("injected".to_string()))
                }
                _ => Ok(()),
            }
        }
    }

    impl StudyBoardStorePort for FakeStore {
        fn upsert_board(&self, board: &StudyBoard) -> Result<(), StudyBoardStoreError> {
            self.check("board_save_failed")?;
            self.boards.lock().insert(board.id.clone(), board.clone());
            Ok(())
        }

        fn get_board(&self, id: &str) -> Result<Option<StudyBoard>, StudyBoardStoreError> {
            self.check("board_read_failed")?;
            Ok(self.boards.lock().get(id).cloned())
        }

        fn list_boards(
            &self,
            limit: usize,
        ) -> Result<Vec<StudyBoardSummary>, StudyBoardStoreError> {
            self.check("board_list_failed")?;
            let mut items: Vec<StudyBoardSummary> = self
                .boards
                .lock()
                .values()
                .map(StudyBoard::summary)
                .collect();
            items.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
            items.truncate(limit);
            Ok(items)
        }

        fn upsert_snapshot(
            &self,
            snapshot: &StudyBoardSnapshot,
        ) -> Result<(), StudyBoardStoreError> {
            self.check("snapshot_save_failed")?;
            self.snapshots
                .lock()
                .insert(snapshot.id.clone(), snapshot.clone());
            Ok(())
        }

        fn get_snapshot(
            &self,
            id: &str,
        ) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
            self.check("snapshot_read_failed")?;
            Ok(self.snapshots.lock().get(id).cloned())
        }

        fn latest_snapshot(
            &self,
            board_id: &str,
        ) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
            self.check("snapshot_read_failed")?;
            Ok(self
                .snapshots
                .lock()
                .values()
                .filter(|snapshot| snapshot.board_id == board_id)
                .max_by_key(|snapshot| snapshot.created_at)
                .cloned())
        }
    }

    fn service() -> (Arc<FakeStore>, StudyBoardService) {
        let store = Arc::new(FakeStore::default());
        let service = StudyBoardService::new(Arc::clone(&store) as Arc<dyn StudyBoardStorePort>);
        (store, service)
    }

    #[test]
    fn save_is_idempotent_and_keeps_untouched_fields() {
        let (store, service) = service();
        let created = service
            .save(
                "b-1",
                Some("遵义会议".into()),
                Some(json!({"strokes": [{"points": [[1, 2]]}]})),
                Some("history".into()),
            )
            .expect("create");
        assert!(created.created);
        assert_eq!(created.board.stroke_count(), 1);
        assert_eq!(created.board.created_at, created.board.updated_at);

        let updated = service
            .save("b-1", Some("遵义会议草图".into()), None, None)
            .expect("update");
        assert!(!updated.created);
        assert_eq!(updated.board.stroke_count(), 1, "未给笔迹时保留原笔迹");
        assert_eq!(
            updated.board.module_origin, "history",
            "未给来源时保留原来源"
        );
        assert_eq!(
            updated.board.created_at, created.board.created_at,
            "created_at 不动"
        );
        assert_eq!(store.boards.lock().len(), 1, "按 id upsert，不新增行");
    }

    #[test]
    fn save_rejects_empty_and_injection_shaped_input() {
        let (_store, service) = service();
        assert_eq!(
            service.save("b-1", None, None, None).unwrap_err(),
            StudyBoardError::MissingContent
        );
        assert_eq!(
            service
                .save("b-new", None, Some(json!({"strokes": []})), None)
                .unwrap_err(),
            StudyBoardError::MissingTitle("b-new".into())
        );
        let long = service
            .save(
                "b-1",
                Some("板".repeat(STUDY_BOARD_MAX_TITLE_CHARS + 1)),
                None,
                None,
            )
            .unwrap_err();
        assert_eq!(long, StudyBoardError::TitleTooLong);
        assert!(long.is_invalid_argument());
        for bad in ["../etc/passwd", "B-UPPER", "b 1", ""] {
            assert!(
                service.save(bad, Some("x".into()), None, None).is_err(),
                "应当拒绝 {bad:?}"
            );
        }
    }

    #[test]
    fn missing_board_is_not_found_not_store_error() {
        let (_store, service) = service();
        let error = service.save_snapshot("b-none", None).unwrap_err();
        assert!(error.is_not_found());
        assert!(!error.is_invalid_argument());
        // 读取不存在的板返回 None（调用方决定怎么展示）。
        assert!(service.get("b-none").expect("read missing").is_none());
    }

    #[test]
    fn store_failures_carry_stable_reason_without_payload() {
        let (store, service) = service();
        store.inject_failure("board_save_failed");
        let error = service
            .save(
                "b-1",
                Some("含笔迹".into()),
                Some(json!({"strokes": [{"points": [[7, 7]]}]})),
                None,
            )
            .unwrap_err();
        let text = error.to_string();
        assert!(text.starts_with("board_save_failed: "), "{text}");
        assert!(!text.contains("points"), "错误不得回显笔迹正文");
        assert!(!error.is_invalid_argument() && !error.is_not_found());
    }

    #[test]
    fn snapshot_is_registered_with_bounded_summary() {
        let (store, service) = service();
        service
            .save(
                "b-1",
                Some("板".into()),
                Some(json!({"strokes": [{"points": [[1, 1], [2, 2]]}]})),
                None,
            )
            .expect("save");
        let snapshot = service.save_snapshot("b-1", None).expect("snapshot");
        assert_eq!(snapshot.board_id, "b-1");
        assert!(snapshot.png_base64.is_none());
        assert_eq!(
            snapshot.strokes_summary,
            "1 条矢量笔画（坐标由前端渲染，不在此展示）"
        );
        assert_eq!(
            service.latest_snapshot_id("b-1").expect("latest"),
            Some(snapshot.id.clone())
        );
        assert_eq!(
            service
                .get_snapshot(&snapshot.id)
                .expect("get")
                .map(|item| item.id),
            Some(snapshot.id)
        );
        assert_eq!(store.snapshots.lock().len(), 1);
    }

    #[test]
    fn list_is_clamped_and_newest_first() {
        let (store, service) = service();
        store.seed(StudyBoard::new(
            "b-old",
            "旧",
            json!({"strokes": []}),
            100,
            "history",
        ));
        store.seed(StudyBoard::new(
            "b-new",
            "新",
            json!({"strokes": []}),
            900,
            "language",
        ));
        let items = service.list(None).expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "b-new", "按更新时间倒序");
        // limit 夹取：0 → 1，超大 → 上限。
        assert_eq!(service.list(Some(0)).expect("limit 0").len(), 1);
        assert_eq!(
            service
                .list(Some(STUDY_BOARD_LIST_MAX_LIMIT + 100))
                .expect("limit huge")
                .len(),
            2
        );
    }

    #[test]
    fn get_returns_board_with_untouched_strokes() {
        let (_store, service) = service();
        service
            .save(
                "b-1",
                Some("板".into()),
                Some(json!({"strokes": [{"points": [[5, 5]]}]})),
                None,
            )
            .expect("save");
        let board = service.get("b-1").expect("get").expect("存在");
        assert_eq!(board.title, "板");
        assert_eq!(board.stroke_count(), 1);
        // 前后空白被规整（前端可能带空格），但非法形态仍拒绝。
        assert!(service.get("  b-1  ").expect("trimmed").is_some());
        assert!(service.get("b 1").is_err());
    }
}
