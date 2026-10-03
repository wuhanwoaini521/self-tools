//! Language 用例层测试。
//!
//! 范围：**编排正确性**——用例是否调用了正确的能力、参数是否正确、结果如何组装。
//! 真实 SQLite 行为（切分、迁移、持久化）在 `crates/infrastructure` 覆盖。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::language::{
    Difficulty, LanguageCode, LanguageLearningItem, LearningItemType, Mistake,
};
use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, LearningAction, LearningEvent,
    LearningProgress, LearningStatus, MasteryCalculator, ReviewQueueItem, ReviewQueueStats,
    ReviewRating, ReviewScheduleOutcome, SpacedRepetitionScheduler, UniversalReviewCard,
};

use crate::language::mocks::FakeLanguageStore;
use crate::language::ports::LanguageStorePort as _;
use crate::language::{LanguageLearningService, LanguageService, StudyAction};
use crate::learning::ports::{LearningPortError, LearningStorePort};
use crate::learning::service::LearningService as PlatformLearningService;

const NOW: i64 = 1_700_000_000;

// ============================================================================
// 平台的内存实现（供本模块测试组合真实 LearningService）
// ============================================================================

/// 应用层测试用的平台学习存储：与 `learning/store.rs` 同一套算法，只是放内存。
#[derive(Default)]
pub struct InMemoryLearningStore {
    events: Mutex<Vec<LearningEvent>>,
    progress: Mutex<HashMap<String, LearningProgress>>,
    cards: Mutex<HashMap<String, UniversalReviewCard>>,
}

impl InMemoryLearningStore {
    fn entity_key(event: &LearningEvent) -> String {
        format!("{}:{}:{}", event.module, event.entity_type, event.entity_id)
    }
}

impl LearningStorePort for InMemoryLearningStore {
    fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError> {
        self.events.lock().push(event.clone());
        let key = Self::entity_key(event);
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
            LearningAction::Correct => {
                progress.review_count += 1;
                progress.correct_count += 1;
            }
            LearningAction::Incorrect => {
                progress.review_count += 1;
                progress.incorrect_count += 1;
            }
            LearningAction::Review => progress.review_count += 1,
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
        _status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        let mut list: Vec<LearningProgress> = self
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
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError> {
        let mut items: Vec<ReviewQueueItem> = self
            .cards
            .lock()
            .values()
            .filter(|card| card.due_at <= now)
            .filter(|card| module_filter.is_none_or(|module| card.module == module))
            .cloned()
            .map(|card| {
                let is_overdue = card.due_at < now - 86_400;
                let urgency = (100.0 - card.mastery_score)
                    + ((now - card.due_at) as f64 / 86_400.0).max(0.0) * 5.0;
                ReviewQueueItem {
                    card,
                    is_overdue,
                    urgency_score: urgency,
                }
            })
            .collect();
        items.sort_by(|left, right| {
            left.urgency_score
                .partial_cmp(&right.urgency_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(limit);
        Ok(items)
    }

    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        let cards = self.cards.lock();
        Ok(ReviewQueueStats {
            total_due: cards.values().filter(|card| card.due_at <= now).count() as u32,
            due_count: cards.values().filter(|card| card.due_at <= now).count() as u32,
            overdue_count: cards
                .values()
                .filter(|card| card.due_at <= now - 86_400)
                .count() as u32,
            upcoming_count: 0,
            by_module: HashMap::new(),
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
        let action = if outcome.is_correct {
            LearningAction::Correct
        } else {
            LearningAction::Incorrect
        };
        self.record_event(&LearningEvent {
            id: format!("evt_{now}_{card_id}"),
            module: card.module.clone(),
            entity_type: card.entity_type.clone(),
            entity_id: card.entity_id.clone(),
            entity_title: Some(card.prompt.clone()),
            action,
            timestamp: now,
            duration_ms: None,
            metadata: serde_json::json!({}),
            source: Some("review_center".to_string()),
        })?;
        Ok(outcome)
    }

    fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        _tags: &[String],
        now: i64,
    ) -> Result<Collection, LearningPortError> {
        Ok(Collection {
            id: format!("col_{now}"),
            title: title.to_string(),
            description: description.map(str::to_string),
            tags: Vec::new(),
            item_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        Ok(Vec::new())
    }

    fn add_collection_item(
        &self,
        collection_id: &str,
        _entity: CollectionItemRef,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        Ok(CollectionItem {
            id: format!("ci_{collection_id}_{now}"),
            collection_id: collection_id.to_string(),
            module: String::new(),
            entity_type: String::new(),
            entity_id: String::new(),
            title: String::new(),
            note: None,
            added_at: now,
        })
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
        Ok(0.0)
    }
}

// ============================================================================
// 夹具
// ============================================================================

fn platform() -> Arc<PlatformLearningService> {
    Arc::new(PlatformLearningService::new(Arc::new(
        InMemoryLearningStore::default(),
    )))
}

fn learning_service() -> (Arc<FakeLanguageStore>, LanguageLearningService) {
    let content = Arc::new(FakeLanguageStore::new());
    content.insert_word("jmdict:1", LanguageCode::Jap, "駅", "车站");
    content.insert_word("jmdict:2", LanguageCode::Jap, "電車", "电车");
    let service = LanguageLearningService::new(content.clone(), platform());
    (content, service)
}

fn word(id: &str, text: &str) -> LanguageLearningItem {
    LanguageLearningItem::from_item(
        &devtoolbox_core::language::LanguageItem::plain(
            LanguageCode::Jap,
            devtoolbox_core::language::LanguageItemType::Word,
            id.to_string(),
            text.to_string(),
            "jmdict".to_string(),
        ),
        Some("车站".to_string()),
        None,
        Difficulty::Unknown,
    )
    .expect("word")
}

// ============================================================================
// 用例测试
// ============================================================================

#[test]
fn languages_always_lists_four_supported_codes() {
    let store = Arc::new(FakeLanguageStore::new());
    let service = LanguageService::new(store);
    let languages = service.languages().expect("languages");
    assert_eq!(languages.len(), 4);
    assert!(
        languages.iter().any(|info| info.code == "jpn"),
        "日语应始终在列"
    );
}

#[test]
fn detail_returns_none_for_unknown_id() {
    let store = Arc::new(FakeLanguageStore::new());
    let service = LanguageService::new(store);
    assert!(service.detail("不存在").expect("detail").is_none());
}

#[test]
fn detail_pairs_each_relation_with_its_own_word() {
    let store = Arc::new(FakeLanguageStore::new());
    store.insert_word("jmdict:1", LanguageCode::Jap, "駅", "车站");
    let service = LanguageService::new(store);
    let detail = service.detail("jmdict:1").expect("detail").expect("exists");
    // 夹具无关联关系 → 关联列表必须为空，而不是错位到别的词
    assert!(detail.relations.is_empty());
    assert_eq!(detail.meanings.len(), 1);
}

#[test]
fn study_event_reaches_platform_progress() {
    let (_content, service) = learning_service();
    let progress = service
        .record_study(&word("jmdict:1", "駅"), StudyAction::Study, NOW)
        .expect("record study");

    assert_eq!(progress.module, "language");
    assert_eq!(progress.entity_type, "word");
    assert_eq!(progress.entity_id, "jmdict:1");
    assert_eq!(progress.entity_key, "language:word:jmdict:1");
    assert_eq!(progress.study_count, 1);
    assert!(progress.mastery_score > 0.0, "学习后掌握度应大于 0");
}

#[test]
fn add_to_review_creates_a_due_platform_card() {
    let (_content, service) = learning_service();
    service
        .add_to_review(&word("jmdict:1", "駅"), NOW)
        .expect("add to review");

    let queue = service.review_queue(10, NOW).expect("queue");
    assert_eq!(queue.len(), 1, "应生成一张平台复习卡");
    let card = &queue[0].card;
    assert_eq!(card.module, "language");
    assert_eq!(card.entity_id, "jmdict:1");
    assert!(card.due_at <= NOW, "刚加入复习的卡应立即到期");
}

#[test]
fn wrong_answer_records_a_mistake_and_increases_error_count() {
    let (content, service) = learning_service();
    service
        .add_to_review(&word("jmdict:1", "駅"), NOW)
        .expect("add to review");
    let card = service.review_queue(10, NOW).expect("queue")[0]
        .card
        .clone();

    // 第一次答错
    service
        .submit_review(&card, "火车", ReviewRating::Again, NOW)
        .expect("submit");
    let mistakes = service.mistakes(10).expect("mistakes");
    assert_eq!(mistakes.len(), 1);
    assert_eq!(mistakes[0].user_answer, "火车");
    assert_eq!(mistakes[0].correct_answer, card.answer);
    assert_eq!(mistakes[0].error_count, 1);

    // 再次答错：同一条累加，不新增行
    service
        .submit_review(&card, "公交车", ReviewRating::Again, NOW + 60)
        .expect("submit again");
    let mistakes = service.mistakes(10).expect("mistakes");
    assert_eq!(mistakes.len(), 1, "同一错误不应重复插入");
    assert_eq!(mistakes[0].error_count, 2);
    assert_eq!(
        mistakes[0].user_answer, "公交车",
        "应记录最近一次的错误答案"
    );
    assert!(content.called("record_mistake"));
}

#[test]
fn correct_answer_clears_the_mistake() {
    let (_content, service) = learning_service();
    service
        .add_to_review(&word("jmdict:1", "駅"), NOW)
        .expect("add to review");
    let card = service.review_queue(10, NOW).expect("queue")[0]
        .card
        .clone();

    service
        .submit_review(&card, "错", ReviewRating::Again, NOW)
        .expect("wrong");
    assert_eq!(service.mistake_count().expect("count"), 1);

    // 下一轮到期后答对
    let outcome = service
        .submit_review(&card, "车站", ReviewRating::Good, NOW + 86_400)
        .expect("correct");
    assert!(outcome.is_correct);
    assert_eq!(service.mistake_count().expect("count"), 0, "答对应移除错题");
    assert!(service.mistakes(10).expect("mistakes").is_empty());
}

#[test]
fn lesson_references_items_and_resumes_from_saved_position() {
    let (content, service) = learning_service();
    let lesson = service
        .create_lesson(
            LanguageCode::Jap,
            "日本 · 交通基础",
            &["jmdict:1".to_string(), "jmdict:2".to_string()],
            NOW,
        )
        .expect("create lesson");

    assert_eq!(lesson.steps.len(), 2, "两个已知条目应成为两步");
    assert_eq!(lesson.step_position("jmdict:2"), Some(1));
    assert!(content.called("upsert_lesson"));

    // 学习到第 2 步后退出
    service
        .save_position(&lesson.id, 1, NOW)
        .expect("save position");
    assert_eq!(service.resume_step(&lesson), 1, "应恢复到第 2 步");

    // Continue 入口应能列出这节课
    let continuing = service.continue_lessons(5).expect("continue");
    assert_eq!(continuing.len(), 1);
    assert_eq!(continuing[0].lesson.id, lesson.id);
    assert_eq!(continuing[0].step_index, 1);
    assert_eq!(continuing[0].total_steps, 2);
}

#[test]
fn lesson_skips_ids_that_are_not_in_the_dictionary() {
    let (_content, service) = learning_service();
    let lesson = service
        .create_lesson(
            LanguageCode::Jap,
            "混合",
            &["jmdict:1".to_string(), "不存在".to_string()],
            NOW,
        )
        .expect("create lesson");
    assert_eq!(
        lesson.steps.len(),
        1,
        "不存在的条目应被跳过，而不是产生空步骤"
    );
    assert_eq!(lesson.steps[0].item_id, "jmdict:1");
}

#[test]
fn resume_position_is_clamped_when_lesson_shrinks() {
    let (content, service) = learning_service();
    let lesson = service
        .create_lesson(
            LanguageCode::Jap,
            "两课",
            &["jmdict:1".to_string(), "jmdict:2".to_string()],
            NOW,
        )
        .expect("create lesson");
    service.save_position(&lesson.id, 99, NOW).expect("save");

    // 课程内容被改短后，越界位置必须收敛而不是越界
    let mut shorter = lesson.clone();
    shorter.steps.truncate(1);
    content.upsert_lesson(&shorter).expect("shrink");
    let loaded = service.lesson(&lesson.id).expect("read").expect("exists");
    assert_eq!(service.resume_step(&loaded), 0, "越界位置应收敛到有效范围");
}

#[test]
fn weak_items_rank_by_mastery_then_recency() {
    let (_content, service) = learning_service();
    let strong = word("jmdict:1", "駅");
    let weak = word("jmdict:2", "電車");

    service
        .record_study(&weak, StudyAction::Study, NOW - 86_400)
        .expect("study weak");
    // 弱项连续答错 → 掌握度低、错误数高
    for _ in 0..2 {
        service
            .submit_review(
                &UniversalReviewCard {
                    id: "card_language_word_jmdict:2".into(),
                    module: "language".into(),
                    entity_id: "jmdict:2".into(),
                    entity_type: "word".into(),
                    card_type: devtoolbox_core::learning::ReviewCardType::Recall,
                    prompt: "「电车」对应哪个词？".into(),
                    answer: "電車".into(),
                    options: None,
                    hint: None,
                    context: None,
                    due_at: NOW - 86_400,
                    interval_days: 0.0,
                    ease: 2.5,
                    mastery_score: 0.0,
                    repetition_count: 0,
                    lapses: 0,
                    last_reviewed_at: None,
                    created_at: NOW - 86_400,
                },
                "电车",
                ReviewRating::Again,
                NOW - 86_400,
            )
            .expect("rate");
    }
    service
        .record_study(&strong, StudyAction::Study, NOW)
        .expect("study strong");

    let weak_items = service.weak_items(10).expect("weak");
    assert!(!weak_items.is_empty());
    assert_eq!(
        weak_items[0].entity_id, "jmdict:2",
        "掌握度最低的应排在最前"
    );
    assert!(weak_items[0].incorrect_count >= 1);
    assert_eq!(
        weak_items[0].difficulty,
        Difficulty::derive(weak_items[0].incorrect_count),
        "难度应与错误次数一致"
    );
}

#[test]
fn weak_items_is_empty_before_any_study() {
    let (_content, service) = learning_service();
    assert!(
        service.weak_items(10).expect("weak").is_empty(),
        "没有任何学习记录时不得编造薄弱项"
    );
}

#[test]
fn sentence_study_is_sourced_from_the_dictionary() {
    let content = Arc::new(FakeLanguageStore::new());
    content.insert_sentence("tatoeba:4812", LanguageCode::Jap, "私は日本語が話せない。");
    let service = LanguageLearningService::new(content.clone(), platform());

    let study = service
        .sentence_study("tatoeba:4812")
        .expect("study")
        .expect("exists");
    assert_eq!(study.original, "私は日本語が話せない。");
    assert!(content.called("sentence_study"));

    assert!(
        service.sentence_study("不存在").expect("study").is_none(),
        "不存在的句子应返回 None 而不是空对象"
    );
}

#[test]
fn mistake_listing_is_ordered_by_most_recent() {
    let (content, service) = learning_service();
    let make = |id: &str, at: i64| Mistake {
        id: id.to_string(),
        item_id: "jmdict:1".to_string(),
        item_type: LearningItemType::Word,
        language: LanguageCode::Jap,
        content: "駅".to_string(),
        question: "駅".to_string(),
        user_answer: "火车".to_string(),
        correct_answer: "车站".to_string(),
        error_count: 1,
        last_missed_at: at,
    };
    content
        .record_mistake(&make("old", NOW - 10_000), "old")
        .expect("old");
    content
        .record_mistake(&make("new", NOW), "new")
        .expect("new");

    let mistakes = service.mistakes(10).expect("mistakes");
    assert_eq!(mistakes[0].id, "new", "最近答错的应排最前");
}

#[test]
fn study_queue_puts_due_reviews_first_then_new_content() {
    // 「进 Language 就该有卡片」的基础：队列不能是空的，
    // 且到期复习必须排在未学新词之前。
    let (content, service) = learning_service();
    let now = NOW;

    // 空库 → 全靠新内容，仍要有卡片
    let fresh = service
        .study_queue(LanguageCode::Jap, 5, now)
        .expect("study queue");
    assert!(!fresh.is_empty(), "新装的库也必须给得出卡片");
    assert!(
        fresh.iter().all(|card| !card.from_review),
        "还没加过复习，卡片不应标记为来自复习队列"
    );

    // 把第一个词加进复习 → 它应回到队首，且带 card_id
    let first = fresh[0].item.clone();
    service.add_to_review(&first, now).expect("add to review");
    let next = service
        .study_queue(LanguageCode::Jap, 5, now + 60)
        .expect("study queue");
    assert!(!next.is_empty());
    assert_eq!(next[0].item.id, first.id, "到期复习应排在最前");
    assert!(next[0].from_review, "应标记为来自复习队列");
    assert!(
        next[0].card_id.is_some(),
        "复习卡片必须带 card_id，否则提交评分时找不到卡"
    );
    assert!(content.called("next_new_items"));
}

#[test]
fn study_queue_learned_words_are_not_offered_again_as_new() {
    let (_content, service) = learning_service();
    let now = NOW;
    let first = service
        .study_queue(LanguageCode::Jap, 1, now)
        .expect("queue")
        .remove(0)
        .item;
    service
        .record_study(&first, StudyAction::Study, now)
        .expect("study");

    let rest = service
        .study_queue(LanguageCode::Jap, 10, now + 1)
        .expect("queue");
    assert!(
        !rest.iter().any(|card| card.item.id == first.id),
        "已经学过的词不该再作为「新内容」发一遍"
    );
}

#[test]
fn study_queue_is_empty_when_nothing_to_learn() {
    let (_content, service) = learning_service();
    assert!(
        service
            .study_queue(LanguageCode::Jap, 0, NOW)
            .expect("queue")
            .is_empty(),
        "limit=0 应直接返回空"
    );
}
