//! 工作区/文档浏览的纯数据契约（`WorkspaceFile`）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
}
