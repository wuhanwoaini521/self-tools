//! 共享端口适配器：平台学习存储（SQLite）→ `LearningStorePort`。
//!
//! 与 [`super::language`] 同理：原先只存在于 `apps/desktop`，服务端无法复用。

use std::sync::Arc;

use devtoolbox_application::learning::{LearningPortError, LearningStorePort};
use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, LearningEvent, LearningProgress,
    LearningStatus, ReviewQueueItem, ReviewQueueStats, ReviewRating, ReviewScheduleOutcome,
    UniversalReviewCard,
};
use parking_lot::Mutex;

use crate::learning::LearningStore;

/// 把 `LearningStore`（`config/learning.db`）包装成 application 端口。
pub struct LearningStoreAdapter {
    store: Arc<Mutex<LearningStore>>,
}

impl LearningStoreAdapter {
    pub fn new(store: Arc<Mutex<LearningStore>>) -> Self {
        Self { store }
    }
}

impl LearningStorePort for LearningStoreAdapter {
    fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError> {
        self.store
            .lock()
            .record_event(event)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, LearningPortError> {
        self.store
            .lock()
            .get_progress(entity_key)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        self.store
            .lock()
            .list_progress(module_filter, status_filter, limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .upsert_review_card(card)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, LearningPortError> {
        self.store
            .lock()
            .get_review_card(card_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError> {
        self.store
            .lock()
            .list_due_reviews(module_filter, now, limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        self.store
            .lock()
            .get_review_stats(now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError> {
        self.store
            .lock()
            .record_review_outcome(card_id, rating, now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<Collection, LearningPortError> {
        self.store
            .lock()
            .create_collection(title, description, tags, now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        self.store
            .lock()
            .list_collections()
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn add_collection_item(
        &self,
        collection_id: &str,
        entity: CollectionItemRef,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        self.store
            .lock()
            .add_collection_item(collection_id, entity, now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItem>, LearningPortError> {
        self.store
            .lock()
            .list_collection_items(collection_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .remove_collection_item(item_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .delete_collection(collection_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, LearningPortError> {
        self.store
            .lock()
            .get_continue_items(limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, LearningPortError> {
        self.store
            .lock()
            .count_topics_studied_today(day_start_ts)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_average_mastery(&self) -> Result<f64, LearningPortError> {
        self.store
            .lock()
            .get_average_mastery()
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }
}
