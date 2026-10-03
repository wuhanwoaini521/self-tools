//! 英语课程学习子域的 Tauri 命令。
//!
//! 参数契约：Rust 侧参数用 snake_case（Tauri v2 自动把前端 camelCase `lessonId`
//! 映射过来），与 `language.rs` 一致。
//! 导入类命令是长任务：`async` + 进度事件（`language-nce-progress` / `language-dict-progress`）
//! + 可取消 flag；扫描（scan）先于导入，便于用户确认目录是否正确。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use devtoolbox_application::language::course::{
    BookView, CourseService, DataStatus, EnglishProgress, EnglishSearchResult, LessonDetail,
    ProgressPatch, TodayDashboard, WordLookup,
};
use devtoolbox_application::language::{CourseStorePort, DictionaryService};
use devtoolbox_application::learning::LearningService as PlatformLearningService;
use devtoolbox_core::language::{
    CourseBook, LearningPlan, LessonProgress, QuizAnswer, QuizItem, QuizResult, WordMark,
};

use crate::{AppState, CommandError, composition};

/// NCE 导入取消 flag（全局单例：同一时间只允许一个导入任务）。
static NCE_CANCEL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();
/// ECDICT 导入取消 flag。
static DICT_CANCEL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn nce_cancel() -> &'static Arc<AtomicBool> {
    NCE_CANCEL.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

fn dict_cancel() -> &'static Arc<AtomicBool> {
    DICT_CANCEL.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

/// 组合根装配：课程端口 → CourseService（+ DictionaryService）。
pub fn course_service(state: &State<'_, AppState>) -> CourseService {
    let store: Arc<dyn CourseStorePort> = Arc::new(composition::CourseStoreAdapter::new(
        Arc::clone(&state.language_store),
    ));
    let platform = Arc::new(PlatformLearningService::new(Arc::clone(
        &state.learning_store,
    )));
    CourseService::new(store, platform)
}

fn dictionary_service(state: &State<'_, AppState>) -> DictionaryService {
    let store: Arc<dyn CourseStorePort> = Arc::new(composition::CourseStoreAdapter::new(
        Arc::clone(&state.language_store),
    ));
    DictionaryService::new(store)
}

fn now() -> i64 {
    devtoolbox_infrastructure::now_unix()
}

fn language_err(code: &'static str, message: impl Into<String>) -> CommandError {
    CommandError {
        code,
        message: message.into(),
    }
}

// ============================================================================
// Today / 课程浏览
// ============================================================================

/// English 首页驾驶舱（今天该学什么）。
#[tauri::command]
pub fn language_course_today(state: State<'_, AppState>) -> Result<TodayDashboard, CommandError> {
    course_service(&state)
        .today(now())
        .map_err(CommandError::from)
}

/// 课程书册列表（New Concept English 1–4）。
#[tauri::command]
pub fn language_course_books(state: State<'_, AppState>) -> Result<Vec<CourseBook>, CommandError> {
    course_service(&state).library().map_err(CommandError::from)
}

/// 一册书的课时列表 + 汇总。
#[tauri::command]
pub fn language_course_book(
    state: State<'_, AppState>,
    book_id: String,
) -> Result<Option<BookView>, CommandError> {
    course_service(&state)
        .book_view(&book_id)
        .map_err(CommandError::from)
}

/// 一课的完整学习数据（句子 / 生词 / 进度）。
#[tauri::command]
pub fn language_course_lesson(
    state: State<'_, AppState>,
    lesson_id: String,
) -> Result<Option<LessonDetail>, CommandError> {
    course_service(&state)
        .lesson_detail(&lesson_id, now())
        .map_err(CommandError::from)
}

// ============================================================================
// 学习进度
// ============================================================================

/// 课时进度心跳（阶段 / 音频位置 / 句子 / 跟读位置 / 学习秒数）。
#[tauri::command]
pub fn language_course_update_progress(
    state: State<'_, AppState>,
    lesson_id: String,
    patch: ProgressPatch,
) -> Result<LessonProgress, CommandError> {
    course_service(&state)
        .update_lesson_progress(&lesson_id, patch, now())
        .map_err(CommandError::from)
}

/// 完成一课（生成明天复习本课的平台卡片）。
#[tauri::command]
pub fn language_course_complete_lesson(
    state: State<'_, AppState>,
    lesson_id: String,
    quiz_score: Option<u32>,
) -> Result<LessonProgress, CommandError> {
    course_service(&state)
        .complete_lesson(&lesson_id, quiz_score, now())
        .map_err(CommandError::from)
}

// ============================================================================
// 单词
// ============================================================================

/// 课前单词三态标记（know / fuzzy / unknown）→ 直接进入平台 SRS。
#[tauri::command]
pub fn language_course_mark_word(
    state: State<'_, AppState>,
    lesson_id: String,
    word: String,
    mark: String,
) -> Result<devtoolbox_core::learning::LearningProgress, CommandError> {
    let parsed = WordMark::parse(&mark)
        .ok_or_else(|| language_err("language_invalid_mark", format!("unknown mark: {mark}")))?;
    course_service(&state)
        .mark_word(&lesson_id, &word, parsed, now())
        .map_err(CommandError::from)
}

/// 查词（词典 + 遇见历史 + 学习状态），并记一次 lookup occurrence。
#[tauri::command]
pub fn language_course_lookup_word(
    state: State<'_, AppState>,
    word: String,
    sentence: Option<String>,
    lesson_id: Option<String>,
) -> Result<WordLookup, CommandError> {
    course_service(&state)
        .lookup_word(&word, sentence.as_deref(), lesson_id.as_deref(), now())
        .map_err(CommandError::from)
}

/// 词典查询（轻量路径；Lesson 内点击单词用，避免拉历史）。
#[tauri::command]
pub fn language_dict_lookup(
    state: State<'_, AppState>,
    word: String,
) -> Result<Option<devtoolbox_core::language::WordEntry>, CommandError> {
    dictionary_service(&state)
        .lookup(&word)
        .map_err(CommandError::from)
}

/// 词典状态（是否就绪 / 词条数）。
#[derive(Debug, Serialize)]
pub struct DictStatus {
    pub ready: bool,
    pub count: i64,
}

#[tauri::command]
pub fn language_dict_status(state: State<'_, AppState>) -> Result<DictStatus, CommandError> {
    // 直接问端口词条数；空串 search 恒空，不能当计数用。
    let store: Arc<dyn CourseStorePort> = Arc::new(composition::CourseStoreAdapter::new(
        Arc::clone(&state.language_store),
    ));
    let total = store
        .dict_count()
        .map_err(|error| language_err("language_error", error))?;
    Ok(DictStatus {
        ready: total > 0,
        count: total,
    })
}

// ============================================================================
// Quiz
// ============================================================================

/// 生成课末 Quiz（词汇 / 填空 / 听写 / 翻译，全部来自本课真实数据）。
#[tauri::command]
pub fn language_course_quiz(
    state: State<'_, AppState>,
    lesson_id: String,
) -> Result<Vec<QuizItem>, CommandError> {
    course_service(&state)
        .generate_quiz(&lesson_id)
        .map_err(CommandError::from)
}

/// 提交 Quiz（计分 + 错词进 SRS + 完成判定）。
#[tauri::command]
pub fn language_course_submit_quiz(
    state: State<'_, AppState>,
    lesson_id: String,
    answers: Vec<QuizAnswer>,
) -> Result<QuizResult, CommandError> {
    course_service(&state)
        .submit_quiz(&lesson_id, &answers, now())
        .map_err(CommandError::from)
}

// ============================================================================
// 计划 / 统计 / 搜索
// ============================================================================

#[tauri::command]
pub fn language_course_plan_get(
    state: State<'_, AppState>,
) -> Result<Option<LearningPlan>, CommandError> {
    course_service(&state)
        .get_plan()
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_course_plan_save(
    state: State<'_, AppState>,
    plan: LearningPlan,
) -> Result<(), CommandError> {
    let mut plan = plan;
    plan.updated_at = now();
    course_service(&state)
        .save_plan(&plan)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_course_progress(
    state: State<'_, AppState>,
) -> Result<EnglishProgress, CommandError> {
    course_service(&state)
        .english_progress(now())
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_course_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<EnglishSearchResult, CommandError> {
    course_service(&state)
        .search(&query, limit.unwrap_or(10))
        .map_err(CommandError::from)
}

// ============================================================================
// 音频
// ============================================================================

/// 读取课时音频字节（纯逻辑，便于单测；命令层只负责包装成 IPC 响应）。
///
/// 路径只来自数据库里的 `audio_path`（由导入器写入用户数据目录），不接受前端传入的路径。
pub(crate) fn read_lesson_audio(
    store: &devtoolbox_infrastructure::language::LanguageStore,
    lesson_id: &str,
) -> Result<Vec<u8>, (&'static str, String)> {
    let lesson = store
        .course_lesson(lesson_id)
        .map_err(|error| ("language_error", error.to_string()))?
        .ok_or(("language_lesson_not_found", lesson_id.to_string()))?;
    let path = lesson
        .audio_path
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                "language_audio_missing",
                "lesson has no audio (was .mp3 imported?)".to_string(),
            )
        })?;
    std::fs::read(&path).map_err(|error| ("language_audio_read", format!("{path}: {error}")))
}

/// 读取课时音频二进制（前端转 Blob URL；离线可用，不依赖 asset 协议 scope）。
#[tauri::command]
pub fn language_lesson_audio(
    state: State<'_, AppState>,
    lesson_id: String,
) -> Result<tauri::ipc::Response, CommandError> {
    let store = state.language_store.lock();
    match read_lesson_audio(&store, &lesson_id) {
        Ok(bytes) => Ok(tauri::ipc::Response::new(bytes)),
        Err((code, message)) => Err(CommandError { code, message }),
    }
}

// ============================================================================
// 导入：NCE
// ============================================================================

/// 学习资料现状：教材在哪、词典有多少、还缺什么（只读，不改数据）。
#[tauri::command]
pub fn language_data_status(app: AppHandle) -> Result<DataStatus, CommandError> {
    use tauri::Manager as _;
    let data_dir = crate::project_config_directory_public(&app)?;
    let state = app.state::<AppState>();
    let store: Arc<dyn devtoolbox_application::language::CourseStorePort> = Arc::new(
        composition::CourseStoreAdapter::new(Arc::clone(&state.language_store)),
    );
    Ok(devtoolbox_application::language::course::data_status(
        store.as_ref(),
        &data_dir,
    ))
}

/// 读取英语学习计划（含教材源目录）；未设置时返回默认值。
fn language_plan_read(
    app: &AppHandle,
) -> Result<devtoolbox_core::language::LearningPlan, CommandError> {
    use tauri::Manager as _;
    let state = app.state::<AppState>();
    let store = state.language_store.lock();
    Ok(store
        .learning_plan("eng")
        .map_err(|error| language_err("language_error", error.to_string()))?
        .unwrap_or_default())
}

/// 写回英语学习计划。
fn plan_write(
    app: &AppHandle,
    plan: devtoolbox_core::language::LearningPlan,
) -> Result<(), CommandError> {
    use tauri::Manager as _;
    let state = app.state::<AppState>();
    let store = state.language_store.lock();
    store
        .save_learning_plan(&plan)
        .map_err(|error| language_err("language_error", error.to_string()))
}

/// 扫描 NCE 源文件夹（只读预览：找到几册几课、哪些缺音频/字幕）。
#[tauri::command]
pub fn language_nce_scan(
    source_dir: String,
) -> Result<devtoolbox_infrastructure::language::NceScanReport, CommandError> {
    let root = PathBuf::from(source_dir);
    if !root.is_dir() {
        return Err(language_err(
            "language_nce_bad_dir",
            format!("{} is not a directory", root.display()),
        ));
    }
    Ok(devtoolbox_infrastructure::language::scan_nce_source(&root))
}

/// 导入 NCE（长任务：逐课进度事件 + 可取消）。
#[tauri::command]
pub async fn language_nce_import(
    app: AppHandle,
    state: State<'_, AppState>,
    source_dir: String,
) -> Result<devtoolbox_infrastructure::language::NceImportReport, CommandError> {
    let source = PathBuf::from(source_dir.clone());
    if !source.is_dir() {
        return Err(language_err(
            "language_nce_bad_dir",
            format!("{} is not a directory", source.display()),
        ));
    }
    let source_display = source_dir;
    let media_dir = crate::language_media_dir(&app)?;
    let cancel = Arc::clone(nce_cancel());
    cancel.store(false, Ordering::Relaxed);
    let emitter = app.clone();
    let store = Arc::clone(&state.language_store);
    // 导入在阻塞线程跑；Tauri 命令本身 async，不阻塞 UI 线程。
    let report = tauri::async_runtime::spawn_blocking(move || {
        let options = devtoolbox_infrastructure::language::NceImport {
            source_dir: source,
            media_dir,
            cancel,
            on_progress: Box::new(move |progress| {
                let _ = emitter.emit("language-nce-progress", &progress);
            }),
        };
        let guard = store.lock();
        devtoolbox_infrastructure::language::import_nce(&guard, &options)
    })
    .await
    .map_err(|error| language_err("language_nce_panic", error.to_string()))?
    .map_err(|error| language_err("language_nce_import", error))?;
    // 记住用户选的目录：下次打开界面直接告诉他「你的教材在这里」。
    if !report.cancelled {
        remember_nce_source(&app, &source_display);
    }
    Ok(report)
}

/// 把教材源目录写进英语学习计划（仅用于界面显示，不影响学习逻辑）。
fn remember_nce_source(app: &AppHandle, source_dir: &str) {
    if let Ok(mut plan) = language_plan_read(app) {
        plan.nce_source_dir = Some(source_dir.to_string());
        plan.updated_at = devtoolbox_infrastructure::now_unix();
        let _ = plan_write(app, plan);
    }
}

/// 取消正在进行的 NCE 导入。
#[tauri::command]
pub fn language_nce_cancel() -> Result<(), CommandError> {
    nce_cancel().store(true, Ordering::Relaxed);
    Ok(())
}

// ============================================================================
// 导入：ECDICT
// ============================================================================

/// 导入 ECDICT csv（长任务：按批进度事件 + 可取消）。
#[tauri::command]
pub async fn language_dict_import(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<devtoolbox_infrastructure::language::DictImportReport, CommandError> {
    let source = PathBuf::from(path);
    if !source.is_file() {
        return Err(language_err(
            "language_dict_bad_file",
            format!("{} is not a file", source.display()),
        ));
    }
    let cancel = Arc::clone(dict_cancel());
    cancel.store(false, Ordering::Relaxed);
    let emitter = app.clone();
    let store = Arc::clone(&state.language_store);
    let report = tauri::async_runtime::spawn_blocking(move || {
        let guard = store.lock();
        devtoolbox_infrastructure::language::import_ecdict_csv(
            &guard,
            &source,
            &cancel,
            &move |progress| {
                let _ = emitter.emit("language-dict-progress", &progress);
            },
        )
    })
    .await
    .map_err(|error| language_err("language_dict_panic", error.to_string()))?
    .map_err(|error| language_err("language_dict_import", error))?;
    Ok(report)
}

/// 取消正在进行的 ECDICT 导入。
#[tauri::command]
pub fn language_dict_cancel() -> Result<(), CommandError> {
    dict_cancel().store(true, Ordering::Relaxed);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_infrastructure::language::LanguageStore;

    fn store_with_lesson(audio: Option<&str>) -> (tempfile::TempDir, LanguageStore) {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        store
            .replace_lesson_content(
                &devtoolbox_core::language::CourseLesson {
                    id: "nce:1:1".into(),
                    book_id: "nce:1".into(),
                    lesson_no: 1,
                    title: "Excuse Me".into(),
                    audio_path: audio.map(str::to_string),
                    duration_ms: Some(1000),
                    sentence_count: 0,
                    vocab_count: 0,
                },
                &[],
                &[],
            )
            .expect("lesson");
        (directory, store)
    }

    #[test]
    fn audio_reads_real_bytes_from_local_media() {
        let directory = tempfile::tempdir().expect("media");
        let media = directory.path().join("001.mp3");
        std::fs::write(&media, b"ID3fake-mp3-bytes").expect("write");
        let (_keep, store) = store_with_lesson(Some(media.to_str().expect("utf8")));
        let bytes = read_lesson_audio(&store, "nce:1:1").expect("read audio");
        assert_eq!(bytes, b"ID3fake-mp3-bytes");
    }

    #[test]
    fn audio_missing_is_explicit_error_not_panic() {
        let (_keep, store) = store_with_lesson(None);
        let (code, _) = read_lesson_audio(&store, "nce:1:1").expect_err("must fail");
        assert_eq!(code, "language_audio_missing");

        let (code, _) = read_lesson_audio(&store, "nope").expect_err("must fail");
        assert_eq!(code, "language_lesson_not_found");
    }

    #[test]
    fn audio_file_moved_reports_read_error() {
        let directory = tempfile::tempdir().expect("media");
        let media = directory.path().join("gone.mp3");
        std::fs::write(&media, b"x").expect("write");
        let path = media.to_str().expect("utf8").to_string();
        let (_keep, store) = store_with_lesson(Some(&path));
        std::fs::remove_file(&media).expect("remove");
        let (code, message) = read_lesson_audio(&store, "nce:1:1").expect_err("must fail");
        assert_eq!(code, "language_audio_read");
        assert!(message.contains("gone.mp3"));
    }
}
