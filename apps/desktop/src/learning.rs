//! Desktop Learning OS 适配器与 Tauri 命令。

use std::sync::{Arc, Mutex};
use tauri::State;

use devtoolbox_application::learning::{LearningPortError, LearningService, LearningStorePort};
use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, ExploreRecommendation,
    GraphNeighborhood, LearningEvent, LearningProgress, LearningStatus, ReviewQueueItem,
    ReviewQueueStats, ReviewRating, ReviewScheduleOutcome, TodayDashboardData, UniversalReviewCard,
};
use devtoolbox_infrastructure::LearningStore;

use crate::{AppState, CommandError};

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
            .unwrap()
            .record_event(event)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, LearningPortError> {
        self.store
            .lock()
            .unwrap()
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
            .unwrap()
            .list_progress(module_filter, status_filter, limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .upsert_review_card(card)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, LearningPortError> {
        self.store
            .lock()
            .unwrap()
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
            .unwrap()
            .list_due_reviews(module_filter, now, limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        self.store
            .lock()
            .unwrap()
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
            .unwrap()
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
            .unwrap()
            .create_collection(title, description, tags, now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        self.store
            .lock()
            .unwrap()
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
            .unwrap()
            .add_collection_item(collection_id, entity, now)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItem>, LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .list_collection_items(collection_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .remove_collection_item(item_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .delete_collection(collection_id)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .get_continue_items(limit)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .count_topics_studied_today(day_start_ts)
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }

    fn get_average_mastery(&self) -> Result<f64, LearningPortError> {
        self.store
            .lock()
            .unwrap()
            .get_average_mastery()
            .map_err(|e| LearningPortError::Store(e.to_string()))
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn learning_record_event(
    state: State<'_, AppState>,
    event: LearningEvent,
) -> Result<LearningProgress, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service.record_event(&event, now).map_err(|e| CommandError {
        code: "learning_error",
        message: e.to_string(),
    })
}

#[tauri::command]
pub async fn learning_get_progress(
    state: State<'_, AppState>,
    entity_key: String,
) -> Result<Option<LearningProgress>, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service.get_progress(&entity_key).map_err(|e| CommandError {
        code: "learning_error",
        message: e.to_string(),
    })
}

#[tauri::command]
pub async fn learning_list_progress(
    state: State<'_, AppState>,
    module_filter: Option<String>,
    status_filter: Option<LearningStatus>,
    limit: Option<usize>,
) -> Result<Vec<LearningProgress>, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .list_progress(module_filter.as_deref(), status_filter, limit.unwrap_or(50))
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_get_today(
    state: State<'_, AppState>,
) -> Result<TodayDashboardData, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service.get_today_dashboard(now).map_err(|e| CommandError {
        code: "learning_error",
        message: e.to_string(),
    })
}

#[tauri::command]
pub async fn learning_get_review_queue(
    state: State<'_, AppState>,
    module_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<ReviewQueueItem>, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service
        .get_review_queue(module_filter.as_deref(), now, limit.unwrap_or(30))
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_get_review_stats(
    state: State<'_, AppState>,
) -> Result<ReviewQueueStats, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service.get_review_stats(now).map_err(|e| CommandError {
        code: "learning_error",
        message: e.to_string(),
    })
}

#[tauri::command]
pub async fn learning_submit_review(
    state: State<'_, AppState>,
    card_id: String,
    rating: ReviewRating,
) -> Result<ReviewScheduleOutcome, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service
        .submit_review(&card_id, rating, now)
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_get_graph(
    state: State<'_, AppState>,
    root_id: Option<String>,
    hops: Option<u32>,
) -> Result<GraphNeighborhood, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .get_knowledge_graph(root_id.as_deref(), hops.unwrap_or(1))
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_get_explore(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<ExploreRecommendation>, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .get_explore_recommendations(limit.unwrap_or(6))
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_list_collections(
    state: State<'_, AppState>,
) -> Result<Vec<Collection>, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service.list_collections().map_err(|e| CommandError {
        code: "learning_error",
        message: e.to_string(),
    })
}

#[tauri::command]
pub async fn learning_create_collection(
    state: State<'_, AppState>,
    title: String,
    description: Option<String>,
    tags: Vec<String>,
) -> Result<Collection, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service
        .create_collection(&title, description.as_deref(), &tags, now)
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_add_collection_item(
    state: State<'_, AppState>,
    collection_id: String,
    module: String,
    entity_type: String,
    entity_id: String,
    title: String,
    note: Option<String>,
) -> Result<CollectionItem, CommandError> {
    let now = devtoolbox_infrastructure::now_unix();
    let service = LearningService::new(state.learning_store.clone());
    service
        .add_collection_item(
            &collection_id,
            CollectionItemRef {
                module,
                entity_type,
                entity_id,
                title,
                note,
            },
            now,
        )
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_list_collection_items(
    state: State<'_, AppState>,
    collection_id: String,
) -> Result<Vec<CollectionItem>, CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .list_collection_items(&collection_id)
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_remove_collection_item(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<(), CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .remove_collection_item(&item_id)
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}

#[tauri::command]
pub async fn learning_delete_collection(
    state: State<'_, AppState>,
    collection_id: String,
) -> Result<(), CommandError> {
    let service = LearningService::new(state.learning_store.clone());
    service
        .delete_collection(&collection_id)
        .map_err(|e| CommandError {
            code: "learning_error",
            message: e.to_string(),
        })
}
