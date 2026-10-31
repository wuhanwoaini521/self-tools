//! Learning OS 端口定义（Port 契约）。

use thiserror::Error;

use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, LearningEvent, LearningProgress,
    LearningStatus, ReviewQueueItem, ReviewQueueStats, ReviewRating, ReviewScheduleOutcome,
    UniversalReviewCard,
};

#[derive(Debug, Error)]
pub enum LearningPortError {
    #[error("store error: {0}")]
    Store(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

pub trait LearningStorePort: Send + Sync {
    fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError>;
    fn get_progress(&self, entity_key: &str)
    -> Result<Option<LearningProgress>, LearningPortError>;
    fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError>;

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError>;
    /// Save user-authored content while preserving any existing SRS schedule.
    /// Production storage overrides this with a transaction; the default supports test ports.
    fn save_knowledge_review_card(
        &self,
        card: &UniversalReviewCard,
    ) -> Result<UniversalReviewCard, LearningPortError> {
        let mut content = card.clone();
        if let Some(existing) = self.get_review_card(&card.id)? {
            if (
                existing.module.as_str(),
                existing.entity_type.as_str(),
                existing.entity_id.as_str(),
            ) != (
                card.module.as_str(),
                card.entity_type.as_str(),
                card.entity_id.as_str(),
            ) {
                return Err(LearningPortError::InvalidArgument(
                    "review card source identity cannot be changed".to_string(),
                ));
            }
            content.due_at = existing.due_at;
            content.interval_days = existing.interval_days;
            content.ease = existing.ease;
            content.mastery_score = existing.mastery_score;
            content.repetition_count = existing.repetition_count;
            content.lapses = existing.lapses;
            content.last_reviewed_at = existing.last_reviewed_at;
            content.created_at = existing.created_at;
        }
        self.upsert_review_card(&content)?;
        self.get_review_card(&content.id)?
            .ok_or_else(|| LearningPortError::NotFound(content.id.clone()))
    }
    fn delete_knowledge_review_card(&self, card_id: &str) -> Result<bool, LearningPortError> {
        let _ = card_id;
        Err(LearningPortError::Store(
            "review card deletion is not supported by this store".to_string(),
        ))
    }
    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, LearningPortError>;
    fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError>;
    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError>;
    fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError>;
    /// Record exactly one outcome for a client-generated idempotency key.
    fn record_review_outcome_once(
        &self,
        card_id: &str,
        rating: ReviewRating,
        request_id: &str,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError> {
        let _ = request_id;
        self.record_review_outcome(card_id, rating, now)
    }

    fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<Collection, LearningPortError>;
    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError>;
    fn add_collection_item(
        &self,
        collection_id: &str,
        entity: CollectionItemRef,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError>;
    fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItem>, LearningPortError>;
    fn update_collection_item_note(
        &self,
        item_id: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        let _ = (item_id, note, now);
        Err(LearningPortError::Store(
            "collection note editing is not supported by this store".to_string(),
        ))
    }
    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError>;
    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError>;

    fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, LearningPortError>;
    fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, LearningPortError>;
    /// Return distinct eligible learning-activity day starts (UTC timestamps) for the requested
    /// UTC offset, newest first. Timestamps after `now` are excluded.
    fn list_study_activity_day_starts(
        &self,
        utc_offset_seconds: i64,
        now: i64,
    ) -> Result<Vec<i64>, LearningPortError> {
        let _ = (utc_offset_seconds, now);
        Ok(Vec::new())
    }
    fn get_average_mastery(&self) -> Result<f64, LearningPortError>;
}
