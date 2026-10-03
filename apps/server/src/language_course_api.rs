//! 英语课程（NCE）子域的 HTTP 接口（网页端）。
//!
//! 与桌面端 `apps/desktop/src/language_course.rs` 调用**同一个** `CourseService`，
//! 因此两端数据与字段完全一致。只提供**读接口**：导入与写操作属于桌面端
//! （需要原生文件夹选择器与文件写入），网页端明确不提供，而不是返回假数据。

use std::sync::Arc;

use devtoolbox_application::language::course::CourseService;
use serde::{Deserialize, Serialize};

use crate::language_api::{LanguageErrorBody, now_from};

/// `GET /api/v1/language/course/today` —— English 首页驾驶舱。
pub async fn today(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<devtoolbox_application::language::course::TodayDashboard>, LanguageErrorBody>
{
    course
        .today(now_from(&clock))
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/books` —— 课程书册列表。
pub async fn books(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
) -> Result<axum::Json<Vec<devtoolbox_core::language::CourseBook>>, LanguageErrorBody> {
    course
        .library()
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/book/{id}` —— 一册书的课时列表。
pub async fn book(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::extract::Path(book_id): axum::extract::Path<String>,
) -> Result<axum::Json<Option<devtoolbox_application::language::course::BookView>>, LanguageErrorBody>
{
    course
        .book_view(&book_id)
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/lesson/{id}` —— 一课的完整学习数据。
pub async fn lesson(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::extract::Path(lesson_id): axum::extract::Path<String>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<
    axum::Json<Option<devtoolbox_application::language::course::LessonDetail>>,
    LanguageErrorBody,
> {
    course
        .lesson_detail(&lesson_id, now_from(&clock))
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/progress` —— 学习统计。
pub async fn progress(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<devtoolbox_application::language::course::EnglishProgress>, LanguageErrorBody>
{
    course
        .english_progress(now_from(&clock))
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/plan` —— 学习计划。
pub async fn plan(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
) -> Result<axum::Json<Option<devtoolbox_core::language::LearningPlan>>, LanguageErrorBody> {
    course
        .get_plan()
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/dict/{word}` —— 词典查词（不记 occurrence）。
#[derive(Serialize)]
pub struct DictLookupBody {
    pub word: String,
    pub entry: Option<devtoolbox_core::language::WordEntry>,
}

pub async fn dict_lookup(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::extract::Path(word): axum::extract::Path<String>,
    axum::Extension(clock): axum::Extension<Arc<dyn Fn() -> i64 + Send + Sync>>,
) -> Result<axum::Json<DictLookupBody>, LanguageErrorBody> {
    course
        .lookup_word(&word, None, None, now_from(&clock))
        .map(|lookup| {
            axum::Json(DictLookupBody {
                word: word.to_ascii_lowercase(),
                entry: lookup.entry,
            })
        })
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}

/// `GET /api/v1/language/course/search?q=&limit=` —— 词典 + 课时搜索。
#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<usize>,
}

pub async fn search(
    axum::Extension(course): axum::Extension<Arc<CourseService>>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<
    axum::Json<devtoolbox_application::language::course::EnglishSearchResult>,
    LanguageErrorBody,
> {
    course
        .search(&params.q, params.limit.unwrap_or(10))
        .map(axum::Json)
        .map_err(|error| LanguageErrorBody {
            code: "language_error",
            message: error.to_string(),
        })
}
