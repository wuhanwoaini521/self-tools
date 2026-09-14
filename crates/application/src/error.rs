use std::path::PathBuf;

use devtoolbox_infrastructure::InfrastructureError;
use thiserror::Error;

use crate::history::ports::HistoryPortError;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("document path is empty")]
    EmptyDocumentPath,
    #[error("workspace path is empty")]
    EmptyWorkspacePath,
    #[error("feed url is invalid (expect http/https): {0}")]
    InvalidFeedUrl(String),
    #[error("feed already subscribed: {0}")]
    DuplicateFeed(String),
    #[error("feed not found: {0}")]
    FeedNotFound(i64),
    #[error("city name is empty")]
    EmptyCity,
    #[error("travel research failed: {0}")]
    TravelFailed(String),
    #[error("geography error: {message}")]
    Geography { message: String },
    #[error("geography data error: {0}")]
    GeographyData(String),
    /// History 只读查询失败。消息由适配层在端口错误中提供，保持此前桌面端
    /// 透传的 Infrastructure 错误文本不变（用户可见的 message 与 code 均不改变）。
    /// 注意：不能写 `#[from]`（Rss 已占用 From<InfrastructureError>），
    /// 由 HistoryService 用 `map_err(ApplicationError::History)` 显式构造。
    #[error(transparent)]
    History(HistoryPortError),
    #[error("operation failed for {path}: {message}")]
    Infrastructure {
        path: PathBuf,
        /// 可显示的错误文本（适配层已把基础设施错误转换为字符串）。
        message: String,
    },
    #[error("language error: {source}")]
    Language { source: InfrastructureError },
    #[error("language license gate: {0}")]
    License(String),
    #[error("rss error: {source}")]
    Rss {
        #[from]
        source: InfrastructureError,
    },
    #[error("travel error: {source}")]
    Travel { source: InfrastructureError },
}

pub(crate) fn infrastructure(path: PathBuf, message: String) -> ApplicationError {
    ApplicationError::Infrastructure { path, message }
}