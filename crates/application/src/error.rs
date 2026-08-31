use std::path::PathBuf;

use devtoolbox_infrastructure::InfrastructureError;
use thiserror::Error;

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
    #[error("history error: {source}")]
    History { source: InfrastructureError },
    #[error("history data error: {0}")]
    HistoryData(String),
    #[error("operation failed for {path}: {source}")]
    Infrastructure {
        path: PathBuf,
        #[source]
        source: InfrastructureError,
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

pub(crate) fn infrastructure(path: PathBuf, source: InfrastructureError) -> ApplicationError {
    ApplicationError::Infrastructure { path, source }
}
