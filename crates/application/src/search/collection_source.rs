//! Global Search adapter for saved collection references.
//!
//! Collection items are stored as stable references to their original module entity;
//! their note is searchable, while navigation continues to target that source entity.

use std::sync::Arc;

use devtoolbox_core::search::{GlobalSearchHit, GlobalSearchQuery, SearchSource};

use crate::learning::LearningService;

pub struct CollectionSearchPort {
    learning: Arc<LearningService>,
}

impl CollectionSearchPort {
    #[must_use]
    pub fn new(learning: Arc<LearningService>) -> Self {
        Self { learning }
    }
}

impl super::GlobalSearchPort for CollectionSearchPort {
    fn source(&self) -> SearchSource {
        SearchSource::Collections
    }

    fn search(&self, query: &GlobalSearchQuery) -> Result<Vec<GlobalSearchHit>, String> {
        let needle = query.trimmed().to_lowercase();
        let matches = self
            .learning
            .search_collection_items(&needle)
            .map_err(|error| error.to_string())?;

        let mut hits = matches
            .into_iter()
            .map(|matched| {
                let item = matched.item;
                let note = item.note.as_deref().unwrap_or("");
                let snippet = if note.is_empty() {
                    item.title.as_str()
                } else {
                    note
                };
                let score = if item.title.to_lowercase().contains(&needle) {
                    0.95
                } else if note.to_lowercase().contains(&needle) {
                    0.85
                } else {
                    0.65
                };
                GlobalSearchHit::new(
                    SearchSource::Collections,
                    "collection_item",
                    item.title.clone(),
                    format!("{} · {snippet}", matched.collection_title),
                    serde_json::json!({
                        "module": item.module,
                        "kind": item.entity_type,
                        "id": item.entity_id,
                        "collection_id": item.collection_id,
                        "collection_item_id": item.id,
                    }),
                    score,
                )
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(hits)
    }
}
