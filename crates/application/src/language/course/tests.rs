//! CourseService 编排测试：内存 CourseStorePort + 内存平台 LearningStorePort。
//!
//! 关注点：编排正确性（事件/卡片/进度/occurrence 是否按约定写入），
//! 真实 SQLite 行为在 infrastructure 层覆盖。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::language::{
    BookSummary, Course, CourseBook, CourseLesson, LearningPlan, LessonListEntry, LessonProgress,
    LessonSentence, LessonStage, LessonStatus, LessonVocab, QuizAnswer, QuizItem, WordEntry,
    WordMark, WordOccurrence,
};
use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, LearningEvent, LearningProgress,
    LearningStatus, MasteryCalculator, ReviewQueueItem, ReviewQueueStats, ReviewRating,
    ReviewScheduleOutcome, SpacedRepetitionScheduler, UniversalReviewCard,
};

use crate::language::course::{CourseService, CourseStorePort, DictionaryService};
use crate::learning::ports::{LearningPortError, LearningStorePort};
use crate::learning::service::LearningService as PlatformLearningService;

const NOW: i64 = 1_700_000_000;

// ============================================================================
// 内存平台
// ============================================================================

#[derive(Default)]
struct FakePlatform {
    progress: Mutex<HashMap<String, LearningProgress>>,
    cards: Mutex<HashMap<String, UniversalReviewCard>>,
}

impl LearningStorePort for FakePlatform {
    fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError> {
        let key = format!("{}:{}:{}", event.module, event.entity_type, event.entity_id);
        let mut guard = self.progress.lock();
        let mut progress = guard.get(&key).cloned().unwrap_or_else(|| {
            LearningProgress::new(
                &event.module,
                &event.entity_type,
                &event.entity_id,
                event.entity_title.clone().unwrap_or_default(),
                event.timestamp,
            )
        });
        match event.action {
            devtoolbox_core::learning::LearningAction::Correct => {
                progress.review_count += 1;
                progress.correct_count += 1;
            }
            devtoolbox_core::learning::LearningAction::Incorrect => {
                progress.review_count += 1;
                progress.incorrect_count += 1;
            }
            _ => progress.study_count += 1,
        }
        progress.last_studied_at = event.timestamp;
        let (score, status) = MasteryCalculator::calculate(
            progress.study_count,
            progress.correct_count,
            progress.incorrect_count,
            progress.interval_days,
            progress.last_studied_at,
            event.timestamp,
        );
        progress.mastery_score = score;
        progress.status = status;
        guard.insert(key, progress.clone());
        Ok(progress)
    }

    fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, LearningPortError> {
        Ok(self.progress.lock().get(entity_key).cloned())
    }

    fn list_progress(
        &self,
        module_filter: Option<&str>,
        _status: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        let mut list: Vec<_> = self
            .progress
            .lock()
            .values()
            .filter(|entry| module_filter.is_none_or(|module| entry.module == module))
            .cloned()
            .collect();
        list.sort_by_key(|entry| std::cmp::Reverse(entry.last_studied_at));
        list.truncate(limit);
        Ok(list)
    }

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError> {
        self.cards.lock().insert(card.id.clone(), card.clone());
        Ok(())
    }

    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, LearningPortError> {
        Ok(self.cards.lock().get(card_id).cloned())
    }

    fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        _limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError> {
        Ok(self
            .cards
            .lock()
            .values()
            .filter(|card| card.due_at <= now)
            .filter(|card| module_filter.is_none_or(|module| card.module == module))
            .cloned()
            .map(|card| ReviewQueueItem {
                is_overdue: false,
                urgency_score: 0.0,
                card,
            })
            .collect())
    }

    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        let cards = self.cards.lock();
        let due = cards
            .values()
            .filter(|card| card.due_at <= now && card.module == "language")
            .count() as u32;
        let mut by_module = HashMap::new();
        by_module.insert("language".to_string(), due);
        Ok(ReviewQueueStats {
            total_due: due,
            due_count: due,
            overdue_count: 0,
            upcoming_count: 0,
            by_module,
            mastered_count: 0,
            learning_count: 0,
            total_cards: cards.len() as u32,
        })
    }

    fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError> {
        let card = self
            .get_review_card(card_id)?
            .ok_or_else(|| LearningPortError::NotFound(card_id.to_string()))?;
        let outcome = SpacedRepetitionScheduler::schedule(
            card.interval_days,
            card.ease,
            card.repetition_count,
            card.lapses,
            rating,
            now,
        );
        {
            let mut guard = self.cards.lock();
            if let Some(stored) = guard.get_mut(card_id) {
                stored.interval_days = outcome.interval_days;
                stored.ease = outcome.ease;
                stored.due_at = outcome.due_at;
                stored.repetition_count = outcome.repetition_count;
                stored.lapses = outcome.lapses;
                stored.last_reviewed_at = Some(now);
            }
        }
        // 同步进度（与真实 store 语义一致：评分会落到 learning_progress）。
        let key = format!("{}:{}:{}", card.module, card.entity_type, card.entity_id);
        let mut guard = self.progress.lock();
        let mut progress = guard.get(&key).cloned().unwrap_or_else(|| {
            LearningProgress::new(
                &card.module,
                &card.entity_type,
                &card.entity_id,
                card.prompt.clone(),
                now,
            )
        });
        progress.review_count += 1;
        if rating != ReviewRating::Again {
            progress.correct_count += 1;
        } else {
            progress.incorrect_count += 1;
        }
        progress.interval_days = outcome.interval_days;
        progress.ease = outcome.ease;
        progress.next_review_at = Some(outcome.due_at);
        progress.last_studied_at = now;
        let (score, status) = MasteryCalculator::calculate(
            progress.study_count,
            progress.correct_count,
            progress.incorrect_count,
            progress.interval_days,
            progress.last_studied_at,
            now,
        );
        progress.mastery_score = score;
        progress.status = status;
        guard.insert(key, progress);
        Ok(outcome)
    }

    // ---- 合集/图谱（本测试不关心，最小实现）----
    fn create_collection(
        &self,
        _title: &str,
        _description: Option<&str>,
        _tags: &[String],
        _now: i64,
    ) -> Result<Collection, LearningPortError> {
        unimplemented!()
    }
    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        Ok(Vec::new())
    }
    fn add_collection_item(
        &self,
        _collection_id: &str,
        _entity: CollectionItemRef,
        _now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        unimplemented!()
    }
    fn list_collection_items(
        &self,
        _collection_id: &str,
    ) -> Result<Vec<CollectionItem>, LearningPortError> {
        Ok(Vec::new())
    }
    fn remove_collection_item(&self, _item_id: &str) -> Result<(), LearningPortError> {
        Ok(())
    }
    fn delete_collection(&self, _collection_id: &str) -> Result<(), LearningPortError> {
        Ok(())
    }
    fn get_continue_items(&self, _limit: usize) -> Result<Vec<ContinueItem>, LearningPortError> {
        Ok(Vec::new())
    }
    fn count_topics_studied_today(&self, _day_start_ts: i64) -> Result<u32, LearningPortError> {
        Ok(0)
    }
    fn get_average_mastery(&self) -> Result<f64, LearningPortError> {
        let guard = self.progress.lock();
        if guard.is_empty() {
            return Ok(0.0);
        }
        Ok(guard.values().map(|p| p.mastery_score).sum::<f64>() / guard.len() as f64)
    }
}

// ============================================================================
// 内存课程库
// ============================================================================

#[derive(Default)]
struct FakeCourseStore {
    courses: Mutex<Vec<Course>>,
    books: Mutex<Vec<CourseBook>>,
    lessons: Mutex<Vec<CourseLesson>>,
    sentences: Mutex<Vec<LessonSentence>>,
    vocab: Mutex<Vec<LessonVocab>>,
    progress: Mutex<HashMap<String, LessonProgress>>,
    occurrences: Mutex<Vec<WordOccurrence>>,
    plan: Mutex<Option<LearningPlan>>,
    dict: Mutex<HashMap<String, WordEntry>>,
}

impl FakeCourseStore {
    fn with_lesson() -> Self {
        let store = Self::default();
        store.courses.lock().push(Course {
            id: "nce".into(),
            language: devtoolbox_core::language::LanguageCode::Eng,
            code: "nce".into(),
            title: "New Concept English".into(),
            description: None,
            source_type: "nce".into(),
            created_at: NOW,
        });
        store.books.lock().push(CourseBook {
            id: "nce:2".into(),
            course_id: "nce".into(),
            book_no: 2,
            title: "New Concept English 2".into(),
            subtitle: None,
            total_lessons: 96,
        });
        store.lessons.lock().push(CourseLesson {
            id: "nce:2:17".into(),
            book_id: "nce:2".into(),
            lesson_no: 17,
            title: "Always Young".into(),
            audio_path: Some("/media/NCE2/017.mp3".into()),
            duration_ms: Some(120_000),
            sentence_count: 4,
            vocab_count: 3,
        });
        let sentences = [
            (
                "My aunt Jennifer is an actress.",
                Some("我的姑母詹妮弗是一位演员。"),
            ),
            (
                "She must be at least thirty-five years old.",
                Some("她至少也有35岁了。"),
            ),
            ("Don't hesitate to ask questions.", Some("有问题尽管问。")),
            ("She often appears on the stage as a young girl.", None),
        ];
        for (sequence, (english, chinese)) in sentences.into_iter().enumerate() {
            store.sentences.lock().push(LessonSentence {
                id: format!("nce:2:17#{sequence}"),
                lesson_id: "nce:2:17".into(),
                sequence: sequence as u32,
                start_ms: sequence as i64 * 3000,
                end_ms: sequence as i64 * 3000 + 3000,
                english: english.into(),
                chinese: chinese.map(Into::into),
            });
        }
        let vocab = [
            ("hesitate", "hesitate", "v. 犹豫；迟疑", 8000, 80),
            ("actress", "actress", "n. 女演员", 12000, 85),
            ("stage", "stage", "n. 舞台", 2500, 45),
        ];
        for (word, surface, translation, frq, importance) in vocab {
            store.vocab.lock().push(LessonVocab {
                lesson_id: "nce:2:17".into(),
                word: word.into(),
                surface: Some(surface.into()),
                sentence_id: Some("nce:2:17#2".into()),
                context: Some("Don't hesitate to ask questions.".into()),
                phonetic: Some("hɪˈzɪteɪt".into()),
                pos: Some("v.".into()),
                translation_zh: Some(translation.into()),
                definition_en: None,
                frequency: frq,
                tags: vec!["cet4".into()],
                importance,
            });
            store.dict.lock().insert(
                word.into(),
                WordEntry {
                    word: word.into(),
                    lemma: word.into(),
                    translation_zh: Some(translation.into()),
                    phonetic: Some("hɪˈzɪteɪt".into()),
                    ..WordEntry::default()
                },
            );
        }
        store
    }
}

impl CourseStorePort for FakeCourseStore {
    fn courses(&self) -> Result<Vec<Course>, String> {
        Ok(self.courses.lock().clone())
    }
    fn course_books(&self, _course_id: &str) -> Result<Vec<CourseBook>, String> {
        Ok(self.books.lock().clone())
    }
    fn book(&self, book_id: &str) -> Result<Option<CourseBook>, String> {
        Ok(self.books.lock().iter().find(|b| b.id == book_id).cloned())
    }
    fn book_lessons(&self, book_id: &str) -> Result<Vec<LessonListEntry>, String> {
        let progress = self.progress.lock();
        Ok(self
            .lessons
            .lock()
            .iter()
            .filter(|lesson| lesson.book_id == book_id)
            .map(|lesson| {
                let p = progress.get(&lesson.id);
                let status = match p {
                    Some(p) if p.completed_at.is_some() => LessonStatus::Completed,
                    Some(_) => LessonStatus::Learning,
                    None => LessonStatus::NotStarted,
                };
                LessonListEntry {
                    lesson: lesson.clone(),
                    status,
                    percent: p.map(|p| p.percent()).unwrap_or(0),
                }
            })
            .collect())
    }
    fn course_lesson(&self, lesson_id: &str) -> Result<Option<CourseLesson>, String> {
        Ok(self
            .lessons
            .lock()
            .iter()
            .find(|l| l.id == lesson_id)
            .cloned())
    }
    fn lesson_sentences(&self, lesson_id: &str) -> Result<Vec<LessonSentence>, String> {
        Ok(self
            .sentences
            .lock()
            .iter()
            .filter(|s| s.lesson_id == lesson_id)
            .cloned()
            .collect())
    }
    fn lesson_vocab(&self, lesson_id: &str) -> Result<Vec<LessonVocab>, String> {
        Ok(self
            .vocab
            .lock()
            .iter()
            .filter(|v| v.lesson_id == lesson_id)
            .cloned()
            .collect())
    }
    fn book_summary(&self, _book_id: &str) -> Result<BookSummary, String> {
        Ok(BookSummary {
            total_lessons: 1,
            ..BookSummary::default()
        })
    }
    fn lesson_progress(&self, lesson_id: &str) -> Result<Option<LessonProgress>, String> {
        Ok(self.progress.lock().get(lesson_id).cloned())
    }
    fn save_lesson_progress(&self, progress: &LessonProgress) -> Result<(), String> {
        self.progress
            .lock()
            .insert(progress.lesson_id.clone(), progress.clone());
        Ok(())
    }
    fn latest_learning_lesson(&self) -> Result<Option<LessonListEntry>, String> {
        let progress = self.progress.lock();
        let latest = progress.values().max_by_key(|p| p.updated_at);
        Ok(latest.and_then(|p| {
            self.lessons
                .lock()
                .iter()
                .find(|l| l.id == p.lesson_id)
                .map(|lesson| LessonListEntry {
                    lesson: lesson.clone(),
                    status: LessonStatus::Learning,
                    percent: p.percent(),
                })
        }))
    }
    fn recent_lesson_entries(&self, limit: usize) -> Result<Vec<LessonListEntry>, String> {
        let mut entries = Vec::new();
        let progress = self.progress.lock();
        for lesson in self.lessons.lock().iter() {
            if let Some(p) = progress.get(&lesson.id) {
                entries.push(LessonListEntry {
                    lesson: lesson.clone(),
                    status: LessonStatus::Learning,
                    percent: p.percent(),
                });
            }
        }
        entries.truncate(limit);
        Ok(entries)
    }
    fn study_seconds_since(&self, day_start: i64) -> Result<i64, String> {
        Ok(self
            .progress
            .lock()
            .values()
            .filter(|p| p.updated_at >= day_start)
            .map(|p| p.study_seconds)
            .sum())
    }
    fn study_streak_days(&self, _now: i64) -> Result<u32, String> {
        Ok(3)
    }
    fn learning_plan(&self, _language: &str) -> Result<Option<LearningPlan>, String> {
        Ok(self.plan.lock().clone())
    }
    fn save_learning_plan(&self, plan: &LearningPlan) -> Result<(), String> {
        *self.plan.lock() = Some(plan.clone());
        Ok(())
    }
    fn record_word_occurrence(&self, occurrence: &WordOccurrence) -> Result<(), String> {
        self.occurrences.lock().push(occurrence.clone());
        Ok(())
    }
    fn word_occurrences(&self, word: &str, limit: usize) -> Result<Vec<WordOccurrence>, String> {
        let mut list: Vec<_> = self
            .occurrences
            .lock()
            .iter()
            .filter(|o| o.word == word)
            .cloned()
            .collect();
        list.sort_by_key(|o| std::cmp::Reverse(o.occurred_at));
        list.truncate(limit);
        Ok(list)
    }
    fn word_occurrence_count(&self, word: &str) -> Result<i64, String> {
        Ok(self
            .occurrences
            .lock()
            .iter()
            .filter(|o| o.word == word)
            .count() as i64)
    }
    fn dict_lookup(&self, word: &str) -> Result<Option<WordEntry>, String> {
        Ok(self.dict.lock().get(&word.to_ascii_lowercase()).cloned())
    }
    fn dict_search(&self, query: &str, limit: usize) -> Result<Vec<WordEntry>, String> {
        let mut list: Vec<_> = self
            .dict
            .lock()
            .values()
            .filter(|entry| entry.word.starts_with(&query.to_ascii_lowercase()))
            .cloned()
            .collect();
        list.truncate(limit);
        Ok(list)
    }
    fn dict_count(&self) -> Result<i64, String> {
        Ok(self.dict.lock().len() as i64)
    }
    fn lesson_title_search(&self, query: &str, _limit: usize) -> Result<Vec<CourseLesson>, String> {
        Ok(self
            .lessons
            .lock()
            .iter()
            .filter(|l| {
                l.title
                    .to_ascii_lowercase()
                    .contains(&query.to_ascii_lowercase())
            })
            .cloned()
            .collect())
    }
}

fn service_with_lesson() -> (CourseService, Arc<FakeCourseStore>, Arc<FakePlatform>) {
    let store = Arc::new(FakeCourseStore::with_lesson());
    let platform = Arc::new(FakePlatform::default());
    let service = CourseService::new(
        store.clone(),
        Arc::new(PlatformLearningService::new(platform.clone())),
    );
    (service, store, platform)
}

// ============================================================================
// 测试
// ============================================================================

#[test]
fn today_empty_store_is_honest() {
    let store = Arc::new(FakeCourseStore::default());
    let platform = Arc::new(FakePlatform::default());
    let service = CourseService::new(store, Arc::new(PlatformLearningService::new(platform)));
    let today = service.today(NOW).expect("today");
    assert!(!today.imported);
    assert!(!today.dict_ready);
    assert!(today.continue_lesson.is_none());
    assert_eq!(today.due_reviews, 0);
}

#[test]
fn today_shows_continue_and_next_lesson() {
    let (service, _store, _platform) = service_with_lesson();
    service
        .save_plan(&LearningPlan {
            book_id: Some("nce:2".into()),
            course_id: Some("nce".into()),
            ..LearningPlan::default()
        })
        .expect("plan");
    // 制造学习痕迹。
    service
        .update_lesson_progress(
            "nce:2:17",
            super::ProgressPatch {
                stage: Some(LessonStage::Listen),
                position_ms: Some(30_000),
                sentence_seq: None,
                vocab_index: None,
                shadow_seq: None,
                study_seconds_delta: Some(120),
            },
            NOW,
        )
        .expect("progress");
    let today = service.today(NOW).expect("today");
    assert!(today.imported);
    assert!(today.dict_ready);
    let continue_lesson = today.continue_lesson.expect("continue");
    assert_eq!(continue_lesson.lesson.id, "nce:2:17");
    assert!(continue_lesson.percent > 0);
    assert_eq!(today.study_seconds_today, 120);
    // 17 课在 Learning，没有「下一课」（只有一课）。
    assert!(today.next_lesson.is_none());
}

#[test]
fn lesson_detail_combines_content_and_state() {
    let (service, _store, _platform) = service_with_lesson();
    let detail = service
        .lesson_detail("nce:2:17", NOW)
        .expect("detail")
        .expect("some");
    assert_eq!(detail.sentences.len(), 4);
    assert_eq!(detail.vocab.len(), 3);
    assert!(detail.vocab.iter().all(|v| v.state == "new"));
    assert_eq!(detail.progress.stage, LessonStage::Vocabulary);
    assert!(detail.lesson.audio_path.is_some());
}

#[test]
fn mark_word_unknown_creates_due_card_and_progress() {
    let (service, _store, platform) = service_with_lesson();
    let progress = service
        .mark_word("nce:2:17", "Hesitate", WordMark::Unknown, NOW)
        .expect("mark");
    assert_eq!(progress.entity_id, "en:hesitate");
    // Unknown → Again → 今天到期。
    let card = platform
        .cards
        .lock()
        .get("language:nce-word:hesitate")
        .cloned()
        .expect("card");
    assert_eq!(card.answer, "v. 犹豫；迟疑");
    assert!(card.due_at <= NOW + 600);
    assert!(card.due_at >= NOW - 600);
    // occurrence 已记。
    let lookup = service
        .lookup_word("hesitate", None, Some("nce:2:17"), NOW + 1)
        .expect("lookup");
    assert!(lookup.seen_count >= 2); // mark + lookup 各一次
    assert!(lookup.entry.is_some());
}

#[test]
fn mark_word_ratings_map_to_smart_intervals() {
    let (service, _store, platform) = service_with_lesson();
    // Know → Easy → 平台调度器首评 3 天。
    service
        .mark_word("nce:2:17", "stage", WordMark::Know, NOW)
        .expect("mark know");
    let know_card = platform
        .cards
        .lock()
        .get("language:nce-word:stage")
        .cloned()
        .expect("know card");
    assert!(
        know_card.due_at >= NOW + 3 * 86_400,
        "know should schedule ~3 days out"
    );

    // Fuzzy → Hard → 明天。
    service
        .mark_word("nce:2:17", "actress", WordMark::Fuzzy, NOW)
        .expect("mark fuzzy");
    let fuzzy_card = platform
        .cards
        .lock()
        .get("language:nce-word:actress")
        .cloned()
        .expect("fuzzy card");
    assert!(
        fuzzy_card.due_at >= NOW + 86_400 && fuzzy_card.due_at < NOW + 2 * 86_400,
        "fuzzy should schedule ~1 day out"
    );

    // Unknown → Again → 今天。
    service
        .mark_word("nce:2:17", "hesitate", WordMark::Unknown, NOW)
        .expect("mark unknown");
    let unknown_card = platform
        .cards
        .lock()
        .get("language:nce-word:hesitate")
        .cloned()
        .expect("unknown card");
    assert!(unknown_card.due_at <= NOW, "unknown should be due today");
}

#[test]
fn quiz_generation_uses_real_lesson_data() {
    let (service, _store, _platform) = service_with_lesson();
    let items = service.generate_quiz("nce:2:17").expect("quiz");
    // 3 词 → 3 词汇题；填空 ≥1；听写 ≥1；翻译 2（3 句有中文，take(2)）。
    let vocab = items
        .iter()
        .filter(|i| matches!(i, QuizItem::Vocabulary { .. }))
        .count();
    let fill = items
        .iter()
        .filter(|i| matches!(i, QuizItem::FillBlank { .. }))
        .count();
    let dictation = items
        .iter()
        .filter(|i| matches!(i, QuizItem::Dictation { .. }))
        .count();
    let translate = items
        .iter()
        .filter(|i| matches!(i, QuizItem::Translate { .. }))
        .count();
    assert_eq!(vocab, 3);
    assert!(fill >= 1);
    assert!(dictation >= 1);
    assert_eq!(translate, 2);
    // 词汇题答案必须在选项内。
    for item in &items {
        if let QuizItem::Vocabulary {
            options, answer, ..
        } = item
        {
            assert!(*answer < options.len());
        }
    }
    // 填空题答案确实是被挖掉的本课生词。
    let fill_item = items
        .iter()
        .find_map(|i| match i {
            QuizItem::FillBlank {
                sentence, answer, ..
            } => Some((sentence, answer)),
            _ => None,
        })
        .expect("fill");
    assert!(fill_item.0.contains("_____"));
    assert!(["hesitate", "actress", "stage"].contains(&fill_item.1.as_str()));
}

#[test]
fn submit_quiz_scores_and_reinforces_wrong_words() {
    let (service, _store, platform) = service_with_lesson();
    let result = service
        .submit_quiz(
            "nce:2:17",
            &[
                QuizAnswer {
                    item_index: 0,
                    correct: true,
                    user_answer: Some("hesitate|犹豫".into()),
                },
                QuizAnswer {
                    item_index: 1,
                    correct: false,
                    user_answer: Some("actress|选错了".into()),
                },
                QuizAnswer {
                    item_index: 2,
                    correct: true,
                    user_answer: None,
                },
            ],
            NOW,
        )
        .expect("submit");
    assert_eq!(result.total, 3);
    assert_eq!(result.correct, 2);
    assert_eq!(result.score, 66);
    assert_eq!(result.wrong_words, vec!["actress".to_string()]);
    // 错词 actress 应有一张今天到期的复习卡。
    let card = platform
        .cards
        .lock()
        .get("language:nce-word:actress")
        .cloned()
        .expect("wrong word card");
    assert!(card.due_at <= NOW + 600);
    // 分数 ≥60 → 课时完成 + 明天复习本课的卡。
    let progress = service
        .lesson_detail("nce:2:17", NOW)
        .expect("detail")
        .expect("some")
        .progress;
    assert_eq!(progress.stage, LessonStage::Done);
    assert_eq!(progress.quiz_score, Some(66));
    assert!(progress.completed_at.is_some());
    assert!(
        platform
            .cards
            .lock()
            .contains_key("language:nce-lesson:nce:2:17")
    );
}

#[test]
fn failing_quiz_does_not_complete_lesson() {
    let (service, _store, _platform) = service_with_lesson();
    let result = service
        .submit_quiz(
            "nce:2:17",
            &[
                QuizAnswer {
                    item_index: 0,
                    correct: false,
                    user_answer: None,
                },
                QuizAnswer {
                    item_index: 1,
                    correct: false,
                    user_answer: None,
                },
            ],
            NOW,
        )
        .expect("submit");
    assert_eq!(result.score, 0);
    let progress = service
        .lesson_detail("nce:2:17", NOW)
        .expect("detail")
        .expect("some")
        .progress;
    assert_eq!(progress.stage, LessonStage::Quiz);
    assert!(progress.completed_at.is_none());
}

#[test]
fn blank_word_matches_whole_word_case_insensitive() {
    use super::blank_word;
    let vocab = LessonVocab {
        lesson_id: "x".into(),
        word: "stage".into(),
        surface: Some("Stage".into()),
        sentence_id: None,
        context: None,
        phonetic: None,
        pos: None,
        translation_zh: None,
        definition_en: None,
        frequency: 0,
        tags: vec![],
        importance: 0,
    };
    assert_eq!(
        blank_word("She often appears on the stage as a girl.", &vocab),
        Some("She often appears on the _____ as a girl.".to_string())
    );
    // 不匹配词内子串（stagecoach 里的 stage 不能挖）。
    assert_eq!(blank_word("The stagecoach left.", &vocab), None);
}

#[test]
fn dictionary_service_lookup_is_case_insensitive() {
    let store = Arc::new(FakeCourseStore::with_lesson());
    let dict = DictionaryService::new(store);
    let entry = dict.lookup("HESITATE").expect("lookup").expect("found");
    assert_eq!(entry.word, "hesitate");
    assert!(dict.lookup("missing").expect("miss").is_none());
    assert!(dict.is_ready().expect("ready"));
}

#[test]
fn search_covers_words_and_lessons() {
    let (service, _store, _platform) = service_with_lesson();
    let result = service.search("hesi", 10).expect("search");
    assert_eq!(result.words.len(), 1);
    let lessons = service.search("Always", 10).expect("lessons");
    assert_eq!(lessons.lessons.len(), 1);
}
