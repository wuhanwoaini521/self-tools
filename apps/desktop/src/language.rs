//! Desktop Language 适配器与 Tauri 命令。
//!
//! **参数契约**：全部使用 Tauri v2 的顶层驼峰参数（`entityId` / `cardId` …），
//! 与其余模块一致。此前 Language 的三个写命令把参数包进
//! `#[serde(rename_all = "snake_case")]` 的 `request` 结构体（要求 `item_id`），
//! 而前端发的是 `{ request: { itemId } }` —— Tauri 只对**顶层**参数做驼峰转换，
//! 嵌套结构体按 serde 规则反序列化，于是「标记状态 / 复习评分 / 收藏」三条命令
//! 在运行时必然反序列化失败，整个学习写入路径是死的。

use std::sync::Arc;

use tauri::State;

use devtoolbox_application::language::{
    LanguageLearningService, LanguageService, LanguageStorePort, StudyAction,
};
use devtoolbox_application::learning::LearningService as PlatformLearningService;
use devtoolbox_core::language::{
    LanguageCode, LanguageLearningItem, Lesson, Mistake, SentenceRecord, SentenceStudy,
    SpeakingScore,
};
use devtoolbox_core::learning::{
    LearningProgress, ReviewQueueItem, ReviewRating, ReviewScheduleOutcome,
};
use devtoolbox_infrastructure::language::starter::StarterReport;

use crate::{AppState, CommandError, SpeakingScoreRequest, composition};

/// 词典侧服务。
pub fn language_service(state: &State<'_, AppState>) -> LanguageService {
    LanguageService::new(Arc::new(composition::LanguageStoreAdapter::new(
        Arc::clone(&state.language_store),
    )))
}

/// 学习侧服务：语言内容 + 平台学习系统。
pub fn learning_service(state: &State<'_, AppState>) -> LanguageLearningService {
    let content: Arc<dyn LanguageStorePort> = Arc::new(composition::LanguageStoreAdapter::new(
        Arc::clone(&state.language_store),
    ));
    let platform = Arc::new(PlatformLearningService::new(Arc::clone(
        &state.learning_store,
    )));
    LanguageLearningService::new(content, platform)
}

fn now() -> i64 {
    devtoolbox_infrastructure::now_unix()
}

/// 安装内置 Starter Pack（离线；真实数据子集 + attribution）。
#[tauri::command]
pub fn language_install_starter(
    state: State<'_, AppState>,
    only: Option<String>,
) -> Result<StarterReport, CommandError> {
    let mut store = state.language_store.lock();
    devtoolbox_infrastructure::language::starter::install_starter(&mut store, only.as_deref())
        .map_err(|error| CommandError {
            code: "language_error",
            message: error.to_string(),
        })
}

/// 加入平台合集（复用全局 Collections，不造 Language 自己的收藏）。
#[tauri::command]
pub fn language_add_collection_item(
    state: State<'_, AppState>,
    collection_id: String,
    entity_id: String,
    note: Option<String>,
) -> Result<(), CommandError> {
    let content = composition::LanguageStoreAdapter::new(Arc::clone(&state.language_store));
    let item = require_item(&content, &entity_id)?;
    let platform = PlatformLearningService::new(Arc::clone(&state.learning_store));
    platform
        .add_collection_item(
            &collection_id,
            devtoolbox_core::learning::CollectionItemRef {
                module: "language".to_string(),
                entity_type: item.item_type.as_str().to_string(),
                entity_id: item.id,
                title: item.content,
                note,
            },
            now(),
        )
        .map(|_| ())
        .map_err(|error| CommandError {
            code: "collection_error",
            message: error.to_string(),
        })
}

// ============================================================================
// 词典数据
// ============================================================================

/// 学习卡片队列（进 Language 直接给卡片）。先到期复习，再补新内容。
#[tauri::command]
pub fn language_study_queue(
    state: State<'_, AppState>,
    language: String,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_application::language::StudyCard>, CommandError> {
    let code = LanguageCode::from_code(&language).ok_or_else(|| CommandError {
        code: "language_invalid",
        message: format!("未知语言代码：{language}"),
    })?;
    learning_service(&state)
        .study_queue(code, limit.unwrap_or(20), now())
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_languages(
    state: State<'_, AppState>,
) -> Result<Vec<devtoolbox_application::language::LanguageInfo>, CommandError> {
    language_service(&state)
        .languages()
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_search(
    state: State<'_, AppState>,
    language: Option<String>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_application::language::LanguageSearchHit>, CommandError> {
    language_service(&state)
        .search(language.as_deref(), &query, limit.unwrap_or(30))
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<devtoolbox_application::language::WordDetail>, CommandError> {
    language_service(&state)
        .detail(&id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_sentences(
    state: State<'_, AppState>,
    language: String,
    limit: Option<usize>,
) -> Result<Vec<SentenceRecord>, CommandError> {
    language_service(&state)
        .sentences(&language, limit.unwrap_or(20))
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_sources(
    state: State<'_, AppState>,
) -> Result<Vec<devtoolbox_application::language::SourceInfo>, CommandError> {
    language_service(&state)
        .sources()
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_speaking_feedback(
    state: State<'_, AppState>,
    request: SpeakingScoreRequest,
) -> Result<SpeakingScore, CommandError> {
    Ok(language_service(&state).speaking_feedback(
        &request.target,
        &request.transcript,
        request.duration_ms,
        request.target_ms,
        &request.long_pauses_ms,
    ))
}

// ============================================================================
// 学习条目
// ============================================================================

/// 学习对象（word / phrase / sentence / article 统一形状）。
#[tauri::command]
pub fn language_learning_item(
    state: State<'_, AppState>,
    entity_id: String,
) -> Result<Option<LanguageLearningItem>, CommandError> {
    let content = composition::LanguageStoreAdapter::new(Arc::clone(&state.language_store));
    content
        .learning_item(&entity_id)
        .map_err(|error| CommandError {
            code: "language_error",
            message: error,
        })
}

/// 记录一次学习行为（view / study / complete）→ 平台 LearningEvent + 进度。
#[tauri::command]
pub fn language_record_study(
    state: State<'_, AppState>,
    entity_id: String,
    action: String,
) -> Result<LearningProgress, CommandError> {
    let service = learning_service(&state);
    let content = composition::LanguageStoreAdapter::new(Arc::clone(&state.language_store));
    let item = content
        .learning_item(&entity_id)
        .map_err(|error| CommandError {
            code: "language_error",
            message: error,
        })?;
    let Some(item) = item else {
        return Err(not_found(&entity_id));
    };
    service
        .record_study(&item, parse_study_action(&action), now())
        .map_err(CommandError::from)
}

// ============================================================================
// 入参校验（纯函数；命令层只做转发，校验逻辑可被单测直接覆盖）
// ============================================================================

/// 学习动作：只认 `view` / `complete`，其余一律按 `study` 处理。
#[must_use]
pub fn parse_study_action(action: &str) -> StudyAction {
    match action {
        "view" => StudyAction::View,
        "complete" => StudyAction::Complete,
        _ => StudyAction::Study,
    }
}

fn not_found(entity_id: &str) -> CommandError {
    CommandError {
        code: "language_not_found",
        message: format!("未找到学习对象：{entity_id}"),
    }
}

fn review_card_not_found(card_id: &str) -> CommandError {
    CommandError {
        code: "review_card_not_found",
        message: format!("复习卡不存在：{card_id}"),
    }
}

/// 校验建课请求。空条目列表被拒绝：否则会建出一节空课程，
/// 用户点「继续」只看到「第 1 / 0 步」。
pub fn validate_lesson_request(
    request: &CreateLessonRequest,
) -> Result<LanguageCode, CommandError> {
    let language = LanguageCode::from_code(&request.language).ok_or_else(|| CommandError {
        code: "language_invalid",
        message: format!("未知语言代码：{}", request.language),
    })?;
    if request.item_ids.is_empty() {
        return Err(CommandError {
            code: "language_invalid",
            message: "Lesson 至少需要一个条目".to_string(),
        });
    }
    Ok(language)
}

/// 把条目加入平台复习队列。
#[tauri::command]
pub fn language_add_to_review(
    state: State<'_, AppState>,
    entity_id: String,
) -> Result<(), CommandError> {
    let service = learning_service(&state);
    let content = composition::LanguageStoreAdapter::new(Arc::clone(&state.language_store));
    let item = require_item(&content, &entity_id)?;
    service
        .add_to_review(&item, now())
        .map_err(CommandError::from)
}

fn require_item(
    content: &composition::LanguageStoreAdapter,
    entity_id: &str,
) -> Result<LanguageLearningItem, CommandError> {
    content
        .learning_item(entity_id)
        .map_err(|error| CommandError {
            code: "language_error",
            message: error,
        })?
        .ok_or_else(|| not_found(entity_id))
}

// ============================================================================
// 复习（平台 Review Center）
// ============================================================================

/// Language 的复习队列（走平台，卡片由平台存储与排期）。
#[tauri::command]
pub fn language_review_queue(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<ReviewQueueItem>, CommandError> {
    learning_service(&state)
        .review_queue(limit.unwrap_or(20), now())
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_submit_review(
    state: State<'_, AppState>,
    card_id: String,
    rating: ReviewRating,
    user_answer: Option<String>,
) -> Result<ReviewScheduleOutcome, CommandError> {
    let service = learning_service(&state);
    let answer = user_answer.unwrap_or_default();
    let Some(card) = service.review_card(&card_id).map_err(CommandError::from)? else {
        return Err(review_card_not_found(&card_id));
    };
    service
        .submit_review(&card, &answer, rating, now())
        .map_err(CommandError::from)
}

// ============================================================================
// 错题
// ============================================================================

#[tauri::command]
pub fn language_mistakes(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<Mistake>, CommandError> {
    learning_service(&state)
        .mistakes(limit.unwrap_or(50))
        .map_err(CommandError::from)
}

// ============================================================================
// Lesson
// ============================================================================

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLessonRequest {
    pub language: String,
    pub title: String,
    pub item_ids: Vec<String>,
}

#[tauri::command]
pub fn language_create_lesson(
    state: State<'_, AppState>,
    request: CreateLessonRequest,
) -> Result<Lesson, CommandError> {
    let language = validate_lesson_request(&request)?;
    learning_service(&state)
        .create_lesson(language, request.title, &request.item_ids, now())
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_lessons(
    state: State<'_, AppState>,
    language: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Lesson>, CommandError> {
    let code = language.as_deref().and_then(LanguageCode::from_code);
    learning_service(&state)
        .lessons(code, limit.unwrap_or(20))
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_lesson(
    state: State<'_, AppState>,
    lesson_id: String,
) -> Result<Option<LessonView>, CommandError> {
    let service = learning_service(&state);
    let Some(lesson) = service.lesson(&lesson_id).map_err(CommandError::from)? else {
        return Ok(None);
    };
    let step_index = service.resume_step(&lesson);
    let items = service.lesson_steps(&lesson).map_err(CommandError::from)?;
    Ok(Some(LessonView {
        lesson,
        step_index,
        items,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct LessonView {
    #[serde(flatten)]
    pub lesson: Lesson,
    pub step_index: usize,
    pub items: Vec<LanguageLearningItem>,
}

#[tauri::command]
pub fn language_delete_lesson(
    state: State<'_, AppState>,
    lesson_id: String,
) -> Result<(), CommandError> {
    learning_service(&state)
        .delete_lesson(&lesson_id)
        .map_err(CommandError::from)
}

/// 上报学习位置（支持「退出后继续」）。
#[tauri::command]
pub fn language_save_lesson_position(
    state: State<'_, AppState>,
    lesson_id: String,
    step_index: usize,
) -> Result<(), CommandError> {
    learning_service(&state)
        .save_position(&lesson_id, step_index, now())
        .map_err(CommandError::from)
}

/// Continue 学习入口：最近学过的 Lesson + 恢复位置。
#[tauri::command]
pub fn language_continue_lessons(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_application::language::ContinueLesson>, CommandError> {
    learning_service(&state)
        .continue_lessons(limit.unwrap_or(5))
        .map_err(CommandError::from)
}

// ============================================================================
// 句子 / 进度
// ============================================================================

#[tauri::command]
pub fn language_sentence_study(
    state: State<'_, AppState>,
    sentence_id: String,
) -> Result<Option<SentenceStudy>, CommandError> {
    learning_service(&state)
        .sentence_study(&sentence_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_progress(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<LearningProgress>, CommandError> {
    learning_service(&state)
        .progress(limit.unwrap_or(100))
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn language_weak_items(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<devtoolbox_application::language::WeakItem>, CommandError> {
    learning_service(&state)
        .weak_items(limit.unwrap_or(8))
        .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // 入参校验（validation / not-found）
    //
    // 这些路径原本埋在 `#[tauri::command]` 里，而命令签名要求 `State<AppState>`，
    // 单元测试无法构造 → 校验逻辑完全没有覆盖。抽出纯函数后直接测。
    // ------------------------------------------------------------------

    #[test]
    fn study_action_accepts_known_verbs_and_defaults_to_study() {
        assert!(matches!(parse_study_action("view"), StudyAction::View));
        assert!(matches!(
            parse_study_action("complete"),
            StudyAction::Complete
        ));
        assert!(matches!(parse_study_action("study"), StudyAction::Study));
        // 未知动词不得变成「什么都不做」——按 study 处理才是安全默认。
        assert!(matches!(
            parse_study_action("随便写点什么"),
            StudyAction::Study
        ));
        assert!(matches!(parse_study_action(""), StudyAction::Study));
    }

    #[test]
    fn lesson_request_rejects_unknown_language_code() {
        let request = CreateLessonRequest {
            language: "klingon".to_string(),
            title: "测试".to_string(),
            item_ids: vec!["jmdict:1".to_string()],
        };
        let error = validate_lesson_request(&request).expect_err("应拒绝未知语言");
        assert_eq!(error.code, "language_invalid");
        assert!(
            error.message.contains("klingon"),
            "错误信息应回显用户输入，实际 {}",
            error.message
        );
    }

    #[test]
    fn lesson_request_rejects_empty_item_list() {
        let request = CreateLessonRequest {
            language: "jpn".to_string(),
            title: "空课程".to_string(),
            item_ids: Vec::new(),
        };
        let error = validate_lesson_request(&request).expect_err("应拒绝空课程");
        assert_eq!(
            error.code, "language_invalid",
            "空课程会让「继续」落到「第 1 / 0 步」"
        );
    }

    #[test]
    fn lesson_request_accepts_a_real_course() {
        let request = CreateLessonRequest {
            language: "jpn".to_string(),
            title: "日本 · 交通基础".to_string(),
            item_ids: vec!["jmdict:1".to_string(), "jmdict:2".to_string()],
        };
        assert_eq!(
            validate_lesson_request(&request).expect("应通过"),
            LanguageCode::Jap
        );
    }

    #[test]
    fn not_found_errors_carry_a_stable_code_and_the_offending_id() {
        let error = not_found("jmdict:missing");
        assert_eq!(error.code, "language_not_found");
        assert!(
            error.message.contains("jmdict:missing"),
            "错误信息应指明是哪个 id 找不到"
        );

        let error = review_card_not_found("card_missing");
        assert_eq!(error.code, "review_card_not_found");
        assert!(error.message.contains("card_missing"));
    }

    // ------------------------------------------------------------------
    // 契约护栏：新增命令必须被注册，否则前端调用恒为 "command not found"
    // ------------------------------------------------------------------

    #[test]
    fn every_language_command_is_registered() {
        // `generate_handler!` 里的命令名。前端 `languageClient.ts` 逐个调用它们；
        // 漏注册（如曾经的 `language_add_collection_item`）只会在运行时炸。
        const REGISTERED: &[&str] = &[
            "language_languages",
            "language_search",
            "language_item",
            "language_sentences",
            "language_sources",
            "language_install_starter",
            "language_speaking_feedback",
            "language_learning_item",
            "language_record_study",
            "language_add_to_review",
            "language_add_collection_item",
            "language_review_queue",
            "language_submit_review",
            "language_mistakes",
            "language_create_lesson",
            "language_lessons",
            "language_lesson",
            "language_delete_lesson",
            "language_save_lesson_position",
            "language_continue_lessons",
            "language_sentence_study",
            "language_progress",
            "language_weak_items",
        ];

        let source = include_str!("lib.rs");
        for command in REGISTERED {
            assert!(
                source.contains(&format!("language::{command},")),
                "命令 {command} 未在 generate_handler! 中注册"
            );
        }
    }
}
