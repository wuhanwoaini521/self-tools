use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, KnowledgeReviewCardDraft,
    LearningAction, LearningEvent, LearningProgress, LearningStatus, MasteryCalculator,
    ReviewCardType, ReviewQueueItem, ReviewQueueStats, ReviewRating, ReviewScheduleOutcome,
    SpacedRepetitionScheduler, UniversalReviewCard,
};

use crate::learning::ports::{LearningPortError, LearningStorePort};
use crate::learning::service::LearningService;

#[derive(Default)]
struct MockLearningStore {
    events: Mutex<Vec<LearningEvent>>,
    progress: Mutex<HashMap<String, LearningProgress>>,
    cards: Mutex<HashMap<String, UniversalReviewCard>>,
    collections: Mutex<HashMap<String, Collection>>,
    collection_items: Mutex<Vec<CollectionItem>>,
}

impl LearningStorePort for MockLearningStore {
    fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError> {
        let entity_key = format!("{}:{}:{}", event.module, event.entity_type, event.entity_id);
        let mut events_guard = self.events.lock().unwrap();
        events_guard.push(event.clone());

        let mut progress_guard = self.progress.lock().unwrap();
        let existing = progress_guard.get(&entity_key).cloned();

        let study_count = existing.as_ref().map(|p| p.study_count).unwrap_or(0)
            + if event.action == LearningAction::Study {
                1
            } else {
                0
            };
        let review_count = existing.as_ref().map(|p| p.review_count).unwrap_or(0)
            + if event.action == LearningAction::Review {
                1
            } else {
                0
            };
        let correct_count = existing.as_ref().map(|p| p.correct_count).unwrap_or(0)
            + if event.action == LearningAction::Correct {
                1
            } else {
                0
            };
        let incorrect_count = existing.as_ref().map(|p| p.incorrect_count).unwrap_or(0)
            + if event.action == LearningAction::Incorrect {
                1
            } else {
                0
            };

        let (mastery_score, status) = MasteryCalculator::calculate(
            study_count,
            correct_count,
            incorrect_count,
            existing.as_ref().map(|p| p.interval_days).unwrap_or(0.0),
            event.timestamp,
            event.timestamp,
        );

        let progress = LearningProgress {
            entity_key: entity_key.clone(),
            module: event.module.clone(),
            entity_type: event.entity_type.clone(),
            entity_id: event.entity_id.clone(),
            entity_title: event
                .entity_title
                .clone()
                .unwrap_or_else(|| event.entity_id.clone()),
            status,
            mastery_score,
            study_count,
            review_count,
            correct_count,
            incorrect_count,
            last_studied_at: event.timestamp,
            next_review_at: Some(event.timestamp + 86400),
            interval_days: existing.as_ref().map(|p| p.interval_days).unwrap_or(0.0),
            ease: existing.as_ref().map(|p| p.ease).unwrap_or(2.5),
            custom_tags: Vec::new(),
        };

        progress_guard.insert(entity_key, progress.clone());
        Ok(progress)
    }

    fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, LearningPortError> {
        Ok(self.progress.lock().unwrap().get(entity_key).cloned())
    }

    fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        let guard = self.progress.lock().unwrap();
        let mut list: Vec<LearningProgress> = guard
            .values()
            .filter(|p| module_filter.map(|m| p.module == m).unwrap_or(true))
            .filter(|p| status_filter.map(|s| p.status == s).unwrap_or(true))
            .cloned()
            .collect();
        list.truncate(limit);
        Ok(list)
    }

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError> {
        self.cards
            .lock()
            .unwrap()
            .insert(card.id.clone(), card.clone());
        Ok(())
    }

    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, LearningPortError> {
        Ok(self.cards.lock().unwrap().get(card_id).cloned())
    }

    fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError> {
        let guard = self.cards.lock().unwrap();
        let mut due: Vec<ReviewQueueItem> = guard
            .values()
            .filter(|c| module_filter.map(|m| c.module == m).unwrap_or(true))
            .filter(|c| c.due_at <= now)
            .map(|c| ReviewQueueItem {
                card: c.clone(),
                is_overdue: now > c.due_at,
                urgency_score: if now > c.due_at {
                    (now - c.due_at) as f64 / 3600.0
                } else {
                    0.0
                },
            })
            .collect();
        due.truncate(limit);
        Ok(due)
    }

    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        let cards_guard = self.cards.lock().unwrap();

        let mut total_due = 0;
        let mut overdue_count = 0;
        let mut upcoming_count = 0;
        let mut by_module: HashMap<String, u32> = HashMap::new();

        for card in cards_guard.values() {
            if card.due_at <= now {
                total_due += 1;
                if now - card.due_at > 86400 {
                    overdue_count += 1;
                }
                *by_module.entry(card.module.clone()).or_insert(0) += 1;
            } else {
                upcoming_count += 1;
            }
        }

        let total_cards = total_due + upcoming_count;
        Ok(ReviewQueueStats {
            total_due,
            due_count: total_due,
            overdue_count,
            upcoming_count,
            by_module,
            mastered_count: 0,
            learning_count: 0,
            total_cards,
        })
    }

    fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError> {
        let mut cards_guard = self.cards.lock().unwrap();
        let card = cards_guard
            .get_mut(card_id)
            .ok_or_else(|| LearningPortError::NotFound(card_id.to_string()))?;

        let outcome = SpacedRepetitionScheduler::schedule(
            card.interval_days,
            card.ease,
            card.repetition_count,
            card.lapses,
            rating,
            now,
        );

        card.interval_days = outcome.interval_days;
        card.ease = outcome.ease;
        card.lapses = outcome.lapses;
        card.repetition_count = outcome.repetition_count;
        card.due_at = outcome.due_at;
        card.last_reviewed_at = Some(now);

        Ok(outcome)
    }

    fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<Collection, LearningPortError> {
        let id = format!("col_{}", now);
        let col = Collection {
            id: id.clone(),
            title: title.to_string(),
            description: description.map(str::to_string),
            tags: tags.to_vec(),
            item_count: 0,
            created_at: now,
            updated_at: now,
        };
        self.collections.lock().unwrap().insert(id, col.clone());
        Ok(col)
    }

    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        Ok(self.collections.lock().unwrap().values().cloned().collect())
    }

    fn add_collection_item(
        &self,
        collection_id: &str,
        entity: CollectionItemRef,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        let item = CollectionItem {
            id: format!("item_{}", now),
            collection_id: collection_id.to_string(),
            module: entity.module,
            entity_type: entity.entity_type,
            entity_id: entity.entity_id,
            title: entity.title,
            note: entity.note,
            added_at: now,
        };
        self.collection_items.lock().unwrap().push(item.clone());
        if let Some(col) = self.collections.lock().unwrap().get_mut(collection_id) {
            col.item_count += 1;
        }
        Ok(item)
    }

    fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItem>, LearningPortError> {
        Ok(self
            .collection_items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.collection_id == collection_id)
            .cloned()
            .collect())
    }

    fn update_collection_item_note(
        &self,
        item_id: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        let mut items = self.collection_items.lock().unwrap();
        let item = items
            .iter_mut()
            .find(|item| item.id == item_id)
            .ok_or_else(|| LearningPortError::NotFound(item_id.to_string()))?;
        item.note = note.map(str::to_string);
        let updated = item.clone();
        let collection_id = item.collection_id.clone();
        drop(items);
        if let Some(collection) = self.collections.lock().unwrap().get_mut(&collection_id) {
            collection.updated_at = now;
        }
        Ok(updated)
    }

    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError> {
        self.collection_items
            .lock()
            .unwrap()
            .retain(|i| i.id != item_id);
        Ok(())
    }

    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError> {
        self.collections.lock().unwrap().remove(collection_id);
        self.collection_items
            .lock()
            .unwrap()
            .retain(|i| i.collection_id != collection_id);
        Ok(())
    }

    fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, LearningPortError> {
        let progress = self.progress.lock().unwrap();
        let mut items: Vec<ContinueItem> = progress
            .values()
            .map(|p| ContinueItem {
                module: p.module.clone(),
                entity_type: p.entity_type.clone(),
                entity_id: p.entity_id.clone(),
                title: p.entity_title.clone(),
                subtitle: None,
                progress_percent: Some(p.mastery_score),
                last_studied_at: p.last_studied_at,
                action_target: format!("{}:{}:{}", p.module, p.entity_type, p.entity_id),
            })
            .collect();
        items.truncate(limit);
        Ok(items)
    }

    fn count_topics_studied_today(&self, _day_start_ts: i64) -> Result<u32, LearningPortError> {
        Ok(self.progress.lock().unwrap().len() as u32)
    }

    fn get_average_mastery(&self) -> Result<f64, LearningPortError> {
        let guard = self.progress.lock().unwrap();
        if guard.is_empty() {
            return Ok(0.0);
        }
        let sum: f64 = guard.values().map(|p| p.mastery_score).sum();
        Ok(sum / guard.len() as f64)
    }
}

#[test]
fn saved_collection_items_are_searchable_by_personal_note_and_retain_source_identity() {
    let service = LearningService::new(Arc::new(MockLearningStore::default()));
    let collection = service
        .create_collection("唐代洛阳", None, &[], 1000)
        .expect("create collection");
    service
        .add_collection_item(
            &collection.id,
            CollectionItemRef {
                module: "geography".into(),
                entity_type: "city".into(),
                entity_id: "luoyang".into(),
                title: "洛阳".into(),
                note: Some("Sprint6-luoyang-note-unique: 记录安史之乱与洛阳的关系".into()),
            },
            1001,
        )
        .expect("save collection item");

    let matches = service
        .search_collection_items("Sprint6-luoyang-note-unique")
        .expect("search collection notes");

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].collection_title, "唐代洛阳");
    assert_eq!(matches[0].item.module, "geography");
    assert_eq!(matches[0].item.entity_type, "city");
    assert_eq!(matches[0].item.entity_id, "luoyang");
    assert_eq!(
        matches[0].item.note.as_deref(),
        Some("Sprint6-luoyang-note-unique: 记录安史之乱与洛阳的关系")
    );
}

#[test]
fn collection_note_updates_are_normalized_and_keep_the_saved_source_reference() {
    let service = LearningService::new(Arc::new(MockLearningStore::default()));
    let collection = service
        .create_collection("洛阳", None, &[], 1000)
        .expect("create collection");
    let item = service
        .add_collection_item(
            &collection.id,
            CollectionItemRef {
                module: "history".into(),
                entity_type: "event".into(),
                entity_id: "event-anlu-luoyang".into(),
                title: "洛阳失守".into(),
                note: Some("旧备注".into()),
            },
            1001,
        )
        .expect("save collection item");

    let updated = service
        .update_collection_item_note(&item.id, Some("  新备注  "), 1002)
        .expect("update note");
    assert_eq!(updated.entity_id, "event-anlu-luoyang");
    assert_eq!(updated.note.as_deref(), Some("新备注"));

    let cleared = service
        .update_collection_item_note(&item.id, Some("  "), 1003)
        .expect("clear note");
    assert_eq!(cleared.note, None);

    let oversized = "x".repeat(4_001);
    assert!(matches!(
        service.update_collection_item_note(&item.id, Some(&oversized), 1004),
        Err(LearningPortError::InvalidArgument(_))
    ));
    assert!(matches!(
        service.update_collection_item_note(&item.id, Some("invalid\u{0000}note"), 1005),
        Err(LearningPortError::InvalidArgument(_))
    ));
}

#[test]
fn test_learning_service_full_flow() {
    let mock_store = Arc::new(MockLearningStore::default());
    let service = LearningService::new(mock_store);

    let event = LearningEvent {
        id: "evt_1".to_string(),
        module: "history".to_string(),
        action: LearningAction::Study,
        entity_type: "story".to_string(),
        entity_id: "silk_road".to_string(),
        entity_title: Some("丝绸之路的历史变迁".to_string()),
        timestamp: 1000,
        duration_ms: Some(5000),
        metadata: serde_json::json!({}),
        source: None,
    };

    let progress = service.record_event(&event, 1000).expect("record event");
    assert_eq!(progress.entity_key, "history:story:silk_road");
    assert_eq!(progress.study_count, 1);
    assert_eq!(progress.status, LearningStatus::Learning);

    // 学过一次**不会**自动生成复习卡：自动卡的题干与答案同义反复，复习无意义。
    assert!(
        service
            .get_review_queue(Some("history"), 90_000, 10)
            .expect("queue")
            .is_empty(),
        "仅学习过不应自动建卡（同义反复的卡会挤占真实复习队列）"
    );

    // 显式加入复习后才出现，且由调用方给出真实问答。
    service
        .upsert_review_card(&UniversalReviewCard {
            id: "card_silk_road".into(),
            module: "history".into(),
            entity_id: "silk_road".into(),
            entity_type: "story".into(),
            card_type: ReviewCardType::Recall,
            prompt: "丝绸之路连接了哪些地区？".into(),
            answer: "长安 — 中亚 — 欧洲（地中海）".into(),
            options: None,
            hint: None,
            context: None,
            source_title: None,
            user_note: None,
            due_at: 86_401,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: 1000,
        })
        .expect("add card");

    let queue = service
        .get_review_queue(Some("history"), 90_000, 10)
        .expect("queue");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].card.prompt, "丝绸之路连接了哪些地区？");

    // Submit review rating
    let outcome = service
        .submit_review(&queue[0].card.id, ReviewRating::Good, 90_000)
        .expect("review");
    assert!(outcome.interval_days >= 1.0);

    // Check today dashboard
    let today = service
        .get_today_dashboard(90_000)
        .expect("today dashboard");
    assert_eq!(today.continue_items.len(), 1);

    // Knowledge graph
    let graph = service
        .get_knowledge_graph(Some("history:story:silk_road"), 1)
        .expect("graph");
    assert_eq!(graph.root_id.as_deref(), Some("history:story:silk_road"));
    assert!(!graph.nodes.is_empty());
}

#[test]
fn knowledge_card_save_is_source_stable_and_preserves_srs_when_edited() {
    let store = Arc::new(MockLearningStore::default());
    let service = LearningService::new(store.clone());
    let mut draft = KnowledgeReviewCardDraft {
        module: "history".into(),
        entity_type: "event".into(),
        entity_id: "event-anlu-luoyang".into(),
        source_title: "洛阳失守".into(),
        prompt: "叛军何时攻陷洛阳？".into(),
        answer: "公元 756 年。".into(),
        user_note: Some("复核具体月份".into()),
    };

    let saved = service
        .save_knowledge_review_card(&draft, 1_800_000_000)
        .expect("save user-authored card");
    assert_eq!(saved.id, "knowledge:history:event:event-anlu-luoyang");
    assert_eq!(saved.card_type, ReviewCardType::Qa);
    assert_eq!(saved.due_at, 1_800_000_000);
    assert_eq!(saved.user_note.as_deref(), Some("复核具体月份"));

    // Simulate an actual review; content edits must not reset scheduling or progress.
    let mut reviewed = saved.clone();
    reviewed.interval_days = 3.0;
    reviewed.ease = 2.7;
    reviewed.repetition_count = 2;
    reviewed.lapses = 1;
    reviewed.mastery_score = 62.5;
    reviewed.last_reviewed_at = Some(1_800_000_111);
    reviewed.due_at = 1_800_259_200;
    store
        .upsert_review_card(&reviewed)
        .expect("record existing schedule in fake store");

    draft.prompt = "洛阳失守发生在哪一年？".into();
    draft.answer = "756 年。".into();
    draft.user_note = Some("来源条目仍保留原 note".into());
    let edited = service
        .save_knowledge_review_card(&draft, 1_900_000_000)
        .expect("edit card content");
    assert_eq!(edited.id, saved.id);
    assert_eq!(edited.prompt, draft.prompt);
    assert_eq!(edited.answer, draft.answer);
    assert_eq!(edited.user_note, draft.user_note);
    assert_eq!(edited.due_at, reviewed.due_at);
    assert_eq!(edited.interval_days, reviewed.interval_days);
    assert_eq!(edited.ease, reviewed.ease);
    assert_eq!(edited.repetition_count, reviewed.repetition_count);
    assert_eq!(edited.lapses, reviewed.lapses);
    assert_eq!(edited.last_reviewed_at, reviewed.last_reviewed_at);
    assert_eq!(
        store.cards.lock().unwrap().len(),
        1,
        "same source upserts one card"
    );
}

#[test]
fn knowledge_card_rejects_unsupported_sources_and_invalid_content() {
    let service = LearningService::new(Arc::new(MockLearningStore::default()));
    let mut draft = KnowledgeReviewCardDraft {
        module: "news".into(),
        entity_type: "article".into(),
        entity_id: "article-1".into(),
        source_title: "Article".into(),
        prompt: "Question".into(),
        answer: "Answer".into(),
        user_note: None,
    };
    assert!(matches!(
        service.save_knowledge_review_card(&draft, 100),
        Err(LearningPortError::InvalidArgument(_))
    ));

    draft.module = "history".into();
    draft.entity_type = "work".into();
    assert!(matches!(
        service.save_knowledge_review_card(&draft, 100),
        Err(LearningPortError::InvalidArgument(_))
    ));
    draft.module = "geography".into();
    draft.entity_type = "city:event".into();
    assert!(matches!(
        service.save_knowledge_review_card(&draft, 100),
        Err(LearningPortError::InvalidArgument(_))
    ));
    draft.module = "history".into();
    draft.entity_type = "event".into();
    draft.prompt = "  ".into();
    assert!(matches!(
        service.save_knowledge_review_card(&draft, 100),
        Err(LearningPortError::InvalidArgument(_))
    ));
    assert!(matches!(
        service.delete_knowledge_review_card("language:word:private"),
        Err(LearningPortError::InvalidArgument(_))
    ));
}
