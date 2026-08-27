//! `Tauri` command adapter。业务规则位于 workspace 的 application/core crates。

use devtoolbox_application::{
    ApplicationError, DocumentDto, convert_lines_to_tasks, cycle_lines, load_document,
    load_settings, save_document, save_settings, scan_workspace,
};
use devtoolbox_infrastructure::{AppSettings, SettingsStore, WorkspaceFile};
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        let code = match error {
            ApplicationError::EmptyDocumentPath => "empty_document_path",
            ApplicationError::EmptyWorkspacePath => "empty_workspace_path",
            ApplicationError::Infrastructure { .. } => "infrastructure_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

fn settings_store(app: &AppHandle) -> Result<SettingsStore, CommandError> {
    let config_directory = app.path().app_config_dir().map_err(|error| CommandError {
        code: "app_config_dir",
        message: error.to_string(),
    })?;
    Ok(SettingsStore::new(config_directory))
}

#[tauri::command]
fn read_document(path: String) -> Result<DocumentDto, CommandError> {
    load_document(&path).map_err(CommandError::from)
}

#[tauri::command]
fn write_document(path: String, text: String) -> Result<(), CommandError> {
    save_document(&path, &text).map_err(CommandError::from)
}

#[tauri::command]
fn list_workspace(path: String) -> Result<Vec<WorkspaceFile>, CommandError> {
    scan_workspace(&path).map_err(CommandError::from)
}

#[tauri::command]
fn convert_task_lines(lines: Vec<String>) -> Vec<String> {
    convert_lines_to_tasks(&lines)
}

#[tauri::command]
fn cycle_task_lines(lines: Vec<String>, step: isize) -> Vec<String> {
    cycle_lines(&lines, step)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, CommandError> {
    let store = settings_store(&app)?;
    load_settings(&store).map_err(CommandError::from)
}

#[tauri::command]
fn put_settings(app: AppHandle, settings: AppSettings) -> Result<(), CommandError> {
    let store = settings_store(&app)?;
    save_settings(&store, &settings).map_err(CommandError::from)
}

/// 应用入口。前端需要的权限被限制在文件选择器和 command API；不暴露 shell 执行能力。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            read_document,
            write_document,
            list_workspace,
            convert_task_lines,
            cycle_task_lines,
            get_settings,
            put_settings
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application event loop failed");
}
