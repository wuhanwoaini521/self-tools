//! `Tauri` command adapter。业务规则位于 workspace 的 application/core crates。
//!
//! 全局 `AppState` 持有 RSS 的 SQLite 存储器与共享 HTTP 客户端；
//! 每个功能模块(文档 / RSS)的命令各自独立，互不依赖。

use std::sync::Mutex;

use devtoolbox_application::{
    ApplicationError, ArticleDto, DocumentDto, FeedDto, RefreshReport, commit_new_feed,
    commit_refresh, convert_lines_to_tasks, cycle_lines, delete_feed, feed_snapshots,
    fetch_all_feeds, fetch_new_feed, latest_articles, list_articles, list_feeds, load_document,
    load_settings, mark_article_read, save_document, save_settings, scan_workspace,
    validate_feed_url,
};
use devtoolbox_infrastructure::{
    AppSettings, FeedRepository, SettingsStore, WorkspaceFile, feed_client,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        let code = match &error {
            ApplicationError::EmptyDocumentPath => "empty_document_path",
            ApplicationError::EmptyWorkspacePath => "empty_workspace_path",
            ApplicationError::InvalidFeedUrl(_) => "rss_invalid_url",
            ApplicationError::DuplicateFeed(_) => "rss_duplicate_feed",
            ApplicationError::FeedNotFound(_) => "rss_feed_not_found",
            ApplicationError::Rss { source } => match source {
                devtoolbox_infrastructure::InfrastructureError::FeedFetch(_) => "rss_fetch_failed",
                devtoolbox_infrastructure::InfrastructureError::FeedParse(_) => "rss_parse_failed",
                _ => "infrastructure_error",
            },
            ApplicationError::Infrastructure { .. } => "infrastructure_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

/// 应用级共享状态：RSS 存储 + HTTP 客户端。
pub struct AppState {
    pub store: Mutex<FeedRepository>,
    pub client: reqwest::Client,
}

fn settings_store(app: &AppHandle) -> Result<SettingsStore, CommandError> {
    let config_directory = app.path().app_config_dir().map_err(|error| CommandError {
        code: "app_config_dir",
        message: error.to_string(),
    })?;
    Ok(SettingsStore::new(config_directory))
}

// ---------- 文档 / Markdown 模块 ----------

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

// ---------- RSS 模块 ----------

#[tauri::command]
async fn add_rss_feed(state: State<'_, AppState>, url: String) -> Result<FeedDto, CommandError> {
    // 两段式：先无锁抓取(可跨 await),再短锁落库。
    let normalized = validate_feed_url(&url).map_err(CommandError::from)?;
    let fetched = fetch_new_feed(&normalized, &state.client)
        .await
        .map_err(CommandError::from)?;
    let store = state.store.lock().expect("rss store poisoned");
    commit_new_feed(&store, &normalized, fetched).map_err(CommandError::from)
}

#[tauri::command]
async fn refresh_rss_feeds(state: State<'_, AppState>) -> Result<RefreshReport, CommandError> {
    // 快照 → 并发抓取(无锁) → 短锁落库;单个 Feed 失败不影响其他。
    let snapshots = {
        let store = state.store.lock().expect("rss store poisoned");
        feed_snapshots(&store).map_err(CommandError::from)?
    };
    let results = fetch_all_feeds(&snapshots, &state.client).await;
    let store = state.store.lock().expect("rss store poisoned");
    commit_refresh(&store, results).map_err(CommandError::from)
}

#[tauri::command]
fn list_rss_feeds(state: State<'_, AppState>) -> Result<Vec<FeedDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    list_feeds(&store).map_err(CommandError::from)
}

#[tauri::command]
fn list_rss_articles(
    state: State<'_, AppState>,
    feed_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    list_articles(&store, feed_id, limit.unwrap_or(200)).map_err(CommandError::from)
}

#[tauri::command]
fn latest_rss_articles(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ArticleDto>, CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    latest_articles(&store, limit.unwrap_or(5)).map_err(CommandError::from)
}

#[tauri::command]
fn mark_rss_article_read(state: State<'_, AppState>, article_id: i64) -> Result<(), CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    mark_article_read(&store, article_id).map_err(CommandError::from)
}

#[tauri::command]
fn delete_rss_feed(state: State<'_, AppState>, feed_id: i64) -> Result<(), CommandError> {
    let store = state.store.lock().expect("rss store poisoned");
    delete_feed(&store, feed_id).map_err(CommandError::from)
}

/// 应用入口。前端需要的权限被限制在文件选择器、command API 与打开原文链接；
/// 不暴露任意 shell 执行能力。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_directory = app.path().app_config_dir()?;
            let store = FeedRepository::open(config_directory.join("dashboard.db"))
                .expect("open rss database");
            let client = feed_client().expect("build http client");
            app.manage(AppState {
                store: Mutex::new(store),
                client,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_document,
            write_document,
            list_workspace,
            convert_task_lines,
            cycle_task_lines,
            get_settings,
            put_settings,
            add_rss_feed,
            refresh_rss_feeds,
            list_rss_feeds,
            list_rss_articles,
            latest_rss_articles,
            mark_rss_article_read,
            delete_rss_feed
        ])
        .run(tauri::generate_context!())
        .expect("Tauri application event loop failed");
}
