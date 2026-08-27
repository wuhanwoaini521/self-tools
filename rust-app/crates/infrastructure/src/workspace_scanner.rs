use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{InfrastructureError, error::io_error};

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "venv",
    "node_modules",
    "__pycache__",
    ".idea",
    ".vscode",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
}

#[must_use]
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// 递归扫描 Markdown 文档，保持当前 Python 的跳过和排序规则。
pub fn scan_markdown_files(root: &Path) -> Result<Vec<WorkspaceFile>, InfrastructureError> {
    if !root.is_dir() {
        return Err(InfrastructureError::InvalidWorkspace(root.to_owned()));
    }

    let mut stack = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory).map_err(|source| io_error(&directory, source))?;
        for entry in entries {
            let entry = entry.map_err(|source| io_error(&directory, source))?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let file_type = entry
                .file_type()
                .map_err(|source| io_error(&path, source))?;
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file() && is_markdown(&path) {
                let relative_path = path
                    .strip_prefix(root)
                    .map_err(|source| io_error(&path, std::io::Error::other(source)))?
                    .to_owned();
                files.push(WorkspaceFile {
                    path,
                    relative_path,
                });
            }
        }
    }
    files.sort_by_cached_key(|file| file.relative_path.to_string_lossy().to_lowercase());
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::scan_markdown_files;

    #[test]
    fn skips_noise_directories_and_sorts_paths() {
        let directory = tempdir().expect("temporary directory");
        fs::create_dir_all(directory.path().join("nested")).expect("nested dir");
        fs::create_dir_all(directory.path().join(".git")).expect("git dir");
        fs::write(directory.path().join("z.md"), "z").expect("z file");
        fs::write(directory.path().join("nested/a.markdown"), "a").expect("a file");
        fs::write(directory.path().join(".git/ignored.md"), "ignored").expect("ignored file");

        let files = scan_markdown_files(directory.path()).expect("scan workspace");
        let paths = files
            .iter()
            .map(|file| file.relative_path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        assert_eq!(paths, ["nested/a.markdown", "z.md"]);
    }
}
