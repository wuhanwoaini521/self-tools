//! 文档来源实现：允许根扫描 + 正文抽取（V6 Track B，§34/§35/§89/§92）。
//!
//! 只读：绝不写、移动或删除任何文件。抽取失败一律降级为「仅元数据」+
//! 简短原因（不把乱码或二进制塞给上层）。

use std::path::{Path, PathBuf};

use devtoolbox_core::documents::{
    DocumentType, ExtractedContent, ScannedDocument, detect_document_type, is_indexable_document,
};
use devtoolbox_core::files::KnowledgeRoot;

use crate::error::InfrastructureError;

/// 噪音目录（与 `workspace_scanner` 同源的跳过规则，另加数据/构建目录）。
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
];

/// 抽取时用于二进制判定的探测字节数。
const BINARY_PROBE_BYTES: usize = 8_192;

/// 文件系统实现（只读）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalDocumentSource;

impl LocalDocumentSource {
    pub fn scan_root(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<ScannedDocument>, bool), InfrastructureError> {
        let root_path = Path::new(&root.path);
        if !root_path.is_dir() {
            return Ok((Vec::new(), false));
        }
        let mut stack: Vec<PathBuf> = vec![root_path.to_path_buf()];
        let mut files: Vec<ScannedDocument> = Vec::new();
        let mut truncated = false;
        while let Some(directory) = stack.pop() {
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                // 单个目录不可读（权限/竞态）→ 跳过，不影响其它目录（§91）。
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                // 隐藏文件（`.env` / `.hidden.md` / `.DS_Store`…）一律不索引。
                if !file_type.is_file() || name.starts_with('.') || !is_indexable_document(&path) {
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
                files.push(ScannedDocument {
                    path: path.clone(),
                    relative_path: relative.to_string_lossy().replace('\\', "/"),
                    size_bytes: metadata.len(),
                    modified_at: modified_timestamp(&metadata),
                });
            }
            if truncated {
                break;
            }
        }
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok((files, truncated))
    }

    pub fn extract(
        &self,
        path: &Path,
        document_type: DocumentType,
        max_bytes: u64,
    ) -> Result<ExtractedContent, InfrastructureError> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| InfrastructureError::Io { path: path.to_path_buf(), source: error })?;
        if metadata.len() > max_bytes {
            return Ok(ExtractedContent::MetadataOnly(format!(
                "文件 {} 字节，超过索引上限 {max_bytes} 字节（仅索引元数据）",
                metadata.len()
            )));
        }
        match document_type {
            // PDF 抽取边界（§35）：V6 不引入重型转换平台，先如实降级。
            DocumentType::Pdf => Ok(ExtractedContent::MetadataOnly(
                "PDF 正文抽取尚未实现（仅索引元数据与文件名）".to_string(),
            )),
            DocumentType::Other => Ok(ExtractedContent::MetadataOnly(
                "不支持的文件类型（仅索引元数据）".to_string(),
            )),
            DocumentType::Markdown | DocumentType::Text | DocumentType::Json => {
                let bytes = std::fs::read(path)
                    .map_err(|error| InfrastructureError::Io { path: path.to_path_buf(), source: error })?;
                if is_binary(&bytes) {
                    return Ok(ExtractedContent::MetadataOnly(
                        "文件内容不是 UTF-8 文本（仅索引元数据）".to_string(),
                    ));
                }
                match String::from_utf8(bytes) {
                    Ok(text) => Ok(ExtractedContent::Text(text)),
                    Err(_) => Ok(ExtractedContent::MetadataOnly(
                        "文件无法按 UTF-8 解码（仅索引元数据）".to_string(),
                    )),
                }
            }
        }
    }
}

/// 修改时间（Unix 秒；不可用 → 0）。
#[must_use]
pub fn modified_timestamp(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// 二进制探测：NUL 字节即判定为二进制（HTTP/文本工具通行的保守规则）。
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_PROBE_BYTES).any(|byte| *byte == 0)
}

/// 文档类型（对外暴露，便于组合根与测试引用）。
#[must_use]
pub fn document_type_of(path: &Path) -> DocumentType {
    detect_document_type(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn root(path: &Path) -> KnowledgeRoot {
        KnowledgeRoot::new("docs", "资料", path.to_string_lossy().to_string())
    }

    #[test]
    fn scan_skips_noise_and_non_indexable_files() {
        let directory = tempfile::tempdir().unwrap();
        write(&directory.path().join("notes/docker.md"), "# docker");
        write(&directory.path().join("notes/report.txt"), "txt");
        write(&directory.path().join("data/list.json"), "{}");
        write(&directory.path().join("noise/photo.png"), "x");
        write(&directory.path().join("noise/archive.docx"), "x");
        write(&directory.path().join("node_modules/pkg/readme.md"), "x");
        write(&directory.path().join(".git/config"), "x");
        write(&directory.path().join(".hidden.md"), "x");

        let (files, truncated) = LocalDocumentSource
            .scan_root(&root(directory.path()), 100)
            .unwrap();
        let paths: Vec<&str> = files.iter().map(|file| file.relative_path.as_str()).collect();
        assert_eq!(paths, ["data/list.json", "notes/docker.md", "notes/report.txt"]);
        assert!(!truncated);
        assert!(files.iter().all(|file| file.size_bytes > 0));
    }

    #[test]
    fn scan_respects_limit_and_reports_truncation() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..5 {
            write(&directory.path().join(format!("a{index}.md")), "x");
        }
        let (files, truncated) = LocalDocumentSource
            .scan_root(&root(directory.path()), 2)
            .unwrap();
        assert_eq!(files.len(), 2);
        assert!(truncated);
    }

    #[test]
    fn scan_missing_root_is_empty_not_an_error() {
        let (files, truncated) = LocalDocumentSource
            .scan_root(&KnowledgeRoot::new("x", "x", "/definitely/missing/root"), 10)
            .unwrap();
        assert!(files.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn extract_text_formats_and_size_gate() {
        let directory = tempfile::tempdir().unwrap();
        let markdown = directory.path().join("a.md");
        write(&markdown, "# 标题\n正文");
        let extracted = LocalDocumentSource
            .extract(&markdown, DocumentType::Markdown, 1_000_000)
            .unwrap();
        assert_eq!(extracted, ExtractedContent::Text("# 标题\n正文".to_string()));

        let too_large = LocalDocumentSource
            .extract(&markdown, DocumentType::Markdown, 2)
            .unwrap();
        assert!(matches!(too_large, ExtractedContent::MetadataOnly(_)));
    }

    #[test]
    fn extract_pdf_and_binary_degrade_to_metadata_only() {
        let directory = tempfile::tempdir().unwrap();
        let pdf = directory.path().join("travel.pdf");
        std::fs::write(&pdf, b"%PDF-1.7\n...").unwrap();
        let extracted = LocalDocumentSource
            .extract(&pdf, DocumentType::Pdf, 1_000_000)
            .unwrap();
        match extracted {
            ExtractedContent::MetadataOnly(reason) => assert!(reason.contains("PDF")),
            other => panic!("PDF 必须只索引元数据，得到 {other:?}"),
        }

        let binary = directory.path().join("blob.txt");
        std::fs::write(&binary, [0u8, 1, 2, 3, 0, 255]).unwrap();
        let extracted = LocalDocumentSource
            .extract(&binary, DocumentType::Text, 1_000_000)
            .unwrap();
        assert!(matches!(extracted, ExtractedContent::MetadataOnly(_)));
        assert!(is_binary(&[0u8, 1, 2]));
        assert!(!is_binary("普通文本".as_bytes()));
    }

    #[test]
    fn extract_missing_file_is_controlled_error() {
        let error = LocalDocumentSource
            .extract(Path::new("/missing/file.md"), DocumentType::Markdown, 10)
            .unwrap_err();
        assert!(error.to_string().contains("file.md"));
    }
}
