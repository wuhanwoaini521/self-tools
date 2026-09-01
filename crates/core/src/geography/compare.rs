use thiserror::Error;

use super::{CompareMetric, GeoCompareView, GeoEntity};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GeoCompareError {
    #[error("cannot compare an entity with itself")]
    SameEntity,
    #[error("unsupported entity types: {0} and {1}")]
    UnsupportedTypes(String, String),
}

pub fn compare_entities(
    left: &GeoEntity,
    right: &GeoEntity,
) -> Result<GeoCompareView, GeoCompareError> {
    if left.id == right.id {
        return Err(GeoCompareError::SameEntity);
    }
    if left.entity_type != right.entity_type {
        return Err(GeoCompareError::UnsupportedTypes(
            left.entity_type.label().into(),
            right.entity_type.label().into(),
        ));
    }
    let metrics = [
        ("面积", "area", None),
        ("人口", "population", None),
        ("人口密度", "population_density", None),
        ("纬度", "latitude", None),
        ("海拔", "elevation", None),
        ("地形", "terrain", None),
        ("气候", "climate", None),
        ("主要城市", "major_cities", None),
    ]
    .into_iter()
    .map(|(label, key, unit)| CompareMetric {
        label: label.into(),
        key: key.into(),
        left: property(left, key).map(|value| value.0),
        right: property(right, key).map(|value| value.0),
        unit: property(left, key)
            .and_then(|value| value.1)
            .or_else(|| property(right, key).and_then(|value| value.1))
            .or(unit.map(str::to_string)),
    })
    .collect();
    let explanation = format!(
        "{}与{}的差异，来自它们所处的地形、纬度和水系条件；先对照事实，再沿关系继续探索。",
        left.name, right.name
    );
    Ok(GeoCompareView {
        left: left.clone(),
        right: right.clone(),
        metrics,
        explanation,
    })
}

fn property(entity: &GeoEntity, key: &str) -> Option<(String, Option<String>)> {
    entity
        .properties
        .iter()
        .find(|item| item.key == key)
        .map(|item| (item.value.clone(), item.unit.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{GeoEntityType, GeoProperty};

    fn entity(id: &str, population: Option<&str>) -> GeoEntity {
        GeoEntity {
            id: id.into(),
            entity_type: GeoEntityType::Country,
            name: id.into(),
            name_en: None,
            aliases: vec![],
            coordinates: None,
            geometry: None,
            parent_id: None,
            properties: population
                .map(|value| {
                    vec![GeoProperty {
                        key: "population".into(),
                        label: "人口".into(),
                        value: value.into(),
                        unit: Some("人".into()),
                        source_ids: vec![],
                    }]
                })
                .unwrap_or_default(),
            summary: String::new(),
            source_ids: vec![],
        }
    }

    #[test]
    fn preserves_missing_values_and_units() {
        let view =
            compare_entities(&entity("甲", Some("10")), &entity("乙", None)).expect("compare");
        let metric = view
            .metrics
            .iter()
            .find(|metric| metric.key == "population")
            .expect("population metric");
        assert_eq!(metric.left.as_deref(), Some("10"));
        assert_eq!(metric.right, None);
        assert_eq!(metric.unit.as_deref(), Some("人"));
    }

    #[test]
    fn rejects_same_entity() {
        assert_eq!(
            compare_entities(&entity("甲", None), &entity("甲", None)),
            Err(GeoCompareError::SameEntity)
        );
    }
}
