use std::sync::{Arc, Mutex};

use devtoolbox_core::geography::{
    GeoCompareView, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint,
    GeoRecommendationService, GeoRelationView, GeoSearchGroup, GeographyHome, compare_entities,
};
use devtoolbox_infrastructure::GeographyStore;

use crate::ApplicationError;

pub struct GeographyService {
    store: Arc<Mutex<GeographyStore>>,
}

impl GeographyService {
    #[must_use]
    pub fn new(store: Arc<Mutex<GeographyStore>>) -> Self {
        Self { store }
    }

    pub fn home(&self, cursor: u64) -> Result<GeographyHome, ApplicationError> {
        let store = self.store.lock().expect("geography store poisoned");
        let entities = store.all_entities().map_err(geo_error)?;
        let recent_ids = store.recent_ids(8).map_err(geo_error)?;
        let recommendation = GeoRecommendationService::recommend(&entities, &recent_ids, cursor)
            .ok_or_else(|| ApplicationError::GeographyData("离线地理数据为空".into()))?;
        let featured = [
            "tibetan-plateau",
            "yangtze",
            "sichuan-basin",
            "japan",
            "chengdu",
        ]
        .into_iter()
        .filter_map(|id| entities.iter().find(|entity| entity.id == id).cloned())
        .collect();
        let recent = recent_ids
            .into_iter()
            .filter_map(|id| entities.iter().find(|entity| entity.id == id).cloned())
            .collect();
        let favorite_ids = store.favorite_ids().map_err(geo_error)?;
        let (map_points, map_lines) = store.map_snapshot().map_err(geo_error)?;
        Ok(GeographyHome {
            recommendation,
            featured,
            recent,
            favorite_ids,
            map_points,
            map_lines,
        })
    }

    pub fn search(
        &self,
        query: &str,
        entity_type: Option<GeoEntityType>,
        limit: usize,
    ) -> Result<Vec<GeoSearchGroup>, ApplicationError> {
        let store = self.store.lock().expect("geography store poisoned");
        let entities = store.search(query, entity_type, limit).map_err(geo_error)?;
        let order = [
            GeoEntityType::World,
            GeoEntityType::Country,
            GeoEntityType::Province,
            GeoEntityType::Region,
            GeoEntityType::City,
            GeoEntityType::River,
            GeoEntityType::MountainRange,
            GeoEntityType::Plateau,
            GeoEntityType::Basin,
        ];
        Ok(order
            .into_iter()
            .filter_map(|kind| {
                let items = entities
                    .iter()
                    .filter(|entity| entity.entity_type == kind)
                    .cloned()
                    .collect::<Vec<_>>();
                (!items.is_empty()).then_some(GeoSearchGroup {
                    entity_type: kind,
                    items,
                })
            })
            .collect())
    }

    pub fn detail(&self, id: &str) -> Result<Option<GeoEntityDetail>, ApplicationError> {
        let store = self.store.lock().expect("geography store poisoned");
        let Some(entity) = store.entity(id).map_err(geo_error)? else {
            return Ok(None);
        };
        store.record_view(id).map_err(geo_error)?;
        let relations = store.relations_for(id).map_err(geo_error)?;
        let all_sources = entity
            .source_ids
            .iter()
            .cloned()
            .chain(
                relations
                    .iter()
                    .flat_map(|relation| relation.source_ids.iter().cloned()),
            )
            .collect::<std::collections::HashSet<_>>();
        let mut relation_views = Vec::new();
        for relation in relations {
            let related_id = if relation.from_id == id {
                &relation.to_id
            } else {
                &relation.from_id
            };
            if let Some(related) = store.entity(related_id).map_err(geo_error)? {
                relation_views.push(GeoRelationView {
                    relation,
                    entity: related,
                });
            }
        }
        let sources = store
            .sources(&all_sources.into_iter().collect::<Vec<_>>())
            .map_err(geo_error)?;
        let favorite = store
            .favorite_ids()
            .map_err(geo_error)?
            .iter()
            .any(|item| item == id);
        Ok(Some(GeoEntityDetail {
            entity,
            relations: relation_views,
            sources,
            favorite,
        }))
    }

    pub fn map(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), ApplicationError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .map_snapshot()
            .map_err(geo_error)
    }

    pub fn compare(
        &self,
        left_id: &str,
        right_id: &str,
    ) -> Result<GeoCompareView, ApplicationError> {
        let store = self.store.lock().expect("geography store poisoned");
        let left = store
            .entity(left_id)
            .map_err(geo_error)?
            .ok_or_else(|| ApplicationError::GeographyData(format!("未找到实体：{left_id}")))?;
        let right = store
            .entity(right_id)
            .map_err(geo_error)?
            .ok_or_else(|| ApplicationError::GeographyData(format!("未找到实体：{right_id}")))?;
        compare_entities(&left, &right)
            .map_err(|error| ApplicationError::GeographyCompare(error.to_string()))
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, ApplicationError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .toggle_favorite(id)
            .map_err(geo_error)
    }
}

fn geo_error(source: devtoolbox_infrastructure::InfrastructureError) -> ApplicationError {
    ApplicationError::Geography { source }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_has_question_map_and_featured_content() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = GeographyStore::open(directory.path().join("geography.db")).expect("store");
        let service = GeographyService::new(Arc::new(Mutex::new(store)));
        let home = service.home(0).expect("home");
        assert!(!home.recommendation.question.is_empty());
        assert!(!home.featured.is_empty());
        assert!(!home.map_points.is_empty());
    }

    #[test]
    fn detail_and_compare_are_composed_from_storage() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = GeographyStore::open(directory.path().join("geography.db")).expect("store");
        let service = GeographyService::new(Arc::new(Mutex::new(store)));
        let detail = service.detail("yangtze").expect("detail").expect("found");
        assert!(
            detail
                .relations
                .iter()
                .any(|item| item.entity.id == "chongqing")
        );
        let comparison = service.compare("china", "japan").expect("compare");
        assert!(!comparison.metrics.is_empty());
    }
}
