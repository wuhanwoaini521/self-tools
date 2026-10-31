//! Curated, stable-ID links across Geography and History datasets.
//!
//! This registry deliberately does not infer links from display-name equality. Add a
//! mapping only after reviewing both source records, and preserve source IDs on each side.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrosswalkStatus {
    Reviewed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrosswalkRelationType {
    HistoricalEventPlaceAnchor,
    ModernUrbanAreaAnchor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CrosswalkTimeScope {
    /// Time covered by the reviewed event evidence, not the full validity period of a place.
    pub start_year: i32,
    pub end_year: i32,
    pub precision: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CrosswalkProvenance {
    pub source_id: &'static str,
    pub record_id: &'static str,
    pub uri: Option<&'static str>,
    pub note: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EntityCrosswalk {
    pub id: &'static str,
    pub geography_entity_id: &'static str,
    pub history_place_id: &'static str,
    pub relation_type: CrosswalkRelationType,
    pub status: CrosswalkStatus,
    /// Crosswalk confidence is not inferred from an event-place link score.
    pub confidence_percent: Option<u8>,
    pub evidence_time_scope: Option<CrosswalkTimeScope>,
    pub geography_source_ids: &'static [&'static str],
    pub history_source_ids: &'static [&'static str],
    pub provenance: &'static [CrosswalkProvenance],
}

/// Explicit reviewed links only; equality of labels never creates a relation.
/// Historical period and place granularity remain attached to each reviewed record.
pub const ENTITY_CROSSWALKS: &[EntityCrosswalk] = &[
    EntityCrosswalk {
        id: "geography:chengdu|history:cbdb-place-406333",
        geography_entity_id: "chengdu",
        history_place_id: "cbdb-place-406333",
        relation_type: CrosswalkRelationType::HistoricalEventPlaceAnchor,
        status: CrosswalkStatus::Reviewed,
        confidence_percent: None,
        evidence_time_scope: Some(CrosswalkTimeScope {
            start_year: 756,
            end_year: 756,
            precision: "year",
        }),
        geography_source_ids: &["geography-seed"],
        history_source_ids: &["source-curated-backbone-v1"],
        provenance: &[
            CrosswalkProvenance {
                source_id: "source-curated-backbone-v1",
                record_id: "event-place-806a475743df317548dd51e2",
                uri: Some(
                    "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml",
                ),
                note: "已审核地点记录：成都 canonical ID 被用作‘蜀地’事件地点锚点；该关联不代表整个古代行政区与现代城市等同。",
            },
            CrosswalkProvenance {
                source_id: "source-curated-backbone-v1",
                record_id: "event-anlu-xuanzong-shu",
                uri: Some(
                    "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml",
                ),
                note: "已审核事件记录将事件时间标为 756 年；时间范围描述事件证据，不是 History place ID。",
            },
        ],
    },
    EntityCrosswalk {
        id: "geography:luoyang|history:cbdb-place-14693",
        geography_entity_id: "luoyang",
        history_place_id: "cbdb-place-14693",
        relation_type: CrosswalkRelationType::ModernUrbanAreaAnchor,
        status: CrosswalkStatus::Reviewed,
        confidence_percent: None,
        evidence_time_scope: Some(CrosswalkTimeScope {
            start_year: 756,
            end_year: 756,
            precision: "year",
        }),
        geography_source_ids: &["geography-wikidata-q187136"],
        history_source_ids: &["source-curated-backbone-v1"],
        provenance: &[
            CrosswalkProvenance {
                source_id: "geography-wikidata-q187136",
                record_id: "Q187136",
                uri: Some("https://www.wikidata.org/entity/Q187136"),
                note: "现代洛阳市 Geography 实体；Wikidata 标注河南省行政归属及代表坐标。",
            },
            CrosswalkProvenance {
                source_id: "source-curated-backbone-v1",
                record_id: "event-place-ef7ba1621307cfb31edd377c",
                uri: Some(
                    "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-luoyang.yml",
                ),
                note: "已审核地点记录：cbdb-place-14693 用于洛阳失守事件，地点角色为‘城市’，记录说明选择唐代有效年代的洛阳 canonical ID。",
            },
            CrosswalkProvenance {
                source_id: "source-curated-backbone-v1",
                record_id: "event-anlu-luoyang",
                uri: Some(
                    "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-luoyang.yml",
                ),
                note: "已审核事件记录将事件时间标为 756 年；不将事件 ID 当作年份，也不推断该地点 ID 的完整有效年代。",
            },
        ],
    },
];

fn matching_crosswalks(
    registry: &[EntityCrosswalk],
    matches: impl Fn(&EntityCrosswalk) -> bool,
) -> Vec<&EntityCrosswalk> {
    registry.iter().filter(|link| matches(link)).collect()
}

#[must_use]
pub fn crosswalks_for_geography(geography_entity_id: &str) -> Vec<&'static EntityCrosswalk> {
    matching_crosswalks(ENTITY_CROSSWALKS, |link| {
        link.geography_entity_id == geography_entity_id
    })
}

fn unique_crosswalk(matches: Vec<&EntityCrosswalk>) -> Option<&EntityCrosswalk> {
    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

#[must_use]
pub fn crosswalk_for_geography(geography_entity_id: &str) -> Option<&'static EntityCrosswalk> {
    unique_crosswalk(crosswalks_for_geography(geography_entity_id))
}

#[must_use]
pub fn crosswalk_for_history_place(history_place_id: &str) -> Option<&'static EntityCrosswalk> {
    unique_crosswalk(matching_crosswalks(ENTITY_CROSSWALKS, |link| {
        link.history_place_id == history_place_id
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        CrosswalkStatus, ENTITY_CROSSWALKS, EntityCrosswalk, crosswalk_for_geography,
        crosswalk_for_history_place, crosswalks_for_geography, matching_crosswalks,
        unique_crosswalk,
    };

    #[test]
    fn reviewed_city_links_are_bidirectional_and_keep_provenance_and_time_scope() {
        let chengdu = crosswalk_for_geography("chengdu").expect("reviewed geography link");
        let chengdu_by_history =
            crosswalk_for_history_place("cbdb-place-406333").expect("reviewed history link");
        assert_eq!(chengdu.id, "geography:chengdu|history:cbdb-place-406333");
        assert_eq!(chengdu.id, chengdu_by_history.id);
        assert_eq!(chengdu.status, CrosswalkStatus::Reviewed);
        assert_eq!(chengdu.evidence_time_scope.unwrap().start_year, 756);
        assert!(
            chengdu
                .provenance
                .iter()
                .any(|row| row.record_id == "event-anlu-xuanzong-shu")
        );

        let luoyang = crosswalk_for_geography("luoyang").expect("reviewed Luoyang link");
        assert_eq!(luoyang.history_place_id, "cbdb-place-14693");
        assert_eq!(
            crosswalk_for_history_place("cbdb-place-14693")
                .unwrap()
                .geography_entity_id,
            "luoyang"
        );
        assert!(
            luoyang
                .provenance
                .iter()
                .any(|row| row.record_id == "Q187136")
        );
    }

    #[test]
    fn unmatched_and_ambiguous_ids_never_fall_back_to_name_matching() {
        assert!(crosswalk_for_geography("上海").is_none());
        assert!(crosswalk_for_geography("shanghai").is_none());
        assert!(crosswalk_for_geography("yangtze").is_none());
        assert!(crosswalk_for_history_place("unknown-place-id").is_none());
        // This same-name Luoyang candidate is explicitly rejected in the pipeline due
        // to a date conflict; only the reviewed Tang-period canonical ID is mapped.
        assert!(crosswalk_for_history_place("cbdb-place-100409").is_none());
        assert!(crosswalk_for_history_place("洛阳").is_none());
    }

    #[test]
    fn registry_lookup_supports_multiple_historical_places_for_one_city_but_rejects_ambiguous_reverse_links()
     {
        let additional = EntityCrosswalk {
            id: "fixture:luoyang|history:district-place",
            history_place_id: "fixture-historical-district",
            ..ENTITY_CROSSWALKS[1].clone()
        };
        let fixture_registry = [ENTITY_CROSSWALKS[1].clone(), additional];
        assert_eq!(
            matching_crosswalks(&fixture_registry, |link| link.geography_entity_id
                == "luoyang")
            .len(),
            2
        );
        assert_eq!(crosswalks_for_geography("luoyang").len(), 1);

        let duplicate_history = EntityCrosswalk {
            id: "fixture:other-city|history:luoyang-place",
            geography_entity_id: "other-city",
            ..ENTITY_CROSSWALKS[1].clone()
        };
        let ambiguous_registry = [ENTITY_CROSSWALKS[1].clone(), duplicate_history];
        let ambiguous_matches = matching_crosswalks(&ambiguous_registry, |link| {
            link.history_place_id == "cbdb-place-14693"
        });
        assert_eq!(ambiguous_matches.len(), 2);
        assert!(unique_crosswalk(ambiguous_matches).is_none());
    }
}
