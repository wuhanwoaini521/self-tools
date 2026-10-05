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
use devtoolbox_application::language::course::{
    CourseService, ProgressPatch, ShadowScoreInput, SpeakingError, SpeakingService, WordLookup,
};
use devtoolbox_core::language::{LearningPlan, LessonProgress, QuizAnswer, QuizItem, WordMark};
use devtoolbox_core::learning::LearningProgress as PlatformProgress;
use serde::Deserialize;
use serde_json::Value;

/// 课程运行时（与 readiness 共用同一实例：同一个 language.db）。
pub type Course = Arc<CourseService>;

/// 跟读评分服务（V13 W2；与课程读写同库）。
pub type Speaking = Arc<SpeakingService>;

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
// 跟读发音评分（V13 W2）
// ============================================================================

/// `POST /api/v1/language/shadow/score`
///
/// 请求只带「哪一课哪一句 + 识别到的转写」；**目标句由服务端查库得到**，
/// 不接受客户端自报的目标句（否则等于自己给自己判分）。
/// 转写为空 → 400，不产生任何记录（没有识别结果就没有分数）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowScoreBody {
    #[serde(alias = "lesson_id")]
    pub lesson_id: String,
    pub sentence_seq: u32,
    /// 语音识别转写（必须来自真实识别结果）。
    pub transcript: String,
    /// 本次开口时长（毫秒）。
    pub duration_ms: u64,
    /// 目标句参考时长（毫秒；用于流利度对比）。
    #[serde(default)]
    pub target_ms: u64,
    /// 超过阈值的长停顿（毫秒数组；用于流利度惩罚）。
    #[serde(default)]
    pub long_pauses_ms: Vec<u64>,
}

/// `GET /api/v1/language/shadow/stats?lessonId=…&since=…`
///
/// **必须 camelCase**：transport 统一发 `lessonId`；不加 rename_all 时
/// 查询参数会被静默忽略 → 端点返回「全部课时」的统计，看起来正常但口径错了
/// （这正是本条注释存在的原因）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowStatsQuery {
    #[serde(default)]
    pub lesson_id: Option<String>,
    /// 只统计该时间戳之后的尝试（0 = 不限）。
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub include_attempts: Option<bool>,
}

/// `POST /api/v1/language/shadow/score`
pub async fn shadow_score(
    Extension(speaking): Extension<Speaking>,
    Json(body): Json<ShadowScoreBody>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    if body.transcript.trim().is_empty() {
        return Err(err(
            "language_shadow_empty_transcript",
            "没有识别到语音内容，无法评分（请确认麦克风权限与网络）".to_string(),
        ));
    }
    let result = speaking
        .score_attempt(
            &body.lesson_id,
            ShadowScoreInput {
                sentence_seq: body.sentence_seq,
                transcript: body.transcript,
                duration_ms: body.duration_ms,
                target_ms: body.target_ms,
                long_pauses_ms: body.long_pauses_ms,
            },
            clock(),
        )
        .map_err(|error| match error {
            SpeakingError::EmptyTranscript => {
                err("language_shadow_empty_transcript", error.to_string())
            }
            // 指错了句子 = 客户端错误（400），不是服务故障。
            SpeakingError::SentenceNotFound { .. } => {
                err("language_shadow_no_sentence", error.to_string())
            }
            SpeakingError::Storage(message) => err("language_shadow_failed", message),
        })?;
    Ok(Json(serde_json::json!({
        "overall": result.overall,
        "accuracy": result.attempt.accuracy,
        "completeness": result.attempt.completeness,
        "fluency": result.attempt.fluency,
        "duration_ms": result.attempt.duration_ms,
        "target": result.attempt.target,
        "transcript": result.attempt.transcript,
        "missing": result.missing,
        "wrong": result.wrong,
        "extra": result.extra,
    })))
}

/// `GET /api/v1/language/shadow/stats`
pub async fn shadow_stats(
    Extension(speaking): Extension<Speaking>,
    Query(query): Query<ShadowStatsQuery>,
) -> Result<Json<Value>, crate::ai_api::ApiError> {
    let stats = speaking
        .stats(query.lesson_id.as_deref(), query.since.unwrap_or_default())
        .map_err(from_app)?;
    let mut body = serde_json::json!({
        "attempts": stats.attempts,
        "spoken_seconds": stats.spoken_seconds,
        "avg_accuracy": stats.avg_accuracy,
        "avg_completeness": stats.avg_completeness,
        "avg_fluency": stats.avg_fluency,
        "strong_attempts": stats.strong_attempts,
    });
    if query.include_attempts == Some(true) {
        let attempts = speaking
            .recent_attempts(query.lesson_id.as_deref(), 20)
            .map_err(from_app)?;
        body["attempts_recent"] = serde_json::to_value(attempts).unwrap_or(Value::Null);
    }
    Ok(Json(body))
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
