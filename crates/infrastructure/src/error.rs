use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InfrastructureError {
    #[error("cannot access {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("cannot decode settings file {path}: {source}")]
    SettingsDecode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("cannot encode settings: {0}")]
    SettingsEncode(serde_json::Error),
    #[error("workspace path is not a directory: {0}")]
    InvalidWorkspace(PathBuf),
    #[error("sqlite error: {0}")]
    Sqlite(String),
    #[error("failed to fetch feed: {0}")]
    FeedFetch(String),
    #[error("failed to parse feed: {0}")]
    FeedParse(String),
}

pub(crate) fn io_error(path: impl Into<PathBuf>, source: io::Error) -> InfrastructureError {
    InfrastructureError::Io {
        path: path.into(),
        source,
    }
}
