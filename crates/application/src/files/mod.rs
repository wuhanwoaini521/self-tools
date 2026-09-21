//! Files 域（V6 Track C）。
//!
//! 依赖方向：只依赖 `devtoolbox-core`（策略与契约）与本 crate 的 `error`/`text`/`time`；
//! 文件系统与索引经 `FileSystemPort` / `FileIndexPort`，实现在 infrastructure。

pub mod ports;
pub mod service;

pub use ports::{
    FileFingerprint, FileIndexError, FileIndexPort, FileIndexStats, FileQuery, FileReadOutcome,
    FileSystemPort, FileSystemResult, RawFile,
};
pub use service::{FileConfig, FileIndexReport, FileReadResult, FileService, file_score};

#[cfg(test)]
mod tests;
