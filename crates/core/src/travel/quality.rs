//! Travel Guide 输出质量门禁。
//!
//! 该层只做可解释、可回归的编辑规则：实体合并、数量上限、低信息文本清理和
//! 住宿粒度控制。它不补写事实，因此不会因为“填满 Schema”而制造幻觉。

use crate::travel::guide::{Attraction, CityGuide, Place};

const GENERIC_DESCRIPTIONS: &[&str] = &[
    "旅游必去景点推荐",
    "必去景点推荐",
    "热门景点推荐",
    "旅游攻略",
    "景点推荐",
];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QualityReport {
    pub duplicate_attractions_removed: usize,
    pub duplicate_restaurants_removed: usize,
    pub generic_descriptions_removed: usize,
    pub attractions_demoted: usize,
    pub restaurants_trimmed: usize,
    pub accommodation_areas_trimmed: usize,
}

/// 供事实匹配和实体归一化使用的稳定名称。
#[must_use]
pub fn normalize_entity_name(value: &str) -> String {
    let mut result = value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !"，。、·-—()（）[]【】/".contains(*ch))
        .collect::<String>();
    for suffix in [
        "风景名胜区",
        "国家森林公园",
        "旅游度假区",
        "风景区",
        "旅游区",
        "景区",
        "漂流",
    ] {
        if result.ends_with(suffix) && result.len() > suffix.len() {
            result.truncate(result.len() - suffix.len());
        }
    }
    result
}

fn same_attraction(left: &Attraction, right: &Attraction) -> bool {
    if let (Some(left_id), Some(right_id)) = (&left.poi_id, &right.poi_id) {
        if !left_id.is_empty() && left_id == right_id {
            return true;
        }
    }
    let left_name = left
        .normalized_name
        .clone()
        .unwrap_or_else(|| normalize_entity_name(&left.name));
    let right_name = right
        .normalized_name
        .clone()
        .unwrap_or_else(|| normalize_entity_name(&right.name));
    if left_name.is_empty() || left_name != right_name {
        return false;
    }
    match (&left.coordinates, &right.coordinates) {
        (Some(a), Some(b)) => {
            let distance =
                ((a.longitude - b.longitude).powi(2) + (a.latitude - b.latitude).powi(2)).sqrt();
            distance < 0.04
        }
        _ => true,
    }
}

fn merge_attraction(target: &mut Attraction, duplicate: Attraction) {
    if target.poi_id.is_none() {
        target.poi_id = duplicate.poi_id;
    }
    if target.id.is_none() {
        target.id = duplicate.id;
    }
    if target.normalized_name.is_none() {
        target.normalized_name = duplicate.normalized_name;
    }
    if target.area.is_none() {
        target.area = duplicate.area;
    }
    if target.coordinates.is_none() {
        target.coordinates = duplicate.coordinates;
    }
    if target.why_go.is_none() {
        target.why_go = duplicate.why_go.clone().or(duplicate.intro.clone());
    }
    if target.why_for_this_trip.is_none() {
        target.why_for_this_trip = duplicate.why_for_this_trip;
    }
    if target.suggested_duration.is_none() {
        target.suggested_duration = duplicate.suggested_duration;
    }
    target.best_for.extend(duplicate.best_for);
    target.best_for.sort();
    target.best_for.dedup();
    target.source_ids.extend(duplicate.source_ids);
    target.source_ids.sort();
    target.source_ids.dedup();
}

fn dedup_attractions(items: &mut Vec<Attraction>, report: &mut QualityReport) {
    let mut unique = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        if let Some(existing) = unique
            .iter_mut()
            .find(|candidate| same_attraction(candidate, &item))
        {
            merge_attraction(existing, item);
            report.duplicate_attractions_removed += 1;
        } else {
            unique.push(item);
        }
    }
    *items = unique;
}

fn same_restaurant(left: &Place, right: &Place) -> bool {
    // 餐厅不能仅凭相似名称合并：同品牌不同分店必须保留。
    matches!((&left.poi_id, &right.poi_id), (Some(a), Some(b)) if !a.is_empty() && a == b)
}

fn dedup_restaurants(items: &mut Vec<Place>, report: &mut QualityReport) {
    let mut unique = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        if unique
            .iter()
            .any(|candidate| same_restaurant(candidate, &item))
        {
            report.duplicate_restaurants_removed += 1;
        } else {
            unique.push(item);
        }
    }
    *items = unique;
}

fn is_generic(value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    GENERIC_DESCRIPTIONS
        .iter()
        .any(|phrase| normalized == *phrase || normalized.contains(phrase))
}

/// 在攻略进入存储和 UI 前执行。返回报告，便于进度日志和回归测试使用。
pub fn apply_quality_gate(guide: &mut CityGuide) -> QualityReport {
    let mut report = QualityReport::default();
    guide
        .attractions
        .retain(|item| !item.name.trim().is_empty());
    guide.top_picks.retain(|item| !item.name.trim().is_empty());
    guide
        .alternatives
        .retain(|item| !item.name.trim().is_empty());
    guide
        .restaurants
        .retain(|item| !item.name.trim().is_empty());
    guide.foods.retain(|item| !item.name.trim().is_empty());
    dedup_attractions(&mut guide.attractions, &mut report);
    dedup_attractions(&mut guide.top_picks, &mut report);
    dedup_attractions(&mut guide.alternatives, &mut report);
    dedup_restaurants(&mut guide.restaurants, &mut report);

    for item in guide
        .attractions
        .iter_mut()
        .chain(guide.top_picks.iter_mut())
        .chain(guide.alternatives.iter_mut())
    {
        if item.why_go.as_deref().is_some_and(is_generic) {
            item.why_go = None;
            report.generic_descriptions_removed += 1;
        }
        if item.intro.as_deref().is_some_and(is_generic) {
            item.intro = None;
            report.generic_descriptions_removed += 1;
        }
        if item.normalized_name.is_none() {
            item.normalized_name = Some(normalize_entity_name(&item.name));
        }
        if item.why_go.is_none() {
            item.why_go = item.intro.clone();
        }
    }

    // 只有具备“为什么去 / 建议时长 / 区域”的条目才进入主推荐；
    // 只有 POI 名称和坐标的结构化候选保留在备选，不伪装成编辑结论。
    let mut ready = Vec::with_capacity(guide.attractions.len());
    let mut incomplete = Vec::new();
    for item in guide.attractions.drain(..) {
        if item.why_go.is_some() && item.suggested_duration.is_some() && item.area.is_some() {
            ready.push(item);
        } else {
            incomplete.push(item);
        }
    }
    if !incomplete.is_empty() {
        report.attractions_demoted += incomplete.len();
        guide.alternatives.extend(incomplete);
    }
    guide.attractions = ready;

    let mut top_ready = Vec::with_capacity(guide.top_picks.len());
    let mut top_incomplete = Vec::new();
    for item in guide.top_picks.drain(..) {
        if item.why_go.is_some() && item.suggested_duration.is_some() && item.area.is_some() {
            top_ready.push(item);
        } else {
            top_incomplete.push(item);
        }
    }
    guide.top_picks = top_ready;
    guide.alternatives.extend(top_incomplete);

    let mut unique_alternatives = Vec::with_capacity(guide.alternatives.len());
    for item in guide.alternatives.drain(..) {
        if guide
            .attractions
            .iter()
            .any(|main| same_attraction(main, &item))
            || unique_alternatives
                .iter()
                .any(|existing| same_attraction(existing, &item))
        {
            continue;
        }
        unique_alternatives.push(item);
    }
    guide.alternatives = unique_alternatives;

    let max_attractions = (guide.meta.days.clamp(1, 7) as usize * 2 + 2).min(6);
    if guide.attractions.len() > max_attractions {
        let overflow = guide.attractions.split_off(max_attractions);
        report.attractions_demoted += overflow.len();
        guide.alternatives.extend(overflow);
    }
    if guide.top_picks.is_empty() {
        guide.top_picks = guide.attractions.clone();
    }
    guide.top_picks.truncate(max_attractions);
    let restaurants_before = guide.restaurants.len();
    guide.restaurants.truncate(5);
    let accommodation_before = guide.accommodation_areas.len();
    guide.accommodation_areas.retain(|area| {
        let text = format!("{} {}", area.name, area.area.clone().unwrap_or_default());
        !["酒店", "民宿", "公寓", "客栈"]
            .iter()
            .any(|word| text.contains(word))
            || text.contains('区')
            || text.contains("商圈")
    });
    guide.accommodation_areas.truncate(3);
    guide.stay_areas = guide.accommodation_areas.clone();
    report.restaurants_trimmed = restaurants_before.saturating_sub(guide.restaurants.len());
    report.accommodation_areas_trimmed =
        accommodation_before.saturating_sub(guide.accommodation_areas.len());
    report
}

#[cfg(test)]
mod tests {
    use super::{apply_quality_gate, normalize_entity_name};
    use crate::travel::{Attraction, CityGuide, GuideMeta, MapCoordinates};

    #[test]
    fn normalizes_scenic_area_aliases() {
        assert_eq!(normalize_entity_name("萨尔浒风景名胜区"), "萨尔浒");
        assert_eq!(normalize_entity_name("红河谷漂流"), "红河谷");
    }

    #[test]
    fn merges_duplicate_attractions_and_caps_two_day_guide() {
        let mut guide = CityGuide {
            meta: GuideMeta {
                days: 2,
                ..GuideMeta::default()
            },
            attractions: vec![
                Attraction {
                    name: "萨尔浒风景名胜区".into(),
                    why_go: Some("近郊山水景观，适合半日游。".into()),
                    suggested_duration: Some("3 小时".into()),
                    area: Some("东洲区".into()),
                    coordinates: Some(MapCoordinates {
                        longitude: 124.2,
                        latitude: 41.9,
                    }),
                    ..Attraction::default()
                },
                Attraction {
                    name: "萨尔浒风景区".into(),
                    why_go: Some("近郊山水景观，适合半日游。".into()),
                    suggested_duration: Some("3 小时".into()),
                    area: Some("东洲区".into()),
                    coordinates: Some(MapCoordinates {
                        longitude: 124.201,
                        latitude: 41.901,
                    }),
                    ..Attraction::default()
                },
            ],
            ..CityGuide::default()
        };
        let report = apply_quality_gate(&mut guide);
        assert_eq!(guide.attractions.len(), 1);
        assert_eq!(report.duplicate_attractions_removed, 1);
    }
}
