//! Study Board（学习板）存储端口（V11 §110-§112）。
//!
//! 端口属于用例层（application）：实现（SQLite `config/study_board.db`）在
//! infrastructure（`StudyBoardSqliteStore`），组合根装配。用例层不感知 SQL /
//! 序列化细节。方法全部同步（短锁语义，不跨 await）。
//!
//! **隐私铁律（V11 §113/§114）**：`strokes` 与快照是用户私有数据 ——
//! 不得自动提升为 Personal Memory（工具路径永不写 memory），不得进日志/遥测；
//! 端口错误文本只含 id 与稳定 reason，不含笔迹正文。

use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot, StudyBoardSummary};

/// Study Board 存储错误（适配层已把基础设施错误转为可显示文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyBoardStoreError(pub String);

impl std::fmt::Display for StudyBoardStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StudyBoardStoreError {}

/// Study Board 持久化端口。
pub trait StudyBoardStorePort: Send + Sync {
    /// 插入或整行覆盖学习板（按 id 幂等；`strokes` 原样存取）。
    fn upsert_board(&self, board: &StudyBoard) -> Result<(), StudyBoardStoreError>;

    /// 按 id 读取学习板（未命中返回 `None`）。
    fn get_board(&self, id: &str) -> Result<Option<StudyBoard>, StudyBoardStoreError>;

    /// 学习板元数据列表（`updated_at` 倒序；不含笔迹正文）。
    fn list_boards(&self, limit: usize) -> Result<Vec<StudyBoardSummary>, StudyBoardStoreError>;

    /// 登记（或更新）快照。
    fn upsert_snapshot(&self, snapshot: &StudyBoardSnapshot) -> Result<(), StudyBoardStoreError>;

    /// 按 id 读快照。
    fn get_snapshot(&self, id: &str) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError>;

    /// 某块板的最近快照。
    fn latest_snapshot(&self, board_id: &str)
        -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError>;
}

/// 基础设施错误 → 端口错误文本（组合根/测试复用；不含正文）。
#[must_use]
pub fn store_error_text(error: impl std::fmt::Display) -> StudyBoardStoreError {
    StudyBoardStoreError(error.to_string())
}
