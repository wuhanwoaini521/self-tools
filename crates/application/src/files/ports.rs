//! Files 端口（V6 Track C，§43-§48）。
//!
//! - `FileSystemPort`：唯一的文件系统接触面（canonicalize / 元数据 / 只读文本 / 扫描）；
//!   **没有** write / move / delete / chmod / execute（V6 §42）。
//! - `FileIndexPort`：SQLite `config/files.db`（元数据索引，不缓存正文）。

use std::fmt;
use std::path::PathBuf;

use devtoolbox_core::files::{FileAccessDenied, FileMetadata, KnowledgeRoot};

/// 文件索引错误（可显示文本；不含正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndexError(pub String);

impl fmt::Display for FileIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FileIndexError {}

/// 文件系统层错误：复用 core 的拒绝原因，保证「拒绝语义」单一来源。
pub type FileSystemResult<T> = Result<T, FileAccessDenied>;

/// 文件系统端口（infrastructure 实现；只读）。
pub trait FileSystemPort: Send + Sync {
    /// 解析为绝对规范路径（解析 symlink / 相对路径 / `.`）。
    /// 不存在 → `NotFound`；非普通文件 → `NotAFile`。
    fn canonicalize(&self, path: &str) -> FileSystemResult<PathBuf>;

    /// 读取元数据（大小 / 修改时间 / 是否普通文件）。
    fn metadata(&self, path: &PathBuf) -> FileSystemResult<RawFile>;

    /// 只读文本读取（UTF-8；含 NUL 或解码失败 → `Binary`）。
    fn read_text(&self, path: &PathBuf, max_bytes: u64) -> FileSystemResult<FileReadOutcome>;

    /// 递归扫描允许根内的**文件**（跳过噪音目录；返回 (文件列表, 是否截断)）。
    /// 不做 deny 过滤（由服务层用策略标注 `restricted`），但绝不读取内容。
    fn walk(&self, root: &KnowledgeRoot, limit: usize) -> FileSystemResult<(Vec<RawFile>, bool)>;
}

// 检索 / 读取的共享数据形状由 core 持有（infra 实现、application 消费）。
pub use devtoolbox_core::files::{
    FileFingerprint, FileIndexStats, FileQuery, FileReadOutcome, RawFile,
};

/// 文件索引端口（SQLite `config/files.db`）。
pub trait FileIndexPort: Send + Sync {
    fn upsert_many(&self, entries: &[FileMetadata]) -> Result<(), FileIndexError>;
    fn get(&self, file_id: &str) -> Result<Option<FileMetadata>, FileIndexError>;
    /// 精确路径查找（服务层授权后按规范路径回填 id）。
    fn find_by_path(&self, path: &str) -> Result<Option<FileMetadata>, FileIndexError>;
    fn search(&self, spec: &FileQuery) -> Result<Vec<FileMetadata>, FileIndexError>;
    fn recent(&self, limit: usize) -> Result<Vec<FileMetadata>, FileIndexError>;
    fn fingerprints(&self, root_id: &str) -> Result<Vec<FileFingerprint>, FileIndexError>;
    fn remove(&self, file_id: &str) -> Result<(), FileIndexError>;
    fn stats(&self) -> Result<FileIndexStats, FileIndexError>;
}
