//! Personal Memory 存储端口（V6 Track A）。
//!
//! 端口属于用例层（application）：实现（SQLite `config/memory.db`）在 infrastructure，
//! 组合根（`apps/desktop`）负责装配。用例层不感知 SQL / 序列化细节。
//!
//! **没有物理删除方法**（V6 §20）：Memory 只支持归档；物理删除留给未来的管理页
//! 与凭据级审计，模型与 V6 命令面都无法调用。

use std::fmt;

use devtoolbox_core::memory::{MemoryItem, MemoryCategory, MemoryQuery, MemoryStatus};

/// Memory 存储错误（适配层已把基础设施错误转换为可显示文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryStoreError(pub String);

impl fmt::Display for MemoryStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MemoryStoreError {}

/// Memory 持久化端口。
pub trait MemoryStorePort: Send + Sync {
    /// 插入或整行覆盖（按 id 幂等）。
    fn upsert(&self, item: &MemoryItem) -> Result<(), MemoryStoreError>;

    /// 按 id 读取。
    fn get(&self, id: &str) -> Result<Option<MemoryItem>, MemoryStoreError>;

    /// 按条件查询（关键词粗筛 + category/status 过滤，updated_at 倒序）。
    ///
    /// `MemoryQuery::query` 的语义：**空白分隔的多个关键词，命中任一即候选**
    /// （大小写不敏感子串匹配；调用方用 `crate::text::keywords` 归一，含 CJK bigram）。
    ///
    /// `MemoryQuery::include_sensitive = false` 时**必须**过滤掉 Sensitive 行
    /// （V6 §69：敏感内容默认不进模型，也不进工具结果）。
    fn query(&self, spec: &MemoryQuery) -> Result<Vec<MemoryItem>, MemoryStoreError>;

    /// 记录「最后使用时间」（检索命中后调用，失败不致命）。
    fn touch_used(&self, ids: &[String], now: i64) -> Result<(), MemoryStoreError>;

    /// 各状态计数（管理页 / 观测；只返回计数不含正文）。
    fn count_by_status(&self) -> Result<Vec<(MemoryStatus, usize)>, MemoryStoreError>;

    /// 各分类计数（管理页筛选用）。
    fn count_by_category(&self) -> Result<Vec<(MemoryCategory, usize)>, MemoryStoreError>;
}
