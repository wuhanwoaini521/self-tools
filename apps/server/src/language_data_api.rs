//! 英语课程**数据导入**的 HTTP 接口。
//!
//! ## 为什么必须有
//!
//! NCE 教材与 ECDICT 词典的导入能力一直在 infrastructure 里，但只由桌面端
//! 命令调用。网页端想导入数据会得到 `「language_nce_import」尚无网页端接口`，
//! 于是「这台机器就是服务器」却只能靠人去点桌面应用导一次。
//!
//! 现在两端调用**同一批函数**、写**同一个** `language.db` 与 `language/nce/`。
//!
//! ## 长任务
//!
//! NCE 导入与 ECDICT 导入都是长任务。这里用**同步 + `spawn_blocking`** 的方式：
//! 请求线程立即让出，实际工作在线程池里跑（导入器自身是纯同步 + 短锁），
//! 进度仍由前端轮询 `status` 端点获取（与桌面端的进度事件等价的网页形态）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::{Extension, Json};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::ai_api::ApiError;

fn err(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError {
        code,
        message: message.into(),
    }
}

/// 导入状态（供前端轮询）。
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ImportStatus {
    pub running: bool,
    pub nce_books: usize,
    pub nce_lessons: usize,
    pub nce_sentences: usize,
    pub nce_vocab: usize,
    pub media_files: usize,
    pub issues: Vec<String>,
    pub last_error: Option<String>,
}

/// 进程内导入状态（单例；同一时刻只允许一个导入任务）。
#[derive(Default)]
pub struct ImportTracker {
    inner: Mutex<ImportStatus>,
    busy: Mutex<bool>,
}

impl ImportTracker {
    fn begin(&self) -> bool {
        let mut busy = self.busy.lock();
        if *busy {
            return false;
        }
        *busy = true;
        self.inner.lock().running = true;
        self.inner.lock().last_error = None;
        true
    }

    fn fail(&self, message: String) {
        let mut status = self.inner.lock();
        status.running = false;
        status.last_error = Some(message);
    }

    fn record_nce(&self, report: &devtoolbox_infrastructure::language::NceImportReport) {
        let mut status = self.inner.lock();
        status.nce_books = report.books;
        status.nce_lessons = report.lessons;
        status.nce_sentences = report.sentences;
        status.nce_vocab = report.vocab;
        status.media_files = report.media_files;
        status.issues = report.issues.clone();
        status.running = false;
    }

    fn record_dict(&self, report: &devtoolbox_infrastructure::language::DictImportReport) {
        let mut status = self.inner.lock();
        if report.cancelled {
            status.running = false;
            return;
        }
        status.running = false;
    }

    fn snapshot(&self) -> ImportStatus {
        self.inner.lock().clone()
    }
}

/// 数据目录（与桌面端同一处：`config/`）。
#[derive(Clone)]
pub struct DataPaths {
    pub config_dir: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBody {
    pub source_dir: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvBody {
    pub path: String,
}

/// `GET /api/v1/language/data-status` —— 教材/词典现状（数据在哪、缺什么）。
pub async fn data_status(
    Extension(paths): Extension<Arc<DataPaths>>,
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let adapter = devtoolbox_infrastructure::ports::CourseStoreAdapter::new(Arc::clone(&store));
    let status = devtoolbox_application::language::course::data_status(&adapter, &paths.config_dir);
    Ok(Json(serde_json::to_value(status).map_err(|error| {
        err("language_encode", error.to_string())
    })?))
}

/// `POST /api/v1/language/nce/scan` —— 扫描教材目录（只读预览）。
pub async fn nce_scan(
    Json(body): Json<SourceBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let root = PathBuf::from(&body.source_dir);
    if !root.is_dir() {
        return Err(err(
            "language_nce_bad_dir",
            format!("{} 不是一个目录", body.source_dir),
        ));
    }
    let report = devtoolbox_infrastructure::language::scan_nce_source(&root);
    Ok(Json(serde_json::to_value(report).map_err(|error| {
        err("language_encode", error.to_string())
    })?))
}

/// `POST /api/v1/language/nce/import` —— 导入教材（长任务，线程池执行）。
pub async fn nce_import(
    Extension(paths): Extension<Arc<DataPaths>>,
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
    Extension(tracker): Extension<Arc<ImportTracker>>,
    Json(body): Json<SourceBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let source = PathBuf::from(&body.source_dir);
    if !source.is_dir() {
        return Err(err(
            "language_nce_bad_dir",
            format!("{} 不是一个目录", body.source_dir),
        ));
    }
    if !tracker.begin() {
        return Err(err("language_import_busy", "已有导入任务在进行中"));
    }
    let media_dir = paths.config_dir.join("language").join("nce");
    let store_for_task = Arc::clone(&store);
    let tracker_for_task = Arc::clone(&tracker);
    // rusqlite Connection 非 Send：交给 spawn_blocking 在工作线程里执行。
    drop(tokio::task::spawn_blocking(move || {
        let options = devtoolbox_infrastructure::language::NceImport {
            source_dir: source,
            media_dir,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            on_progress: Box::new(|_| {}),
        };
        let guard = store_for_task.lock();
        match devtoolbox_infrastructure::language::import_nce(&guard, &options) {
            Ok(report) => tracker_for_task.record_nce(&report),
            Err(message) => tracker_for_task.fail(message),
        }
    }));
    Ok(axum::Json(serde_json::json!({ "started": true })))
}

/// `POST /api/v1/language/dict/import` —— 导入 ECDICT csv（长任务）。
pub async fn dict_import(
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
    Extension(tracker): Extension<Arc<ImportTracker>>,
    Json(body): Json<CsvBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let source = PathBuf::from(&body.path);
    if !source.is_file() {
        return Err(err(
            "language_dict_bad_file",
            format!("{} 不是文件", body.path),
        ));
    }
    if !tracker.begin() {
        return Err(err("language_import_busy", "已有导入任务在进行中"));
    }
    let store_for_task = Arc::clone(&store);
    let tracker_for_task = Arc::clone(&tracker);
    drop(tokio::task::spawn_blocking(move || {
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = store_for_task.lock();
        match devtoolbox_infrastructure::language::import_ecdict_csv(
            &guard,
            &source,
            &cancel,
            &|_| {},
        ) {
            Ok(report) => tracker_for_task.record_dict(&report),
            Err(message) => tracker_for_task.fail(message),
        }
    }));
    Ok(axum::Json(serde_json::json!({ "started": true })))
}

/// `GET /api/v1/language/import-status` —— 导入进度（前端轮询）。
pub async fn import_status(
    Extension(tracker): Extension<Arc<ImportTracker>>,
) -> axum::Json<ImportStatus> {
    axum::Json(tracker.snapshot())
}

/// `GET /api/v1/language/dict/status` —— 词典是否就绪。
pub async fn dict_status(
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
) -> axum::Json<serde_json::Value> {
    let count = store.lock().dict_count().unwrap_or(0);
    axum::Json(serde_json::json!({ "ready": count > 0, "count": count }))
}

/// `DELETE /api/v1/language/course/lesson/{id}` —— 删除自建课时（不影响进度统计）。
pub async fn delete_lesson(
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    store
        .lock()
        .delete_lesson(&lesson_id)
        .map_err(|error| err("language_error", error.to_string()))?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}
