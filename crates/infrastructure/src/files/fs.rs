//! 文件系统实现（V6 Track C，只读）。
//!
//! 唯一接触文件系统的位置。**没有** write / move / delete / chmod / execute。
//! 目录遍历不跟随符号链接（`DirEntry::file_type` 不解引用），避免环与逃逸；
//! 单文件访问的逃逸由上层「先 canonicalize 再校验允许根」拦截（§44）。

use std::path::{Path, PathBuf};

use devtoolbox_core::files::{FileAccessDenied, KnowledgeRoot};

use crate::documents::extract::{is_binary, modified_timestamp};
use devtoolbox_core::files::{FileReadOutcome, RawFile};

/// 扫描跳过的噪音/构建目录（与 Documents 扫描保持一致的语义）。
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
    "target",
    "dist",
    "build",
    ".cache",
    ".next",
    "AppData",
];

/// 本地文件系统（只读）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFileSystem;

impl LocalFileSystem {
    pub fn canonicalize(&self, path: &str) -> Result<PathBuf, FileAccessDenied> {
        if path.trim().is_empty() {
            return Err(FileAccessDenied::NotFound);
        }
        std::fs::canonicalize(path).map_err(|_| FileAccessDenied::NotFound)
    }

    pub fn metadata(&self, path: &PathBuf) -> Result<RawFile, FileAccessDenied> {
        let metadata = std::fs::metadata(path).map_err(|_| FileAccessDenied::NotFound)?;
        let file_name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().to_string());
        Ok(RawFile {
            path: path.clone(),
            relative_path: file_name,
            size_bytes: metadata.len(),
            modified_at: modified_timestamp(&metadata),
            is_file: metadata.is_file(),
        })
    }

    pub fn read_text(
        &self,
        path: &PathBuf,
        max_bytes: u64,
    ) -> Result<FileReadOutcome, FileAccessDenied> {
        let metadata = std::fs::metadata(path).map_err(|_| FileAccessDenied::NotFound)?;
        if !metadata.is_file() {
            return Err(FileAccessDenied::NotAFile);
        }
        if metadata.len() > max_bytes {
            return Ok(FileReadOutcome::TooLarge);
        }
        let bytes = std::fs::read(path).map_err(|_| FileAccessDenied::NotFound)?;
        if is_binary(&bytes) {
            return Ok(FileReadOutcome::Binary);
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(FileReadOutcome::Text(text)),
            Err(_) => Ok(FileReadOutcome::Binary),
        }
    }

    pub fn walk(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<RawFile>, bool), FileAccessDenied> {
        let root_path = Path::new(&root.path);
        if !root_path.is_dir() {
            return Ok((Vec::new(), false));
        }
        let mut stack: Vec<PathBuf> = vec![root_path.to_path_buf()];
        let mut files: Vec<RawFile> = Vec::new();
        let mut truncated = false;
        while let Some(directory) = stack.pop() {
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                // 符号链接/特殊文件一律跳过（不解引用，避免逃逸与环）；
                // 隐藏文件不进入索引（凭据类文件多在 dotfile/隐藏目录里）。
                if !file_type.is_file() || name.starts_with('.') {
                    continue;
                }
                if files.len() >= limit {
                    truncated = true;
                    break;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                let Ok(relative) = path.strip_prefix(root_path) else {
                    continue;
                };
                files.push(RawFile {
                    path: path.clone(),
                    relative_path: relative.to_string_lossy().replace('\\', "/"),
                    size_bytes: metadata.len(),
                    modified_at: modified_timestamp(&metadata),
                    is_file: true,
                });
            }
            if truncated {
                break;
            }
        }
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok((files, truncated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &Path) -> KnowledgeRoot {
        KnowledgeRoot::new("root", "根", path.to_string_lossy().to_string())
    }

    #[test]
    fn canonicalize_resolves_and_reports_missing() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("a.md");
        std::fs::write(&file, "hi").unwrap();
        let canonical = LocalFileSystem
            .canonicalize(&file.to_string_lossy())
            .unwrap();
        assert!(canonical.is_absolute());
        assert!(canonical.ends_with("a.md"));
        assert_eq!(
            LocalFileSystem.canonicalize("/definitely/missing/file"),
            Err(FileAccessDenied::NotFound)
        );
        assert_eq!(
            LocalFileSystem.canonicalize("  "),
            Err(FileAccessDenied::NotFound)
        );
    }

    #[test]
    fn metadata_reports_size_and_kind() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("a.md");
        std::fs::write(&file, "hello").unwrap();
        let raw = LocalFileSystem.metadata(&file).unwrap();
        assert_eq!(raw.size_bytes, 5);
        assert!(raw.is_file);
        assert_eq!(raw.relative_path, "a.md");

        let missing = LocalFileSystem.metadata(&directory.path().join("nope"));
        assert_eq!(missing, Err(FileAccessDenied::NotFound));
    }

    #[test]
    fn read_text_classifies_text_binary_and_oversize() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("a.md");
        std::fs::write(&text, "Docker volume 配置").unwrap();
        assert_eq!(
            LocalFileSystem.read_text(&text, 1_000).unwrap(),
            FileReadOutcome::Text("Docker volume 配置".to_string())
        );

        let binary = directory.path().join("b.bin");
        std::fs::write(&binary, [0u8, 1, 2, 3]).unwrap();
        assert_eq!(
            LocalFileSystem.read_text(&binary, 1_000).unwrap(),
            FileReadOutcome::Binary
        );

        assert_eq!(
            LocalFileSystem.read_text(&text, 2).unwrap(),
            FileReadOutcome::TooLarge
        );

        let directory_target = LocalFileSystem.read_text(&directory.path().to_path_buf(), 1_000);
        assert_eq!(directory_target, Err(FileAccessDenied::NotAFile));
    }

    #[test]
    fn walk_lists_files_skipping_noise_dirs_and_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("notes")).unwrap();
        std::fs::create_dir_all(directory.path().join("node_modules")).unwrap();
        std::fs::create_dir_all(directory.path().join(".git")).unwrap();
        std::fs::write(directory.path().join("notes/a.md"), "a").unwrap();
        std::fs::write(directory.path().join("notes/b.pdf"), "b").unwrap();
        std::fs::write(directory.path().join("node_modules/c.md"), "c").unwrap();
        std::fs::write(directory.path().join(".git/d.md"), "d").unwrap();

        let (files, truncated) = LocalFileSystem.walk(&root(directory.path()), 100).unwrap();
        let paths: Vec<&str> = files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(paths, ["notes/a.md", "notes/b.pdf"]);
        assert!(!truncated);
    }

    #[test]
    fn walk_respects_limit_and_missing_root() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..5 {
            std::fs::write(directory.path().join(format!("f{index}.md")), "x").unwrap();
        }
        let (files, truncated) = LocalFileSystem.walk(&root(directory.path()), 2).unwrap();
        assert_eq!(files.len(), 2);
        assert!(truncated);

        let (none, truncated) = LocalFileSystem
            .walk(&KnowledgeRoot::new("x", "x", "/definitely/missing"), 10)
            .unwrap();
        assert!(none.is_empty());
        assert!(!truncated);
    }
}
