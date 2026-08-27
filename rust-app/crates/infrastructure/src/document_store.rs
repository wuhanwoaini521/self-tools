use std::{fs, path::Path};

use tempfile::NamedTempFile;

use crate::{InfrastructureError, error::io_error};

/// 以 UTF-8 读取文档，保持 Python 当前格式契约。
pub fn read_utf8(path: &Path) -> Result<String, InfrastructureError> {
    fs::read_to_string(path).map_err(|source| io_error(path, source))
}

/// 使用同目录临时文件写入，再替换目标，避免直接截断原文档。
pub fn write_utf8_atomic(path: &Path, text: &str) -> Result<(), InfrastructureError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;

    let mut temporary = NamedTempFile::new_in(parent).map_err(|source| io_error(parent, source))?;
    use std::io::Write;
    temporary
        .write_all(text.as_bytes())
        .map_err(|source| io_error(path, source))?;
    temporary.flush().map_err(|source| io_error(path, source))?;
    temporary
        .persist(path)
        .map_err(|error| io_error(path, error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{read_utf8, write_utf8_atomic};

    #[test]
    fn replaces_existing_utf8_document() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("文档.md");
        write_utf8_atomic(&path, "first").expect("first write");
        write_utf8_atomic(&path, "第二版").expect("replacement write");
        assert_eq!(read_utf8(&path).expect("read document"), "第二版");
    }
}
