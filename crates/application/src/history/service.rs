//! History Explorer 用例编排：React 仅消费 DTO，不承担搜索、推荐和关系拼装。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use devtoolbox_core::history::{
    HistoryDetailView, HistoryNode, HistoryNodeKind, HistoryRecommendationService,
    HistoryRelationView, HistorySearchGroup,
};
use devtoolbox_infrastructure::HistoryStore;

use crate::ApplicationError;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoryHome {
    pub timeline: Vec<devtoolbox_core::history::HistoryPeriod>,
    pub recommendation: HistoryNode,
    pub discoveries: Vec<HistoryNode>,
    pub recent: Vec<HistoryNode>,
    pub favorite_ids: Vec<String>,
}

pub struct HistoryService {
    store: Arc<Mutex<HistoryStore>>,
}

impl HistoryService {
    #[must_use]
    pub fn new(store: Arc<Mutex<HistoryStore>>) -> Self {
        Self { store }
    }

    pub fn home(&self, cursor: u64) -> Result<HistoryHome, ApplicationError> {
        let store = self.store.lock().expect("history store poisoned");
        let nodes = store.all_nodes().map_err(history_error)?;
        let recent_ids = store.recent_ids(8).map_err(history_error)?;
        let recommendation =
            HistoryRecommendationService::recommend(&nodes, &recent_ids, cursor)
                .ok_or_else(|| ApplicationError::HistoryData("离线历史数据为空".to_string()))?;
        let recent = store.recent_nodes(8).map_err(history_error)?;
        let favorite_ids = store.favorite_ids().map_err(history_error)?;
        let discoveries = choose_discoveries(&nodes, &recommendation.id, cursor);
        Ok(HistoryHome {
            timeline: store.periods().map_err(history_error)?,
            recommendation,
            discoveries,
            recent,
            favorite_ids,
        })
    }

    pub fn search(&self, query: &str) -> Result<Vec<HistorySearchGroup>, ApplicationError> {
        let nodes = self
            .store
            .lock()
            .expect("history store poisoned")
            .search(query)
            .map_err(history_error)?;
        let kinds = [
            HistoryNodeKind::Person,
            HistoryNodeKind::Event,
            HistoryNodeKind::Dynasty,
            HistoryNodeKind::Place,
            HistoryNodeKind::War,
            HistoryNodeKind::Institution,
            HistoryNodeKind::Artifact,
            HistoryNodeKind::Culture,
        ];
        Ok(kinds
            .into_iter()
            .filter_map(|kind| {
                let items: Vec<_> = nodes
                    .iter()
                    .filter(|node| node.kind == kind)
                    .cloned()
                    .collect();
                (!items.is_empty()).then_some(HistorySearchGroup { kind, items })
            })
            .collect())
    }

    pub fn detail(&self, id: &str) -> Result<Option<HistoryDetailView>, ApplicationError> {
        let store = self.store.lock().expect("history store poisoned");
        let Some(document) = store.document(id).map_err(history_error)? else {
            return Ok(None);
        };
        store.record_view(id).map_err(history_error)?;
        let nodes = store.all_nodes().map_err(history_error)?;
        let relations = store
            .relations_for(id)
            .map_err(history_error)?
            .into_iter()
            .filter_map(|relation| {
                let related_id = if relation.from_id == id {
                    &relation.to_id
                } else {
                    &relation.from_id
                };
                nodes
                    .iter()
                    .find(|node| node.id == *related_id)
                    .cloned()
                    .map(|node| HistoryRelationView { relation, node })
            })
            .collect();
        let sources = store
            .sources(&document.node.source_ids)
            .map_err(history_error)?;
        Ok(Some(HistoryDetailView {
            document,
            relations,
            sources,
        }))
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, ApplicationError> {
        self.store
            .lock()
            .expect("history store poisoned")
            .toggle_favorite(id)
            .map_err(history_error)
    }
}

fn choose_discoveries(
    nodes: &[HistoryNode],
    recommendation_id: &str,
    cursor: u64,
) -> Vec<HistoryNode> {
    let mut sorted: Vec<_> = nodes
        .iter()
        .filter(|node| node.id != recommendation_id)
        .cloned()
        .collect();
    sorted.sort_by(|left, right| left.title.cmp(&right.title));
    if sorted.is_empty() {
        return sorted;
    }
    let offset = (cursor as usize) % sorted.len();
    sorted.rotate_left(offset);
    let mut kinds = std::collections::HashSet::new();
    let mut selected: Vec<_> = sorted
        .iter()
        .filter(|node| kinds.insert(node.kind))
        .take(6)
        .cloned()
        .collect();
    if selected.len() < 6 {
        let selected_ids: std::collections::HashSet<String> =
            selected.iter().map(|node| node.id.clone()).collect();
        let remaining = 6 - selected.len();
        selected.extend(
            sorted
                .into_iter()
                .filter(|node| !selected_ids.contains(&node.id))
                .take(remaining),
        );
    }
    selected
}

fn history_error(source: devtoolbox_infrastructure::InfrastructureError) -> ApplicationError {
    ApplicationError::History { source }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn detail_exposes_causal_and_place_relations() {
        let dir = tempdir().expect("tempdir");
        let store = HistoryStore::open(dir.path().join("history.db")).expect("store");
        let service = HistoryService::new(Arc::new(Mutex::new(store)));
        let detail = service
            .detail("an-lushan-rebellion")
            .expect("detail")
            .expect("found");
        assert!(
            detail
                .relations
                .iter()
                .any(|item| item.node.id == "changan")
        );
        assert!(!detail.sources.is_empty());
    }

    #[test]
    fn person_event_and_event_narrative_are_explorable() {
        let dir = tempdir().expect("tempdir");
        let store = HistoryStore::open(dir.path().join("history.db")).expect("store");
        let service = HistoryService::new(Arc::new(Mutex::new(store)));
        let person = service.detail("li-shimin").expect("person").expect("found");
        assert!(
            person
                .relations
                .iter()
                .any(|item| item.node.id == "xuanwu-gate")
        );
        let event = service
            .detail("an-lushan-rebellion")
            .expect("event")
            .expect("found");
        let devtoolbox_core::history::HistoryDetail::Event(narrative) = event.document.detail
        else {
            panic!("event detail");
        };
        assert!(!narrative.background.is_empty(), "event cause/background");
        assert!(!narrative.results.is_empty(), "event consequence/result");
    }
}
