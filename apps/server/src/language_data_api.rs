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

/// 导入任务种类（状态端点与审计共用）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Nce,
    Dict,
}

/// 导入状态（供前端轮询）。
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatus {
    pub running: bool,
    /// 当前（或最近一次）任务的 id；取消端点用它定位任务。
    pub job_id: Option<String>,
    pub kind: Option<ImportKind>,
    pub nce_books: usize,
    pub nce_lessons: usize,
    pub nce_sentences: usize,
    pub nce_vocab: usize,
    pub media_files: usize,
    pub dict_entries: usize,
    pub issues: Vec<String>,
    pub last_error: Option<String>,
    /// 最近一次任务是否被用户取消（取消不是错误）。
    pub cancelled: bool,
}

struct ActiveJob {
    id: String,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

/// 进程内导入状态（单例；同一时刻只允许一个导入任务）。
///
/// 取消标志随任务一起存在这里：`begin` 返回**该任务自己的** `AtomicBool`，
/// 取消端点只能通过 job id 命中当前任务，旧 id 无法影响新任务。
#[derive(Default)]
pub struct ImportTracker {
    inner: Mutex<ImportStatus>,
    job: Mutex<Option<ActiveJob>>,
}

impl ImportTracker {
    fn begin(&self, kind: ImportKind) -> Option<(String, Arc<std::sync::atomic::AtomicBool>)> {
        let mut job = self.job.lock();
        if job.is_some() {
            return None;
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        *job = Some(ActiveJob {
            id: id.clone(),
            cancel: Arc::clone(&cancel),
        });
        let mut status = self.inner.lock();
        status.running = true;
        status.job_id = Some(id.clone());
        status.kind = Some(kind);
        status.last_error = None;
        status.cancelled = false;
        drop(status);
        drop(job);
        Some((id, cancel))
    }

    /// 结束后释放任务槽（job id 保留在状态里，便于前端展示"已完成/已取消"）。
    fn finish(&self, job_id: &str) {
        let mut job = self.job.lock();
        if job.as_ref().is_some_and(|active| active.id == job_id) {
            *job = None;
        }
        let mut status = self.inner.lock();
        status.running = false;
    }

    fn fail(&self, job_id: &str, message: String) {
        self.finish(job_id);
        self.inner.lock().last_error = Some(message);
    }

    fn record_nce(
        &self,
        job_id: &str,
        report: &devtoolbox_infrastructure::language::NceImportReport,
    ) {
        {
            let mut status = self.inner.lock();
            status.nce_books = report.books;
            status.nce_lessons = report.lessons;
            status.nce_sentences = report.sentences;
            status.nce_vocab = report.vocab;
            status.media_files = report.media_files;
            status.issues = report.issues.clone();
        }
        self.finish(job_id);
    }

    fn record_dict(
        &self,
        job_id: &str,
        report: &devtoolbox_infrastructure::language::DictImportReport,
    ) {
        self.finish(job_id);
        let mut status = self.inner.lock();
        status.dict_entries = report.entries;
        if report.cancelled {
            status.cancelled = true;
        }
    }

    /// 只取消当前任务：旧 job id 或已完成的任务返回 `None`（404）。
    fn cancel(&self, job_id: &str) -> Option<bool> {
        let job = self.job.lock();
        let active = job.as_ref().filter(|active| active.id == job_id)?;
        active
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Some(true)
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
    let Some((job_id, cancel)) = tracker.begin(ImportKind::Nce) else {
        return Err(err("language_import_busy", "已有导入任务在进行中"));
    };
    let media_dir = paths.config_dir.join("language").join("nce");
    let store_for_task = Arc::clone(&store);
    let tracker_for_task = Arc::clone(&tracker);
    let job_for_task = job_id.clone();
    // rusqlite Connection 非 Send：交给 spawn_blocking 在工作线程里执行。
    drop(tokio::task::spawn_blocking(move || {
        let options = devtoolbox_infrastructure::language::NceImport {
            source_dir: source,
            media_dir,
            cancel,
            on_progress: Box::new(|_| {}),
        };
        let guard = store_for_task.lock();
        match devtoolbox_infrastructure::language::import_nce(&guard, &options) {
            Ok(report) => tracker_for_task.record_nce(&job_for_task, &report),
            Err(message) => tracker_for_task.fail(&job_for_task, message),
        }
    }));
    Ok(axum::Json(
        serde_json::json!({ "started": true, "jobId": job_id }),
    ))
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
    let Some((job_id, cancel)) = tracker.begin(ImportKind::Dict) else {
        return Err(err("language_import_busy", "已有导入任务在进行中"));
    };
    let store_for_task = Arc::clone(&store);
    let tracker_for_task = Arc::clone(&tracker);
    let job_for_task = job_id.clone();
    drop(tokio::task::spawn_blocking(move || {
        let guard = store_for_task.lock();
        match devtoolbox_infrastructure::language::import_ecdict_csv(
            &guard,
            &source,
            &cancel,
            &|_| {},
        ) {
            Ok(report) => tracker_for_task.record_dict(&job_for_task, &report),
            Err(message) => tracker_for_task.fail(&job_for_task, message),
        }
    }));
    Ok(axum::Json(
        serde_json::json!({ "started": true, "jobId": job_id }),
    ))
}

/// `POST /api/v1/language/nce/import/{jobId}/cancel`
pub async fn nce_import_cancel(
    Extension(tracker): Extension<Arc<ImportTracker>>,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    cancel_job(&tracker, ImportKind::Nce, &job_id)
}

/// `POST /api/v1/language/dict/import/{jobId}/cancel`
pub async fn dict_import_cancel(
    Extension(tracker): Extension<Arc<ImportTracker>>,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    cancel_job(&tracker, ImportKind::Dict, &job_id)
}

/// 只允许取消**当前正在跑**的那个任务；种类不符或已结束都是 404。
fn cancel_job(
    tracker: &ImportTracker,
    kind: ImportKind,
    job_id: &str,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let snapshot = tracker.snapshot();
    if snapshot.kind != Some(kind) || !snapshot.running {
        return Err(err("job_not_found", "该导入任务不存在或已经结束"));
    }
    match tracker.cancel(job_id) {
        Some(_) => Ok(axum::Json(
            serde_json::json!({ "cancelled": true, "jobId": job_id }),
        )),
        None => Err(err("job_not_found", "该导入任务不存在或已经结束")),
    }
}

/// `POST /api/v1/language/starter/install` —— 安装内置 Starter 数据（幂等）。
pub async fn starter_install(
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
    Json(body): Json<StarterBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let only = body.only;
    let report = tokio::task::spawn_blocking(move || {
        let mut guard = store.lock();
        devtoolbox_infrastructure::language::starter::install_starter(&mut guard, only.as_deref())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| err("language_error", error.to_string()))?
    .map_err(|message| err("language_error", message))?;
    serde_json::to_value(report)
        .map(axum::Json)
        .map_err(|error| err("language_encode", error.to_string()))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StarterBody {
    pub only: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeakingFeedbackBody {
    pub sentence_id: String,
    pub transcript: String,
    pub duration_ms: u64,
    pub target_ms: u64,
    pub long_pauses_ms: Vec<u64>,
}

/// `POST /api/v1/language/speaking/feedback` —— 跟读评分。
///
/// 目标句由 `sentenceId` 在服务端查库得到，客户端**不能**自带 `target`：
/// 否则「评分」变成调用方自己挑一句最顺口的话来对比。
pub async fn speaking_feedback(
    Extension(store): Extension<Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>>,
    Json(body): Json<SpeakingFeedbackBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let sentence_id = body.sentence_id.trim();
    if sentence_id.is_empty() {
        return Err(err("language_speaking_invalid", "缺少 sentenceId"));
    }
    if body.transcript.trim().is_empty() {
        return Err(err("language_speaking_empty_turn", "转写为空，无法评分"));
    }
    if body.transcript.chars().count() > 2_000 || body.long_pauses_ms.len() > 256 {
        return Err(err("language_speaking_invalid", "评分请求超出长度上限"));
    }
    let target = store
        .lock()
        .item(sentence_id)
        .map_err(|error| err("language_error", error.to_string()))?
        .map(|item| item.text);
    let Some(target) = target else {
        return Err(err("not_found", format!("句子 {sentence_id} 不存在")));
    };
    let score = devtoolbox_core::language::score(
        &target,
        &body.transcript,
        body.duration_ms,
        body.target_ms,
        &body.long_pauses_ms,
    );
    serde_json::to_value(score)
        .map(axum::Json)
        .map_err(|error| err("language_encode", error.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_only_matches_the_active_job() {
        let tracker = ImportTracker::default();
        let (first, cancel) = tracker.begin(ImportKind::Nce).expect("job starts");
        assert!(tracker.snapshot().running);
        assert_eq!(tracker.snapshot().job_id.as_deref(), Some(first.as_str()));

        // 旧 id / 乱填 id 都不能命中。
        assert!(tracker.cancel("not-the-job").is_none());
        assert!(!cancel.load(std::sync::atomic::Ordering::SeqCst));

        assert!(tracker.cancel(&first).is_some());
        assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));

        // 第二个任务在进行中时，第一个 id 已失效。
        tracker.finish(&first);
        let (second, _) = tracker.begin(ImportKind::Dict).expect("second job");
        assert_ne!(first, second);
        assert!(tracker.cancel(&first).is_none());
        assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn only_one_import_may_run_at_a_time() {
        let tracker = ImportTracker::default();
        let (job, _) = tracker.begin(ImportKind::Nce).expect("first job");
        assert!(tracker.begin(ImportKind::Dict).is_none());
        tracker.finish(&job);
        assert!(!tracker.snapshot().running);
        assert!(tracker.begin(ImportKind::Dict).is_some());
    }

    #[tokio::test]
    async fn speaking_feedback_never_trusts_a_client_supplied_target() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Mutex::new(
            devtoolbox_infrastructure::language::LanguageStore::open(
                directory.path().join("language.db"),
            )
            .expect("store"),
        ));

        let empty_sentence = speaking_feedback(
            Extension(Arc::clone(&store)),
            Json(SpeakingFeedbackBody::default()),
        )
        .await
        .expect_err("sentenceId required");
        assert_eq!(empty_sentence.code, "language_speaking_invalid");

        let empty_turn = speaking_feedback(
            Extension(Arc::clone(&store)),
            Json(SpeakingFeedbackBody {
                sentence_id: "sentence-1".into(),
                transcript: "   ".into(),
                ..SpeakingFeedbackBody::default()
            }),
        )
        .await
        .expect_err("empty transcript");
        assert_eq!(empty_turn.code, "language_speaking_empty_turn");

        let missing = speaking_feedback(
            Extension(Arc::clone(&store)),
            Json(SpeakingFeedbackBody {
                sentence_id: "sentence-unknown".into(),
                transcript: "hello".into(),
                ..SpeakingFeedbackBody::default()
            }),
        )
        .await
        .expect_err("unknown sentence");
        assert_eq!(missing.code, "not_found");
        assert!(missing.message.contains("sentence-unknown"));
    }
}
