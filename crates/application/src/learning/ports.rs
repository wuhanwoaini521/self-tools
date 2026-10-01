//! Learning OS 端口定义（Port 契约）。

use thiserror::Error;

use devtoolbox_core::learning::{
    Collection, CollectionItem, ContinueItem, LearningEvent, LearningProgress, LearningStatus,
    ReviewQueueItem, ReviewQueueStats, ReviewRating, ReviewScheduleOutcome, UniversalReviewCard,
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
    fn get_progress(&self, entity_key: &str) -> Result<Option<LearningProgress>, LearningPortError>;
    fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError>;

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError>;
    fn get_review_card(&self, card_id: &str) -> Result<Option<UniversalReviewCard>, LearningPortError>;
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
        module: &str,
        entity_type: &str,
        entity_id: &str,
        title: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError>;
    fn list_collection_items(&self, collection_id: &str) -> Result<Vec<CollectionItem>, LearningPortError>;
    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError>;
    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError>;

    fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, LearningPortError>;
    fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, LearningPortError>;
    fn get_average_mastery(&self) -> Result<f64, LearningPortError>;
}
