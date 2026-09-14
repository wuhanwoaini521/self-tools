use std::path::PathBuf;

use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::workspace::WorkspaceFile;
use devtoolbox_core::{cycle_task_mark, default_registry};
use serde::{Deserialize, Serialize};

pub mod ports;

pub use ports::{
    DocumentStoreError, DocumentStorePort, SettingsStoreError, SettingsStorePort,
};

use crate::error::infrastructure;
use crate::ApplicationError;

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

pub fn load_document(
    store: &dyn DocumentStorePort,
    path: &str,
) -> Result<DocumentDto, ApplicationError> {
    let path = required_path(path, false)?;
    let text = store
        .read(&path)
        .map_err(|source| infrastructure(path.clone(), source.0))?;
    Ok(DocumentDto { path, text })
}

pub fn save_document(
    store: &dyn DocumentStorePort,
    path: &str,
    text: &str,
) -> Result<(), ApplicationError> {
    let path = required_path(path, false)?;
    store
        .write(&path, text)
        .map_err(|source| infrastructure(path, source.0))
}

pub fn scan_workspace(
    store: &dyn DocumentStorePort,
    path: &str,
) -> Result<Vec<WorkspaceFile>, ApplicationError> {
    let path = required_path(path, true)?;
    store
        .scan_markdown(&path)
        .map_err(|source| infrastructure(path, source.0))
}

pub fn load_settings(store: &dyn SettingsStorePort) -> Result<AppSettings, ApplicationError> {
    store
        .load()
        .map_err(|source| infrastructure(store.path(), source.0))
}

pub fn save_settings(
    store: &dyn SettingsStorePort,
    settings: &AppSettings,
) -> Result<(), ApplicationError> {
    store
        .save(settings)
        .map_err(|source| infrastructure(store.path(), source.0))
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
    use std::path::Path;

    use super::{cycle_lines, load_document, save_document, scan_workspace};
    use crate::workflows::ports::fakes::{InMemoryDocumentStore, InMemorySettingsStore};
    use crate::workflows::{load_settings, save_settings};

    #[test]
    fn saves_loads_and_scans_document_workflows() {
        let store = InMemoryDocumentStore::default();
        let path = Path::new("/tmp/note.md");
        save_document(&store, &path.to_string_lossy(), "- [x] done").expect("save document");
        assert_eq!(
            load_document(&store, &path.to_string_lossy())
                .expect("load document")
                .text,
            "- [x] done"
        );
        let workspace = Path::new("/tmp");
        let ignored = Path::new("/tmp/ignored.txt");
        save_document(&store, &ignored.to_string_lossy(), "x").expect("write ignored file");
        assert_eq!(
            scan_workspace(&store, &workspace.to_string_lossy())
                .expect("scan")
                .len(),
            1
        );
    }

    #[test]
    fn settings_round_trip_and_failure_propagates() {
        let mut store = InMemorySettingsStore::default();
        let mut settings = load_settings(&store).expect("default settings");
        settings.auto_save = true;
        save_settings(&store, &settings).expect("save settings");
        let reloaded = load_settings(&store).expect("reload settings");
        assert!(reloaded.auto_save);

        store.fail = true;
        let error = load_settings(&store).expect_err("fail propagates");
        assert!(error.to_string().contains("broken settings.json"));
    }

    #[test]
    fn cycles_lines() {
        assert_eq!(cycle_lines(&["- [ ] US".to_owned()], 1), ["- [~] US"]);
    }
}