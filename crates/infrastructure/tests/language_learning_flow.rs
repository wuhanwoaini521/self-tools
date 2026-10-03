//! Language 学习闭环的端到端验收（任务书 §48）。
//!
//! 这一组用例走**真实的 SQLite**（`LanguageStore` + `LearningStore`）、**真实的
//! starter 数据集导入**（`install_starter`，即 Parser → Validator → 去重 → 落库），
//! 与应用层真实的 `LanguageLearningService` + `LearningService`，不注入任何替身。
//!
//! 覆盖的完整链路：
//!
//! ```text
//! Language Home → Continue Lesson → Study Word → Study Sentence → Do Review
//! → Answer Wrong → Mistake Recorded → Review Again → Answer Correct
//! → Progress Updated → Restart App → State Still Exists
//! ```
//!
//! 放在 `crates/infrastructure/tests/` 而不是 application 层，是因为这条链路的两端
//! 都是基础设施（语言词典库 + 平台学习库），只有真实 SQLite 才能证明「重启后状态还在」。
// 本文件是 integration test target：workspace 的 `unused_crate_dependencies`
// 会把本 crate 的 dev-deps 逐个判给本 target 审，而它们大多服务于 library。
// 与 `apps/mcp/tests/*.rs` 同一处理方式。
#![allow(unused_crate_dependencies)]

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use devtoolbox_application::language::ports::LanguageStorePort;
use devtoolbox_application::language::{LanguageLearningService, LanguageService, StudyAction};
use devtoolbox_application::learning::ports::LearningStorePort;
use devtoolbox_application::learning::{
    LearningPortError, LearningService as PlatformLearningService,
};
use devtoolbox_core::language::{LanguageCode, LanguageItemType, LearningItemType};
use devtoolbox_core::learning::LearningProgress;
use devtoolbox_core::learning::{LearningStatus, ReviewRating};
use devtoolbox_infrastructure::LearningStore;
use devtoolbox_infrastructure::language::LanguageStore;
use devtoolbox_infrastructure::language::starter::install_starter;

const NOW: i64 = 1_700_000_000;

// ============================================================================
// 真实装配
// ============================================================================

struct Harness {
    _directory: tempfile::TempDir,
    language_path: std::path::PathBuf,
    language: LanguageLearningService,
    /// `Mutex<...>`：rusqlite `Connection` 是 `!Sync`，桌面组合根同样用 Mutex 串行化。
    /// 测试刻意保持同一形状，跑的就是真实装配方式。
    content: Arc<Mutex<devtoolbox_infrastructure::language::LanguageStore>>,
    platform_store: Arc<Mutex<LearningStore>>,
    _platform: Arc<PlatformLearningService>,
}

impl Harness {
    /// 用真实 starter 数据集建库。`restart` 复用同一个数据目录（模拟重启 App）。
    fn start(directory: tempfile::TempDir) -> Self {
        let language_path = directory.path().join("language.db");
        {
            let mut store = LanguageStore::open(&language_path).expect("open language db");
            install_starter(&mut store, None).expect("install starter data");
        }
        Self::attach(directory, language_path)
    }

    /// 不导入数据（验证空态）。
    fn start_empty(directory: tempfile::TempDir) -> Self {
        let language_path = directory.path().join("language.db");
        LanguageStore::open(&language_path).expect("open language db");
        Self::attach(directory, language_path)
    }

    fn attach(directory: tempfile::TempDir, language_path: std::path::PathBuf) -> Self {
        let content = Arc::new(Mutex::new(
            LanguageStore::open(&language_path).expect("reopen language db"),
        ));
        let port: Arc<dyn LanguageStorePort> = Arc::new(StoreAdapter(content.clone()));
        let platform_store = Arc::new(Mutex::new(
            LearningStore::open(directory.path().join("learning.db")).expect("open learning db"),
        ));
        let platform = Arc::new(PlatformLearningService::new(Arc::new(
            PlatformStoreAdapter::new(
                LearningStore::open(directory.path().join("learning.db"))
                    .expect("reopen learning db"),
            ),
        )));
        Self {
            _directory: directory,
            language_path,
            language: LanguageLearningService::new(port, platform.clone()),
            content,
            platform_store,
            _platform: platform,
        }
    }

    /// 打开一份新的内容库连接（供用例里的 `LanguageService` 单独使用）。
    fn open_content(&self) -> LanguageStore {
        LanguageStore::open(&self.language_path).expect("open content store")
    }

    /// 模拟「退出 App 再重启」：丢弃全部实例，仅按路径重开。
    fn restart(self) -> Self {
        let directory = self._directory;
        Self::attach(directory, self.language_path)
    }
}

/// 把真实 `LearningStore` 适配成平台端口（桌面组合根做同样的事）。
///
/// 平台 `LearningStore` 内部字段都是 `&self` 方法且连接可用 `Mutex` 保护；
/// 这里只做错误类型转换。
struct PlatformStoreAdapter(Arc<Mutex<LearningStore>>);

impl PlatformStoreAdapter {
    fn new(store: LearningStore) -> Self {
        Self(Arc::new(Mutex::new(store)))
    }
}

impl LearningStorePort for PlatformStoreAdapter {
    fn record_event(
        &self,
        event: &devtoolbox_core::learning::LearningEvent,
    ) -> Result<LearningProgress, LearningPortError> {
        self.0
            .lock()
            .record_event(event)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, LearningPortError> {
        self.0
            .lock()
            .get_progress(entity_key)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        self.0
            .lock()
            .list_progress(module_filter, status_filter, limit)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn upsert_review_card(
        &self,
        card: &devtoolbox_core::learning::UniversalReviewCard,
    ) -> Result<(), LearningPortError> {
        self.0
            .lock()
            .upsert_review_card(card)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<devtoolbox_core::learning::UniversalReviewCard>, LearningPortError> {
        self.0
            .lock()
            .get_review_card(card_id)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::learning::ReviewQueueItem>, LearningPortError> {
        self.0
            .lock()
            .list_due_reviews(module_filter, now, limit)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn get_review_stats(
        &self,
        now: i64,
    ) -> Result<devtoolbox_core::learning::ReviewQueueStats, LearningPortError> {
        self.0
            .lock()
            .get_review_stats(now)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<devtoolbox_core::learning::ReviewScheduleOutcome, LearningPortError> {
        self.0
            .lock()
            .record_review_outcome(card_id, rating, now)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<devtoolbox_core::learning::Collection, LearningPortError> {
        self.0
            .lock()
            .create_collection(title, description, tags, now)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn list_collections(
        &self,
    ) -> Result<Vec<devtoolbox_core::learning::Collection>, LearningPortError> {
        self.0
            .lock()
            .list_collections()
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn add_collection_item(
        &self,
        collection_id: &str,
        entity: devtoolbox_core::learning::CollectionItemRef,
        now: i64,
    ) -> Result<devtoolbox_core::learning::CollectionItem, LearningPortError> {
        self.0
            .lock()
            .add_collection_item(collection_id, entity, now)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<devtoolbox_core::learning::CollectionItem>, LearningPortError> {
        self.0
            .lock()
            .list_collection_items(collection_id)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError> {
        self.0
            .lock()
            .remove_collection_item(item_id)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError> {
        self.0
            .lock()
            .delete_collection(collection_id)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn get_continue_items(
        &self,
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::learning::ContinueItem>, LearningPortError> {
        self.0
            .lock()
            .get_continue_items(limit)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, LearningPortError> {
        self.0
            .lock()
            .count_topics_studied_today(day_start_ts)
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }

    fn get_average_mastery(&self) -> Result<f64, LearningPortError> {
        self.0
            .lock()
            .get_average_mastery()
            .map_err(|error| LearningPortError::Store(error.to_string()))
    }
}

/// 把真实 `LanguageStore` 适配成应用层端口。
///
/// `rusqlite::Connection` 是 `!Sync`，因此像桌面组合根那样用 `Mutex` 串行化；
/// 每个方法都是「加锁 → 一次调用 → 立刻放锁」，不跨 await。
struct StoreAdapter(Arc<Mutex<LanguageStore>>);

impl StoreAdapter {
    fn new(store: LanguageStore) -> Self {
        Self(Arc::new(Mutex::new(store)))
    }

    fn text(error: devtoolbox_infrastructure::error::InfrastructureError) -> String {
        error.to_string()
    }
}

impl LanguageStorePort for StoreAdapter {
    fn language_counts(
        &self,
    ) -> Result<Vec<devtoolbox_application::language::ports::LanguageCount>, String> {
        Ok(self
            .0
            .lock()
            .language_counts()
            .map_err(Self::text)?
            .into_iter()
            .map(
                |count| devtoolbox_application::language::ports::LanguageCount {
                    language: count.language,
                    words: count.words,
                    phrases: count.phrases,
                    sentences: count.sentences,
                    total: count.total,
                },
            )
            .collect())
    }

    fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<devtoolbox_application::language::ports::SearchHitModel>, String> {
        Ok(self
            .0
            .lock()
            .search(language, query, limit)
            .map_err(Self::text)?
            .into_iter()
            .map(
                |hit| devtoolbox_application::language::ports::SearchHitModel {
                    item: hit.item,
                    matched: hit.matched,
                },
            )
            .collect())
    }

    fn item_detail(
        &self,
        id: &str,
    ) -> Result<devtoolbox_application::language::ports::LanguageDetailRows, String> {
        let rows = self.0.lock().item_detail(id).map_err(Self::text)?;
        Ok(
            devtoolbox_application::language::ports::LanguageDetailRows {
                item: rows.item,
                meanings: rows.meanings,
                pronunciations: rows.pronunciations,
                relations: rows.relations,
                related_items: rows.related_items,
                examples: rows
                    .examples
                    .into_iter()
                    .map(
                        |example| devtoolbox_application::language::ports::LanguageExample {
                            text: example.text,
                            translation: example.translation,
                            source: example.source,
                        },
                    )
                    .collect(),
                sentences: rows.sentences,
                extra: rows.extra,
            },
        )
    }

    fn source_by_id(
        &self,
        id: &str,
    ) -> Result<Option<devtoolbox_core::language::LanguageSource>, String> {
        self.0.lock().source_by_id(id).map_err(Self::text)
    }

    fn sources(&self) -> Result<Vec<devtoolbox_core::language::LanguageSource>, String> {
        self.0.lock().sources().map_err(Self::text)
    }

    fn manifests(&self) -> Result<Vec<devtoolbox_core::language::DatasetManifest>, String> {
        self.0.lock().manifests().map_err(Self::text)
    }

    fn count_by_source(&self, source_id: &str) -> Result<i64, String> {
        self.0.lock().count_by_source(source_id).map_err(Self::text)
    }

    fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::language::SentenceRecord>, String> {
        self.0
            .lock()
            .sentences_by_language(language, limit)
            .map_err(Self::text)
    }

    fn learning_item(
        &self,
        item_id: &str,
    ) -> Result<Option<devtoolbox_core::language::LanguageLearningItem>, String> {
        self.0.lock().learning_item(item_id).map_err(Self::text)
    }

    fn learning_items(
        &self,
        item_ids: &[String],
    ) -> Result<Vec<devtoolbox_core::language::LanguageLearningItem>, String> {
        self.0.lock().learning_items(item_ids).map_err(Self::text)
    }

    fn sentence_study(
        &self,
        sentence_id: &str,
    ) -> Result<Option<devtoolbox_core::language::SentenceStudy>, String> {
        self.0
            .lock()
            .sentence_study(sentence_id)
            .map_err(Self::text)
    }

    fn next_new_items(
        &self,
        language: devtoolbox_core::language::LanguageCode,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::language::LanguageLearningItem>, String> {
        self.0
            .lock()
            .next_new_items(language, exclude, limit)
            .map_err(Self::text)
    }

    fn upsert_lesson(&self, lesson: &devtoolbox_core::language::Lesson) -> Result<(), String> {
        self.0.lock().upsert_lesson(lesson).map_err(Self::text)
    }

    fn lesson(&self, lesson_id: &str) -> Result<Option<devtoolbox_core::language::Lesson>, String> {
        self.0.lock().lesson(lesson_id).map_err(Self::text)
    }

    fn lessons(
        &self,
        language: Option<LanguageCode>,
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::language::Lesson>, String> {
        self.0.lock().lessons(language, limit).map_err(Self::text)
    }

    fn delete_lesson(&self, lesson_id: &str) -> Result<(), String> {
        self.0.lock().delete_lesson(lesson_id).map_err(Self::text)
    }

    fn save_lesson_position(
        &self,
        position: &devtoolbox_core::language::LessonPosition,
    ) -> Result<(), String> {
        self.0
            .lock()
            .save_lesson_position(position)
            .map_err(Self::text)
    }

    fn lesson_position(
        &self,
        lesson_id: &str,
    ) -> Result<Option<devtoolbox_core::language::LessonPosition>, String> {
        self.0.lock().lesson_position(lesson_id).map_err(Self::text)
    }

    fn recent_lesson_positions(
        &self,
        limit: usize,
    ) -> Result<Vec<devtoolbox_core::language::LessonPosition>, String> {
        self.0
            .lock()
            .recent_lesson_positions(limit)
            .map_err(Self::text)
    }

    fn record_mistake(
        &self,
        mistake: &devtoolbox_core::language::Mistake,
        card_id: &str,
    ) -> Result<(), String> {
        self.0
            .lock()
            .record_mistake(mistake, card_id)
            .map_err(Self::text)
    }

    fn mistakes(&self, limit: usize) -> Result<Vec<devtoolbox_core::language::Mistake>, String> {
        self.0.lock().mistakes(limit).map_err(Self::text)
    }

    fn resolve_mistake(&self, item_id: &str, card_id: &str) -> Result<(), String> {
        self.0
            .lock()
            .resolve_mistake(item_id, card_id)
            .map_err(Self::text)
    }

    fn mistake_count(&self) -> Result<i64, String> {
        self.0.lock().mistake_count().map_err(Self::text)
    }
}

// ============================================================================
// §48 完整验收链路
// ============================================================================

#[test]
fn full_learning_loop_survives_an_app_restart() {
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));

    // ---- 0. 真实 starter 数据必须可用 ----
    let dictionary = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())));
    let languages = dictionary.languages().expect("languages");
    let japanese = languages
        .iter()
        .find(|info| info.code == "jpn")
        .expect("日语应已安装");
    assert!(japanese.total > 0, "starter 数据集应导入日语词条与句子");
    assert!(japanese.sentences > 0, "应有可学习的真实例句");

    // ---- 1. Language Home：取真实的词与句 id ----
    let hits = dictionary.search(Some("jpn"), "日本", 5).expect("search");
    assert!(!hits.is_empty(), "真实词典应能搜到内容");

    let word_id = hits
        .iter()
        .map(|hit| hit.item.id.clone())
        .find(|id| {
            harness
                .content
                .lock()
                .learning_item(id)
                .ok()
                .flatten()
                .is_some_and(|item| item.item_type == LearningItemType::Word)
        })
        .expect("应有一个真实日语词条");

    let sentence_id = dictionary
        .sentences("jpn", 20)
        .expect("sentences")
        .into_iter()
        .find_map(|sentence| {
            let id = sentence.sentence_id.clone();
            harness
                .content
                .lock()
                .learning_item(&id)
                .ok()
                .flatten()
                .filter(|item| item.item_type == LearningItemType::Sentence)
                .map(|item| item.id)
        })
        .expect("应有一个真实日语例句");

    // ---- 2. Continue Lesson：建课并学习一部分 ----
    let lesson = harness
        .language
        .create_lesson(
            LanguageCode::Jap,
            "日本 · 入门",
            &[word_id.clone(), sentence_id.clone()],
            NOW,
        )
        .expect("create lesson");
    assert_eq!(lesson.steps.len(), 2, "两个真实条目应成为两步");

    let word = harness
        .content
        .lock()
        .learning_item(&word_id)
        .expect("learning item")
        .expect("word exists");
    let sentence_item = harness
        .content
        .lock()
        .learning_item(&sentence_id)
        .expect("learning item")
        .expect("sentence exists");

    // ---- 3. Study Word → 平台 LearningEvent + 进度 ----
    let progress_after_word = harness
        .language
        .record_study(&word, StudyAction::Study, NOW)
        .expect("study word");
    assert_eq!(progress_after_word.module, "language");
    assert_eq!(progress_after_word.entity_type, "word");
    assert_eq!(progress_after_word.study_count, 1);
    assert!(progress_after_word.mastery_score > 0.0);

    // ---- 4. Study Sentence ----
    harness
        .language
        .record_study(&sentence_item, StudyAction::Study, NOW + 1)
        .expect("study sentence");

    // 句子学习视图：拆解必须来自真实词典
    let study = harness
        .language
        .sentence_study(&sentence_id)
        .expect("sentence study")
        .expect("sentence exists");
    assert!(!study.original.is_empty(), "原文应来自真实句子");
    for chunk in &study.chunks {
        if chunk.item_id.is_none() {
            assert!(
                chunk.meaning.is_none(),
                "未收录片段不得带释义（编造）：{:?}",
                chunk.text
            );
        }
    }

    // 学到第 2 步（sentence）后中途退出
    harness
        .language
        .save_position(&lesson.id, 1, NOW + 2)
        .expect("save position");

    // ---- 5. Do Review：把词加入平台复习队列 ----
    harness
        .language
        .add_to_review(&word, NOW + 3)
        .expect("add to review");
    let queue = harness
        .language
        .review_queue(10, NOW + 3)
        .expect("review queue");
    let card = queue
        .into_iter()
        .find(|item| item.card.entity_id == word_id)
        .expect("复习队列应含该词条")
        .card;

    // ---- 6. Answer Wrong → Mistake Recorded ----
    let wrong = harness
        .language
        .submit_review(&card, "完全错误的答案", ReviewRating::Again, NOW + 4)
        .expect("submit wrong");
    assert!(!wrong.is_correct, "Again 应判为答错");
    assert_eq!(wrong.lapses, 1);

    let mistakes = harness.language.mistakes(10).expect("mistakes");
    assert_eq!(mistakes.len(), 1, "答错应记入错题");
    let mistake = &mistakes[0];
    assert_eq!(mistake.user_answer, "完全错误的答案");
    assert_eq!(mistake.item_id, word_id);
    assert_eq!(mistake.error_count, 1);

    // 错题里的正确答案是**真实释义**（不是编造）
    assert_eq!(mistake.correct_answer, card.answer);
    assert!(!mistake.correct_answer.trim().is_empty());

    // ---- 7. Progress Updated：答错计入 incorrect ----
    let progress_after_wrong = harness
        .platform_store
        .lock()
        .get_progress(&format!("language:word:{word_id}"))
        .expect("get progress")
        .expect("progress exists");
    assert_eq!(progress_after_wrong.incorrect_count, 1);
    assert_eq!(progress_after_wrong.correct_count, 0);
    assert_eq!(progress_after_wrong.status, LearningStatus::Learning);
    assert_eq!(
        progress_after_wrong.next_review_at,
        Some(wrong.due_at),
        "答错后应立即重回队列（interval=0）"
    );

    // ---- 8. Review Again → Answer Correct → 错题清除 ----
    let good = harness
        .language
        .submit_review(&card, &card.answer, ReviewRating::Good, NOW + 5)
        .expect("submit correct");
    assert!(good.is_correct);
    assert!(good.interval_days > 0.0, "答对应推进间隔");

    assert_eq!(
        harness.language.mistake_count().expect("count"),
        0,
        "答对后错题应被清除"
    );

    // ---- 9. Restart App：所有状态必须仍在 ----
    let harness = harness.restart();

    // lesson position
    let reloaded = harness
        .language
        .lesson(&lesson.id)
        .expect("read lesson")
        .expect("lesson survives restart");
    assert_eq!(reloaded.steps.len(), 2, "Lesson 内容应跨重启保留");
    assert_eq!(
        harness.language.resume_step(&reloaded),
        1,
        "Continue 应恢复到中断处"
    );

    // progress
    let progress = harness
        .platform_store
        .lock()
        .get_progress(&format!("language:word:{word_id}"))
        .expect("get progress")
        .expect("progress survives restart");
    assert_eq!(progress.study_count, 1);
    assert_eq!(progress.correct_count, 1, "答对应跨重启保留");
    assert_eq!(progress.incorrect_count, 1);
    assert_eq!(progress.interval_days, good.interval_days);
    assert_eq!(progress.next_review_at, Some(good.due_at));

    // review card 的排期
    let card_after = harness
        .platform_store
        .lock()
        .get_review_card(&card.id)
        .expect("get card")
        .expect("card survives restart");
    assert_eq!(card_after.due_at, good.due_at);
    assert_eq!(card_after.repetition_count, good.repetition_count);
    assert_eq!(card_after.lapses, 1, "失误次数应跨重启保留");

    // mistakes（此时应为 0，因为已答对清除；这里验证「空」也是持久化后的真实状态）
    assert_eq!(harness.language.mistake_count().expect("count"), 0);

    // Continue 入口仍能找到这节课
    let continuing = harness.language.continue_lessons(5).expect("continue");
    assert_eq!(continuing.len(), 1);
    assert_eq!(continuing[0].lesson.id, lesson.id);
    assert_eq!(continuing[0].step_index, 1);
}

#[test]
fn mastered_is_reachable_through_repeated_correct_reviews() {
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    let hits = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())))
        .search(Some("jpn"), "日本", 5)
        .expect("search");
    let word_id = hits
        .iter()
        .map(|hit| hit.item.id.clone())
        .find(|id| {
            harness
                .content
                .lock()
                .learning_item(id)
                .ok()
                .flatten()
                .is_some_and(|item| item.item_type == LearningItemType::Word)
        })
        .expect("real word");
    let word = harness
        .content
        .lock()
        .learning_item(&word_id)
        .expect("item")
        .expect("exists");

    harness
        .language
        .add_to_review(&word, NOW)
        .expect("add to review");
    let card = harness
        .language
        .review_queue(10, NOW)
        .expect("queue")
        .into_iter()
        .find(|item| item.card.entity_id == word_id)
        .expect("card")
        .card;

    // 连续 Good：间隔递增 → 越过 14 天 → Mastered
    let mut clock = NOW;
    let mut last_interval = 0.0;
    for _ in 0..5 {
        let outcome = harness
            .language
            .submit_review(&card, &card.answer, ReviewRating::Good, clock)
            .expect("review");
        assert!(
            outcome.interval_days > last_interval,
            "间隔应逐轮增长：{} -> {}",
            last_interval,
            outcome.interval_days
        );
        last_interval = outcome.interval_days;
        clock = outcome.due_at;
    }

    let progress = harness
        .platform_store
        .lock()
        .get_progress(&word.entity_key())
        .expect("progress")
        .expect("exists");
    assert!(
        progress.interval_days >= 14.0,
        "多轮答对后间隔应越过 14 天阈值，实际 {}",
        progress.interval_days
    );
    assert_eq!(
        progress.status,
        LearningStatus::Mastered,
        "掌握状态必须可达（此前因 interval_days 从不写回而永不可达）"
    );

    // 已掌握的条目不应再出现在薄弱项里
    let weak = harness.language.weak_items(10).expect("weak");
    assert!(
        !weak.iter().any(|entry| entry.entity_id == word_id),
        "已掌握条目不应出现在薄弱项"
    );
}

#[test]
fn repeated_mistakes_do_not_duplicate_rows_across_restart() {
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    let hits = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())))
        .search(Some("jpn"), "日本", 5)
        .expect("search");
    let word_id = hits
        .iter()
        .map(|hit| hit.item.id.clone())
        .find(|id| {
            harness
                .content
                .lock()
                .learning_item(id)
                .ok()
                .flatten()
                .is_some_and(|item| item.item_type == LearningItemType::Word)
        })
        .expect("real word");
    let word = harness
        .content
        .lock()
        .learning_item(&word_id)
        .expect("item")
        .expect("exists");

    harness
        .language
        .add_to_review(&word, NOW)
        .expect("add to review");
    let card = harness
        .language
        .review_queue(10, NOW)
        .expect("queue")
        .into_iter()
        .find(|item| item.card.entity_id == word_id)
        .expect("card")
        .card;

    // 同一张卡答错三次（跨两次「重启」）
    for round in 0..3 {
        harness
            .language
            .submit_review(
                &card,
                &format!("错答{round}"),
                ReviewRating::Again,
                NOW + round,
            )
            .expect("submit");
    }
    let harness = harness.restart();
    for round in 3..5 {
        harness
            .language
            .submit_review(
                &card,
                &format!("错答{round}"),
                ReviewRating::Again,
                NOW + round,
            )
            .expect("submit");
    }

    let mistakes = harness.language.mistakes(50).expect("mistakes");
    assert_eq!(
        mistakes.len(),
        1,
        "同一张卡的重复错误只应累加，不应堆积重复行"
    );
    assert_eq!(mistakes[0].error_count, 5);
    assert_eq!(mistakes[0].user_answer, "错答4", "应保留最近一次错误答案");

    // 重启后仍只有一条
    let harness = harness.restart();
    assert_eq!(harness.language.mistakes(50).expect("mistakes").len(), 1);
    assert_eq!(harness.language.mistake_count().expect("count"), 1);
}

#[test]
fn empty_database_yields_honest_empty_states_not_errors() {
    let harness = Harness::start_empty(tempfile::tempdir().expect("tempdir"));

    assert!(
        harness
            .language
            .continue_lessons(5)
            .expect("continue")
            .is_empty(),
        "无学习记录时 Continue 应为空"
    );
    assert!(
        harness.language.mistakes(10).expect("mistakes").is_empty(),
        "无错题时应为空而非错误"
    );
    assert!(
        harness.language.weak_items(10).expect("weak").is_empty(),
        "无进度时不应编造薄弱项"
    );
    assert!(
        harness
            .language
            .review_queue(10, NOW)
            .expect("queue")
            .is_empty(),
        "无卡片时复习队列应为空"
    );
    assert!(
        harness
            .language
            .sentence_study("不存在")
            .expect("study")
            .is_none(),
        "不存在的句子应返回 None"
    );
    assert!(
        harness.language.lesson("不存在").expect("lesson").is_none(),
        "不存在的 Lesson 应返回 None"
    );
}

// ============================================================================
// 平台能力回归：Language 学习不得破坏平台自身的 Today / 搜索
// ============================================================================

#[test]
fn language_progress_shows_up_in_the_platform_module_filter() {
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    let hits = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())))
        .search(Some("jpn"), "日本", 5)
        .expect("search");
    let word_id = hits
        .iter()
        .map(|hit| hit.item.id.clone())
        .find(|id| {
            harness
                .content
                .lock()
                .learning_item(id)
                .ok()
                .flatten()
                .is_some_and(|item| item.item_type == LearningItemType::Word)
        })
        .expect("real word");
    let word = harness
        .content
        .lock()
        .learning_item(&word_id)
        .expect("item")
        .expect("exists");

    harness
        .language
        .record_study(&word, StudyAction::Study, NOW)
        .expect("study");

    // 平台按 module 过滤能看到它
    let all: HashMap<_, _> = harness
        .platform_store
        .lock()
        .list_progress(Some("language"), None, 100)
        .expect("list")
        .into_iter()
        .map(|entry| (entry.entity_id.clone(), entry))
        .collect();
    assert!(all.contains_key(&word_id), "平台进度应包含语言条目");

    // 不属于 language 的模块过滤应看不到
    let others = harness
        .platform_store
        .lock()
        .list_progress(Some("history"), None, 100)
        .expect("list");
    assert!(others.is_empty(), "语言进度不应污染其它模块的过滤结果");
}

#[test]
fn language_items_keep_their_dictionary_item_type() {
    // 确认学习层类型映射没有把词误判成短语（回归保护）。
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    let dictionary = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())));
    let hits = dictionary.search(Some("jpn"), "日本", 5).expect("search");
    for hit in &hits {
        let adapted = harness
            .content
            .lock()
            .learning_item(&hit.item.id)
            .expect("item")
            .expect("adapted");
        let expected = match hit.item.item_type {
            LanguageItemType::Word => LearningItemType::Word,
            LanguageItemType::Phrase | LanguageItemType::Grammar => LearningItemType::Phrase,
            LanguageItemType::Sentence | LanguageItemType::Dialogue => LearningItemType::Sentence,
            LanguageItemType::Passage => LearningItemType::Article,
            // 读音条目不是独立学习单元，不应被适配出来
            LanguageItemType::Pronunciation => continue,
        };
        assert_eq!(adapted.item_type, expected, "id={}", hit.item.id);
        assert_eq!(adapted.language, hit.item.language);
    }
}

// ============================================================================
// Article 阅读模式（§10 / §11）
// ============================================================================

/// Article 路径必须能走通**真实导入 → 适配 → 拆句**的完整链路。
///
/// 背景：目前没有任何 importer 产出 `PASSAGE` 条目（starter 语料只有词与句），
/// 所以这条路径此前从未被任何测试跑过。这里用**真实 import_items 管线**写入一篇
/// 短文来验证代码本身正确 —— 断言的是真实的类型映射与词典命中，不是编造的展示数据。
/// 等真正接入篇章数据集时，同一条链路无需改动即可生效。
#[test]
fn article_imports_adapts_and_splits_into_navigable_sentences() {
    use devtoolbox_infrastructure::language::import::ImportedItem;

    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    {
        let mut store = harness.open_content();
        // 先有一个真实词条，供「添加学习内容」命中。
        let mut word = ImportedItem::new(
            "jmdict:1002990".to_string(),
            devtoolbox_core::language::LanguageCode::Jap,
            devtoolbox_core::language::LanguageItemType::Word,
            "駅".to_string(),
        );
        word.reading = Some("エキ".to_string());
        store
            .import_items(&[word], "jmdict", NOW)
            .expect("import word");

        let article = ImportedItem::new(
            "passage:travel-1".to_string(),
            devtoolbox_core::language::LanguageCode::Jap,
            devtoolbox_core::language::LanguageItemType::Passage,
            "駅に行きます。切符を買います。".to_string(),
        );
        store
            .import_items(&[article], "passages", NOW)
            .expect("import article");
    }

    // 1. 适配：PASSAGE → article
    let adapted = harness
        .content
        .lock()
        .learning_item("passage:travel-1")
        .expect("learning item")
        .expect("article 应可适配为学习条目");
    assert_eq!(
        adapted.item_type,
        devtoolbox_core::language::LearningItemType::Article,
        "篇章条目必须映射为 article，否则文章学习视图不会被触发"
    );
    assert_eq!(adapted.content, "駅に行きます。切符を買います。");

    // 2. 可加入复习：文章本身就是学习对象
    harness
        .language
        .add_to_review(&adapted, NOW)
        .expect("add article to review");
    let queue = harness.language.review_queue(10, NOW).expect("queue");
    let card = queue
        .into_iter()
        .find(|item| item.card.entity_id == "passage:travel-1")
        .expect("文章应进入复习队列")
        .card;
    assert_eq!(card.entity_type, "article");

    // 3. 进度按 article 类型记账，不与词条混在一起
    let progress = harness
        .platform_store
        .lock()
        .get_progress("language:article:passage:travel-1")
        .expect("progress")
        .expect("article progress");
    assert_eq!(progress.study_count, 1);
    assert_eq!(
        progress.entity_key, "language:article:passage:travel-1",
        "文章与词必须是不同的进度条目"
    );
}

#[test]
fn language_items_join_platform_collections_as_references() {
    // 回归保护：Language 曾自建 `favorites` 表。现在「加入合集」必须落到平台
    // `collection_items`，且**只存引用**（module/type/id/title），不复制词条正文。
    let harness = Harness::start(tempfile::tempdir().expect("tempdir"));
    let hits = LanguageService::new(Arc::new(StoreAdapter::new(harness.open_content())))
        .search(Some("jpn"), "日本", 5)
        .expect("search");
    let word_id = hits
        .iter()
        .map(|hit| hit.item.id.clone())
        .find(|id| {
            harness
                .content
                .lock()
                .learning_item(id)
                .ok()
                .flatten()
                .is_some_and(|item| item.item_type == LearningItemType::Word)
        })
        .expect("real word");
    let word = harness
        .content
        .lock()
        .learning_item(&word_id)
        .expect("item")
        .expect("exists");

    // 与 Tauri 命令同构：先确保条目存在，再写平台合集。
    let platform = PlatformLearningService::new(Arc::new(PlatformStoreAdapter::new(
        LearningStore::open(harness._directory.path().join("learning.db")).expect("open"),
    )));
    let collection = platform
        .create_collection("出行必备", Some("日本旅行"), &["旅行".to_string()], NOW)
        .expect("create collection");
    platform
        .add_collection_item(
            &collection.id,
            devtoolbox_core::learning::CollectionItemRef {
                module: "language".to_string(),
                entity_type: word.item_type.as_str().to_string(),
                entity_id: word.id.clone(),
                title: word.content.clone(),
                note: Some("出发前再看一遍".to_string()),
            },
            NOW,
        )
        .expect("add to collection");

    let items = platform
        .list_collection_items(&collection.id)
        .expect("list items");
    assert_eq!(items.len(), 1);
    let stored = &items[0];
    assert_eq!(stored.module, "language");
    assert_eq!(stored.entity_type, "word");
    assert_eq!(stored.entity_id, word_id, "合集只存引用，不复制词条");
    assert_eq!(stored.title, word.content, "标题是展示快照");
    assert_eq!(stored.note.as_deref(), Some("出发前再看一遍"));

    // 平台合集列表能看到条目数
    let collections = platform.list_collections().expect("list");
    assert_eq!(collections[0].item_count, 1);
}
