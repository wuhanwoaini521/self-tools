//! 英语课程（NCE）**写路径**的 HTTP 接口。
//!
//! ## 为什么必须有
//!
//! 之前这里只有 4 个只读端点（today / books / book / lesson）。结果是：
//! 网页端能**打开**课程，却**学不动**——
//! 点「不认识」直接报 `「language_course_mark_word」尚无网页端接口`。
//!
//! 而这台机器就是服务器：学习进度、标记、测验本来就落在同一个 SQLite 里，
//! 没有任何理由要求换一条 IPC 通道才能写。两端调用的是**同一个** `CourseService`，
//! 数据同库，因此进度可以互相看见（桌面端学的，网页端能看到，反之亦然）。
//!
//! 语义与桌面端逐条对齐，不做「网页端简化版」。

use std::sync::Arc;

use axum::extract::Query;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use devtoolbox_application::language::course::{CourseService, ProgressPatch, WordLookup};
use devtoolbox_core::language::{LearningPlan, LessonProgress, QuizAnswer, QuizItem, WordMark};
use devtoolbox_core::learning::LearningProgress as PlatformProgress;
use serde::Deserialize;
use serde_json::Value;

/// 课程运行时（与 readiness 共用同一实例：同一个 language.db）。
pub type Course = Arc<CourseService>;

fn clock() -> i64 {
    devtoolbox_infrastructure::now_unix()
}

fn err(code: &'static str, message: String) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError { code, message }
}

fn from_app(error: devtoolbox_application::error::ApplicationError) -> crate::ai_api::ApiError {
    crate::ai_api::ApiError {
        code: "language_error",
        message: error.to_string(),
    }
}

// ============================================================================
// 请求体
// ============================================================================

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteBody {
    pub quiz_score: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkWordBody {
    pub word: String,
    pub mark: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupBody {
    pub word: String,
    pub sentence: Option<String>,
    pub lesson_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitQuizBody {
    pub answers: Vec<QuizAnswer>,
}

#[derive(Deserialize)]
pub struct LessonQuery {
    pub lesson_id: String,
}

// ============================================================================
// 写路径
// ============================================================================

/// `POST /api/v1/language/course/lesson/{lesson_id}/progress`
///
/// 工作台心跳：阶段 / 音频位置 / 句子序号 / 跟读位置 / 学习秒数。
pub async fn update_progress(
    Extension(course): Extension<Course>,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
    Json(patch): Json<ProgressPatch>,
) -> Result<Json<LessonProgress>, crate::ai_api::ApiError> {
    course
        .update_lesson_progress(&lesson_id, patch, clock())
        .map(Json)
        .map_err(from_app)
}

/// `POST /api/v1/language/course/lesson/{lesson_id}/complete`
pub async fn complete_lesson(
    Extension(course): Extension<Course>,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
    Json(body): Json<CompleteBody>,
) -> Result<Json<LessonProgress>, crate::ai_api::ApiError> {
    course
        .complete_lesson(&lesson_id, body.quiz_score, clock())
        .map(Json)
        .map_err(from_app)
}

/// `POST /api/v1/language/course/lesson/{lesson_id}/mark-word`
///
/// 三态标记（know / fuzzy / unknown）→ 立即进入平台 SRS。
pub async fn mark_word(
    Extension(course): Extension<Course>,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
    Json(body): Json<MarkWordBody>,
) -> Result<Json<PlatformProgress>, crate::ai_api::ApiError> {
    let parsed = WordMark::parse(&body.mark)
        .ok_or_else(|| err("language_invalid_mark", format!("未知标记：{}", body.mark)))?;
    course
        .mark_word(&lesson_id, &body.word, parsed, clock())
        .map(Json)
        .map_err(from_app)
}

/// `POST /api/v1/language/course/lookup-word`
///
/// 查词（词典 + 遇见历史 + 学习状态），并记一次 occurrence。
pub async fn lookup_word(
    Extension(course): Extension<Course>,
    Json(body): Json<LookupBody>,
) -> Result<Json<WordLookup>, crate::ai_api::ApiError> {
    course
        .lookup_word(
            &body.word,
            body.sentence.as_deref(),
            body.lesson_id.as_deref(),
            clock(),
        )
        .map(Json)
        .map_err(from_app)
}

/// `GET /api/v1/language/course/quiz?lesson_id=`
pub async fn quiz(
    Extension(course): Extension<Course>,
    Query(params): Query<LessonQuery>,
) -> Result<Json<Vec<QuizItem>>, crate::ai_api::ApiError> {
    course
        .generate_quiz(&params.lesson_id)
        .map(Json)
        .map_err(from_app)
}

/// `POST /api/v1/language/course/quiz/submit`
pub async fn submit_quiz(
    Extension(course): Extension<Course>,
    Query(params): Query<LessonQuery>,
    Json(body): Json<SubmitQuizBody>,
) -> Result<Json<devtoolbox_core::language::QuizResult>, crate::ai_api::ApiError> {
    course
        .submit_quiz(&params.lesson_id, &body.answers, clock())
        .map(Json)
        .map_err(from_app)
}

/// `POST /api/v1/language/course/plan` —— 保存学习计划（每日目标 / 当前册）。
pub async fn save_plan(
    Extension(course): Extension<Course>,
    Json(mut plan): Json<LearningPlan>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    plan.updated_at = clock();
    course.save_plan(&plan).map_err(from_app)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
/// `GET /api/v1/language/course/lesson/{lesson_id}/audio` —— 课时音频（二进制）。
///
/// 听力 / 跟读 / 逐句精听都依赖它。此前只有桌面端有，网页端整条听力链路不可用。
///
/// 安全：路径只来自数据库里的 `audio_path`（导入器写入 `config/language/nce/`），
/// **不接受前端传入的任意路径**——避免变成任意文件读取。
pub async fn lesson_audio(
    Extension(store): Extension<
        Arc<parking_lot::Mutex<devtoolbox_infrastructure::language::LanguageStore>>,
    >,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
) -> Result<axum::response::Response, crate::ai_api::ApiError> {
    let guard = store.lock();
    let lesson = guard
        .course_lesson(&lesson_id)
        .map_err(|error| err("language_error", error.to_string()))?
        .ok_or_else(|| err("language_lesson_not_found", lesson_id.clone()))?;
    let path = lesson
        .audio_path
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            err(
                "language_audio_missing",
                "本课没有音频（导入时未找到 .mp3）".to_string(),
            )
        })?;
    drop(guard);

    let bytes = std::fs::read(&path)
        .map_err(|error| err("language_audio_read", format!("{path}: {error}")))?;
    Ok(([(axum::http::header::CONTENT_TYPE, "audio/mpeg")], bytes).into_response())
}
