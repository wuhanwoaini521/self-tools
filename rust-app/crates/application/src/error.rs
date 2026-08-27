use std::path::PathBuf;

use devtoolbox_infrastructure::InfrastructureError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("document path is empty")]
    EmptyDocumentPath,
    #[error("workspace path is empty")]
    EmptyWorkspacePath,
    #[error("operation failed for {path}: {source}")]
    Infrastructure {
        path: PathBuf,
        #[source]
        source: InfrastructureError,
    },
}

pub(crate) fn infrastructure(path: PathBuf, source: InfrastructureError) -> ApplicationError {
    ApplicationError::Infrastructure { path, source }
}
