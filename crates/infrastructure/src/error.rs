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
}

pub(crate) fn io_error(path: impl Into<PathBuf>, source: io::Error) -> InfrastructureError {
    InfrastructureError::Io {
        path: path.into(),
        source,
    }
}
