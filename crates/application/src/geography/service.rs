use devtoolbox_core::geography::{
    GeoEntityDetail, GeoEntityType, GeoRecommendationService, GeoRelationView, GeoSearchGroup,
    GeographyHome,
};

use super::ports::{GeographyPortError, GeographyQueryPort};
use crate::ApplicationError;

pub struct GeographyService {
    port: Box<dyn GeographyQueryPort>,
}

impl GeographyService {
    #[must_use]
    pub fn new(port: Box<dyn GeographyQueryPort>) -> Self {
        Self { port }
    }

    pub fn home(&self, cursor: u64) -> Result<GeographyHome, ApplicationError> {
        let entities = self.port.all_entities().map_err(geo_error)?;
        let recent_ids = self.port.recent_ids(8).map_err(geo_error)?;
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
        let favorite_ids = self.port.favorite_ids().map_err(geo_error)?;
        let (map_points, map_lines) = self.port.map_snapshot().map_err(geo_error)?;
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
        let entities = self
            .port
            .search(query, entity_type, limit)
            .map_err(geo_error)?;
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
        let Some(entity) = self.port.entity(id).map_err(geo_error)? else {
            return Ok(None);
        };
        self.port.record_view(id).map_err(geo_error)?;
        let relations = self.port.relations_for(id).map_err(geo_error)?;
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
            if let Some(related) = self.port.entity(related_id).map_err(geo_error)? {
                relation_views.push(GeoRelationView {
                    relation,
                    entity: related,
                });
            }
        }
        let sources = self
            .port
            .sources(&all_sources.into_iter().collect::<Vec<_>>())
            .map_err(geo_error)?;
        let favorite = self
            .port
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

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, ApplicationError> {
        self.port.toggle_favorite(id).map_err(geo_error)
    }
}

fn geo_error(source: GeographyPortError) -> ApplicationError {
    ApplicationError::Geography { message: source.0 }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use devtoolbox_core::geography::{
        CoordinateSystem, GeoCoordinate, GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint,
        GeoRelation, GeoRelationKind, GeoSource,
    };

    use super::*;

    #[derive(Default)]
    struct FakePortData {
        entities: Vec<GeoEntity>,
        relations: Vec<GeoRelation>,
        favorite_ids: Vec<String>,
        map_points: Vec<GeoMapPoint>,
        map_lines: Vec<GeoMapLine>,
        sources: Vec<GeoSource>,
        /// true 时所有查询返回 GeographyPortError。
        fail: bool,
    }

    fn entity(id: &str, entity_type: GeoEntityType) -> GeoEntity {
        GeoEntity {
            id: id.into(),
            entity_type,
            name: id.into(),
            name_en: None,
            aliases: Vec::new(),
            coordinates: None,
            geometry: None,
            parent_id: None,
            properties: Vec::new(),
            summary: String::new(),
            source_ids: Vec::new(),
        }
    }

    fn fake_entities() -> Vec<GeoEntity> {
        [
            ("tibetan-plateau", GeoEntityType::Plateau),
            ("yangtze", GeoEntityType::River),
            ("sichuan-basin", GeoEntityType::Basin),
            ("japan", GeoEntityType::Country),
            ("chengdu", GeoEntityType::City),
            ("chongqing", GeoEntityType::City),
        ]
        .into_iter()
        .map(|(id, kind)| entity(id, kind))
        .collect()
    }

    impl GeographyQueryPort for Rc<RefCell<FakePortData>> {
        fn all_entities(&self) -> Result<Vec<GeoEntity>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.entities.clone())
        }
        fn recent_ids(&self, _limit: i64) -> Result<Vec<String>, GeographyPortError> {
            Ok(Vec::new())
        }
        fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.favorite_ids.clone())
        }
        fn map_snapshot(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok((data.map_points.clone(), data.map_lines.clone()))
        }
        fn search(
            &self,
            query: &str,
            entity_type: Option<GeoEntityType>,
            limit: usize,
        ) -> Result<Vec<GeoEntity>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data
                .entities
                .iter()
                .filter(|entity| {
                    entity_type.is_none_or(|kind| entity.entity_type == kind)
                        && entity.name.contains(query)
                })
                .take(limit)
                .cloned()
                .collect())
        }
        fn entity(&self, id: &str) -> Result<Option<GeoEntity>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.entities.iter().find(|entity| entity.id == id).cloned())
        }
        fn record_view(&self, _id: &str) -> Result<(), GeographyPortError> {
            Ok(())
        }
        fn relations_for(&self, _id: &str) -> Result<Vec<GeoRelation>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.relations.clone())
        }
        fn sources(&self, _ids: &[String]) -> Result<Vec<GeoSource>, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.sources.clone())
        }
        fn toggle_favorite(&self, _id: &str) -> Result<bool, GeographyPortError> {
            let data = self.borrow();
            if data.fail {
                return Err(GeographyPortError("boom".into()));
            }
            Ok(data.favorite_ids.contains(&_id.to_owned()))
        }
    }

    fn service(fake: Rc<RefCell<FakePortData>>) -> GeographyService {
        GeographyService::new(Box::new(fake))
    }

    #[test]
    fn home_has_question_map_and_featured_content() {
        let fake = Rc::new(RefCell::new(FakePortData {
            entities: fake_entities(),
            map_points: vec![GeoMapPoint {
                entity_id: "yangtze".into(),
                name: "长江".into(),
                entity_type: GeoEntityType::River,
                coordinate: GeoCoordinate {
                    system: CoordinateSystem::WGS84,
                    latitude: 30.0,
                    longitude: 110.0,
                },
            }],
            ..Default::default()
        }));
        let service = service(Rc::clone(&fake));
        let home = service.home(0).expect("home");
        assert!(!home.recommendation.question.is_empty());
        assert!(!home.featured.is_empty());
        assert!(!home.map_points.is_empty());
    }

    #[test]
    fn detail_is_composed_from_relations() {
        let fake = Rc::new(RefCell::new(FakePortData {
            entities: fake_entities(),
            relations: vec![GeoRelation {
                from_id: "yangtze".into(),
                to_id: "chongqing".into(),
                kind: GeoRelationKind::FlowsThrough,
                note: None,
                source_ids: Vec::new(),
            }],
            ..Default::default()
        }));
        let service = service(Rc::clone(&fake));
        let detail = service.detail("yangtze").expect("detail").expect("found");
        assert!(
            detail
                .relations
                .iter()
                .any(|item| item.entity.id == "chongqing")
        );
    }

    #[test]
    fn port_failure_maps_to_geography_application_error() {
        let fake = Rc::new(RefCell::new(FakePortData {
            fail: true,
            ..Default::default()
        }));
        let service = service(Rc::clone(&fake));
        let error = service.home(0).expect_err("port failure propagates");
        match error {
            ApplicationError::Geography { message } => assert_eq!(message, "boom"),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
