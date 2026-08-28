use std::path::PathBuf;

use devtoolbox_core::{cycle_task_mark, default_registry, make_task_line};
use devtoolbox_infrastructure::{
    AppSettings, SettingsStore, WorkspaceFile, read_utf8, scan_markdown_files, write_utf8_atomic,
};
use serde::{Deserialize, Serialize};

use crate::{ApplicationError, error::infrastructure};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DocumentDto {
    pub path: PathBuf,
    pub text: String,
}

fn required_path(path: &str, is_workspace: bool) -> Result<PathBuf, ApplicationError> {
    if path.trim().is_empty() {
        return Err(if is_workspace {
            ApplicationError::EmptyWorkspacePath
        } else {
            ApplicationError::EmptyDocumentPath
        });
    }
    Ok(PathBuf::from(path))
}

pub fn load_document(path: &str) -> Result<DocumentDto, ApplicationError> {
    let path = required_path(path, false)?;
    let text = read_utf8(&path).map_err(|source| infrastructure(path.clone(), source))?;
    Ok(DocumentDto { path, text })
}

pub fn save_document(path: &str, text: &str) -> Result<(), ApplicationError> {
    let path = required_path(path, false)?;
    write_utf8_atomic(&path, text).map_err(|source| infrastructure(path, source))
}

pub fn scan_workspace(path: &str) -> Result<Vec<WorkspaceFile>, ApplicationError> {
    let path = required_path(path, true)?;
    scan_markdown_files(&path).map_err(|source| infrastructure(path, source))
}

pub fn load_settings(store: &SettingsStore) -> Result<AppSettings, ApplicationError> {
    store
        .load()
        .map_err(|source| infrastructure(store.path().to_owned(), source))
}

pub fn save_settings(
    store: &SettingsStore,
    settings: &AppSettings,
) -> Result<(), ApplicationError> {
    store
        .save(settings)
        .map_err(|source| infrastructure(store.path().to_owned(), source))
}

#[must_use]
pub fn convert_lines_to_tasks(lines: &[String]) -> Vec<String> {
    let registry = default_registry();
    lines
        .iter()
        .map(|line| make_task_line(line, &registry.first().mark))
        .collect()
}

#[must_use]
pub fn cycle_lines(lines: &[String], step: isize) -> Vec<String> {
    let registry = default_registry();
    lines
        .iter()
        .map(|line| cycle_task_mark(line, &registry, step).0)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        convert_lines_to_tasks, cycle_lines, load_document, save_document, scan_workspace,
    };

    #[test]
    fn saves_loads_and_scans_document_workflows() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("note.md");
        let path_text = path.to_string_lossy();
        save_document(&path_text, "- [x] done").expect("save document");
        assert_eq!(
            load_document(&path_text).expect("load document").text,
            "- [x] done"
        );
        fs::write(directory.path().join("ignored.txt"), "x").expect("write ignored file");
        assert_eq!(
            scan_workspace(&directory.path().to_string_lossy())
                .expect("scan")
                .len(),
            1
        );
    }

    #[test]
    fn converts_and_cycles_lines() {
        assert_eq!(convert_lines_to_tasks(&["US".to_owned()]), ["- [ ] US"]);
        assert_eq!(cycle_lines(&["- [ ] US".to_owned()], 1), ["- [~] US"]);
    }
}
