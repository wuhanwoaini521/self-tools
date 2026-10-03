//! Desktop Learning OS 适配器与 Tauri 命令。

use tauri::State;

use devtoolbox_application::learning::LearningService;
use devtoolbox_core::learning::{
    Collection, CollectionItem, CollectionItemRef, ExploreRecommendation, GraphNeighborhood,
    LearningEvent, LearningProgress, LearningStatus, ReviewQueueItem, ReviewQueueStats,
    ReviewRating, ReviewScheduleOutcome, TodayDashboardData,
};

use crate::{AppState, CommandError};

pub use devtoolbox_infrastructure::ports::LearningStoreAdapter;

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
