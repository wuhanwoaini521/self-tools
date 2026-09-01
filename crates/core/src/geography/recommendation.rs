use std::collections::HashSet;

use super::{GeoEntity, GeoEntityType, GeoRecommendation, RecommendationKind};

pub struct GeoRecommendationService;

impl GeoRecommendationService {
    #[must_use]
    pub fn recommend(
        entities: &[GeoEntity],
        recent_ids: &[String],
        cursor: u64,
    ) -> Option<GeoRecommendation> {
        let recent: HashSet<&str> = recent_ids.iter().map(String::as_str).collect();
        let mut candidates: Vec<&GeoEntity> = entities
            .iter()
            .filter(|entity| !recent.contains(entity.id.as_str()))
            .collect();
        if candidates.is_empty() {
            candidates = entities.iter().collect();
        }
        candidates.sort_by_key(|entity| {
            (
                recommendation_rank(entity.entity_type),
                entity.name.as_str(),
            )
        });
        let entity = candidates.get((cursor as usize) % candidates.len())?;
        let (question, kind) = question_for(entity);
        Some(GeoRecommendation {
            kind,
            title: entity.name.clone(),
            question,
            entity_id: entity.id.clone(),
            tags: entity
                .properties
                .iter()
                .take(3)
                .map(|property| property.label.clone())
                .collect(),
        })
    }
}

fn recommendation_rank(entity_type: GeoEntityType) -> u8 {
    match entity_type {
        GeoEntityType::Plateau => 0,
        GeoEntityType::Basin => 1,
        GeoEntityType::MountainRange => 2,
        GeoEntityType::River => 3,
        GeoEntityType::City => 4,
        GeoEntityType::Country => 5,
        _ => 4,
    }
}

fn question_for(entity: &GeoEntity) -> (String, RecommendationKind) {
    let question = match entity.entity_type {
        GeoEntityType::Plateau => format!("为什么{}会拥有如此极端的海拔？", entity.name),
        GeoEntityType::Basin => format!("为什么{}会形成独特的气候与人口格局？", entity.name),
        GeoEntityType::River => format!("{}如何影响沿线的城市与人口分布？", entity.name),
        GeoEntityType::City => format!("为什么{}会形成今天这样的城市结构？", entity.name),
        GeoEntityType::Country => format!("为什么{}的人口与城市集中在这些地方？", entity.name),
        _ => format!("{}为什么值得从地理角度探索？", entity.name),
    };
    (question, RecommendationKind::Question)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{GeoEntity, GeoProperty};

    fn entity(id: &str, entity_type: GeoEntityType) -> GeoEntity {
        GeoEntity {
            id: id.into(),
            entity_type,
            name: id.into(),
            name_en: None,
            aliases: vec![],
            coordinates: None,
            geometry: None,
            parent_id: None,
            properties: vec![GeoProperty {
                key: "terrain".into(),
                label: "地形".into(),
                value: "测试".into(),
                unit: None,
                source_ids: vec![],
            }],
            summary: String::new(),
            source_ids: vec![],
        }
    }

    #[test]
    fn excludes_recent_entity_when_alternative_exists() {
        let entities = vec![
            entity("四川盆地", GeoEntityType::Basin),
            entity("长江", GeoEntityType::River),
        ];
        let result = GeoRecommendationService::recommend(&entities, &["四川盆地".into()], 0)
            .expect("recommendation");
        assert_eq!(result.entity_id, "长江");
        assert_eq!(result.kind, RecommendationKind::Question);
    }
}
