//! Application use cases that connect canonical Geography and History entities.
//!
//! The crosswalk is keyed by stable dataset IDs. Empty results are returned as empty
//! collections; no name-based fallback or fabricated event is introduced.

use serde::Serialize;

use crate::error::ApplicationError;
use crate::geography::{GeoEntityDetail, GeographyService};
use crate::history::{HistorySemanticEventDetail, HistoryService};

#[derive(Debug, Serialize)]
pub struct LocationHistoryContext {
    pub geography: GeoEntityDetail,
    /// First relation retained for clients that used the v1 singular field.
    pub crosswalk: Option<devtoolbox_core::exploration::EntityCrosswalk>,
    /// A modern entity can be related to multiple historical places; consumers should
    /// render each explicit relation rather than choosing one by name or order.
    pub crosswalks: Vec<devtoolbox_core::exploration::EntityCrosswalk>,
    pub events: Vec<HistorySemanticEventDetail>,
}

pub struct ExplorationService {
    geography: std::sync::Arc<GeographyService>,
    history: std::sync::Arc<HistoryService>,
}

impl ExplorationService {
    #[must_use]
    pub fn new(
        geography: std::sync::Arc<GeographyService>,
        history: std::sync::Arc<HistoryService>,
    ) -> Self {
        Self { geography, history }
    }

    /// Fetch one Geography entity and any reviewed historical records joined by the
    /// curated stable-ID crosswalk. `Ok(None)` means the Geography ID does not exist.
    pub fn location_history(
        &self,
        geography_entity_id: &str,
    ) -> Result<Option<LocationHistoryContext>, ApplicationError> {
        let Some(geography) = self.geography.detail(geography_entity_id)? else {
            return Ok(None);
        };
        let crosswalks: Vec<_> =
            devtoolbox_core::exploration::crosswalks_for_geography(geography_entity_id)
                .into_iter()
                .cloned()
                .collect();
        let crosswalk = (crosswalks.len() == 1).then(|| crosswalks[0].clone());
        let mut events = Vec::new();
        for link in &crosswalks {
            events.extend(self.history.events_for_place(link.history_place_id)?);
        }
        let mut seen = std::collections::HashSet::new();
        events.retain(|event| seen.insert(event.event.id.clone()));
        Ok(Some(LocationHistoryContext {
            geography,
            crosswalk,
            crosswalks,
            events,
        }))
    }

    /// Reverse lookup for History place IDs. Only reviewed explicit links are returned.
    pub fn geography_for_history_place(
        &self,
        history_place_id: &str,
    ) -> Result<Option<GeoEntityDetail>, ApplicationError> {
        let Some(link) =
            devtoolbox_core::exploration::crosswalk_for_history_place(history_place_id)
        else {
            return Ok(None);
        };
        self.geography.detail(link.geography_entity_id)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crosswalk_lookups_keep_unknown_entities_unmatched() {
        assert!(devtoolbox_core::exploration::crosswalk_for_geography("dalian").is_none());
        assert!(
            devtoolbox_core::exploration::crosswalk_for_history_place("unknown-place").is_none()
        );
    }
}
