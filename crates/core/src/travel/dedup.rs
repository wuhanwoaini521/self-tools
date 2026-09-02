//! 去重与事实验证（需求 #十 / 多来源验证）。
//!
//! - `dedup_search_results`：按归一化 URL 去重搜索结果；
//! - `dedup_facts`：同 (category, subject, value) 只保留置信度最高的一条；
//! - `verify_facts`：按 (category, subject) 分组做多源验证——权威来源优先 + 多数一致，
//!   输出 `VerifiedFact`（含 confidence / verified_sources / primary_source / has_conflict）。

use std::collections::HashMap;

use crate::travel::guide::VerifiedFact;
use crate::travel::model::{ContentState, FactCategory, SearchResult, SourceLevel, TravelFact};
use crate::travel::quality::normalize_entity_name;

/// 归一化 URL 用于去重：去 fragment、去尾部斜杠、清掉常见跟踪参数、小写 scheme/host。
#[must_use]
pub fn normalize_url(url: &str) -> String {
    let trimmed = url.trim();
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => (String::new(), trimmed),
    };
    let without_fragment = rest.split('#').next().unwrap_or_default();
    let (path, query) = match without_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (without_fragment, None),
    };
    let path = path.trim_end_matches('/');
    let mut filtered_query = String::new();
    if let Some(query) = query {
        let kept: Vec<&str> = query
            .split('&')
            .filter(|param| {
                let key = param
                    .split('=')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                !(key.starts_with("utm_")
                    || key == "spm"
                    || key == "from"
                    || key == "share"
                    || key == "source")
            })
            .collect();
        if !kept.is_empty() {
            filtered_query = format!("?{}", kept.join("&"));
        }
    }
    let host_lower = path
        .split('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path_part = path.split_once('/').map_or("", |(_, rest)| rest);
    format!("{scheme}://{host_lower}/{path_part}{filtered_query}")
}

/// 搜索结果去重：优先按归一化 URL，无 URL 时按标题。
#[must_use]
pub fn dedup_search_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    results
        .into_iter()
        .filter(|result| {
            let key = if result.url.is_empty() {
                format!("title:{}", result.title.trim().to_lowercase())
            } else {
                normalize_url(&result.url)
            };
            seen.insert(key)
        })
        .collect()
}

/// 事实正文去重键。
fn fact_key(fact: &TravelFact) -> (FactCategoryKey, String, String) {
    (
        fact.category,
        normalize_text(&fact.subject),
        normalize_text(&fact.value),
    )
}

/// 归一化文本用于比较（去空白、小写化）。
fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

type FactCategoryKey = crate::travel::model::FactCategory;

/// 同键事实只保留置信度最高的一条（多来源重复提取的合并）。
#[must_use]
pub fn dedup_facts(facts: Vec<TravelFact>) -> Vec<TravelFact> {
    let mut best: HashMap<(FactCategoryKey, String, String), TravelFact> = HashMap::new();
    for fact in facts {
        let key = fact_key(&fact);
        match best.get(&key) {
            Some(existing) if existing.confidence >= fact.confidence => {}
            _ => {
                best.insert(key, fact);
            }
        }
    }
    best.into_values().collect()
}

/// 合并结构化 POI 的实体别名。网页事实没有稳定 ID 或坐标时不做激进合并，
/// 避免把同名餐厅、不同分店和同名景点错误归为一处。
#[must_use]
pub fn dedup_entity_facts(facts: Vec<TravelFact>) -> Vec<TravelFact> {
    let mut unique: Vec<TravelFact> = Vec::with_capacity(facts.len());
    for fact in facts {
        let duplicate = unique
            .iter()
            .position(|existing| same_entity(existing, &fact));
        if let Some(index) = duplicate {
            if fact.confidence > unique[index].confidence
                || (unique[index].coordinates.is_none() && fact.coordinates.is_some())
            {
                unique[index] = fact;
            }
        } else {
            unique.push(fact);
        }
    }
    unique
}

fn same_entity(left: &TravelFact, right: &TravelFact) -> bool {
    if left.category != right.category {
        return false;
    }
    if let (Some(left_id), Some(right_id)) = (&left.poi_id, &right.poi_id) {
        return !left_id.is_empty() && left_id == right_id;
    }
    // 只对景点使用“别名 + 坐标”规则；餐饮分店必须保守处理。
    if left.category != FactCategory::Attraction {
        return false;
    }
    let left_name = normalize_entity_name(&left.subject);
    let right_name = normalize_entity_name(&right.subject);
    if left_name.is_empty() || left_name != right_name {
        return false;
    }
    match (&left.coordinates, &right.coordinates) {
        (Some(a), Some(b)) => {
            ((a.longitude - b.longitude).powi(2) + (a.latitude - b.latitude).powi(2)).sqrt() < 0.04
        }
        _ => false,
    }
}

/// 按 (category, subject) 分组验证，产生 `VerifiedFact`（需求 #十）。
///
/// 解析规则：
/// - 候选值 = 规范化后的 value 文本；每个候选的得分 =
///   `来源数量 × 1.0 + 该候选下最高权威权重 × 2.0`（官方权重最高，天然优先）；
/// - `verified_sources` = 支持所选值的来源数量；
/// - `confidence`：≥3 个一致来源，或 ≥2 且其中含官方/权威 → "high"；
///   2 个一致来源 → "medium"；否则 "low"；
/// - `has_conflict`：存在 ≥2 个不同候选值（需求 #十：「⚠ 信息存在多个版本」）。
#[must_use]
pub fn verify_facts(
    facts: &[TravelFact],
    source_levels: &HashMap<String, SourceLevel>,
) -> Vec<VerifiedFact> {
    verify_facts_with_states(facts, source_levels, &HashMap::new())
}

/// 带抓取状态的事实验证。仅 SnippetOnly 支持的开放、票价、预约、交通硬事实
/// 不能被标记为 verified；它们仍可作为候选线索保留。
#[must_use]
pub fn verify_facts_with_states(
    facts: &[TravelFact],
    source_levels: &HashMap<String, SourceLevel>,
    source_states: &HashMap<String, ContentState>,
) -> Vec<VerifiedFact> {
    let mut grouped: HashMap<(FactCategoryKey, String), Vec<&TravelFact>> = HashMap::new();
    for fact in facts {
        grouped
            .entry((fact.category, normalize_text(&fact.subject)))
            .or_default()
            .push(fact);
    }

    let mut verified: Vec<VerifiedFact> = grouped
        .into_iter()
        .filter(|(_, members)| !members.is_empty())
        .map(|((category, subject_normalized), members)| {
            let mut candidates: HashMap<String, (usize, f32)> = HashMap::new();
            let mut candidate_examples: HashMap<String, String> = HashMap::new();
            for fact in &members {
                let value = normalize_text(&fact.value);
                let weight = source_levels
                    .get(&fact.source_id)
                    .copied()
                    .unwrap_or(SourceLevel::C)
                    .weight();
                let entry = candidates.entry(value.clone()).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 = entry.1.max(weight);
                candidate_examples
                    .entry(value)
                    .or_insert_with(|| fact.value.clone());
            }
            let mut ranked: Vec<(&String, (usize, f32))> = candidates
                .iter()
                .map(|(value, stats)| (value, *stats))
                .collect();
            ranked.sort_by(|a, b| {
                let score_a = a.1.0 as f32 + a.1.1 * 2.0;
                let score_b = b.1.0 as f32 + b.1.1 * 2.0;
                score_b
                    .partial_cmp(&score_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let (chosen_norm, (count, max_weight)) = ranked[0];
            let chosen_value = candidate_examples
                .get(chosen_norm)
                .cloned()
                .unwrap_or_else(|| chosen_norm.clone());
            let primary = if count >= 2 {
                if max_weight >= 0.8 {
                    "official"
                } else {
                    "multiple"
                }
            } else if max_weight >= 0.8 {
                classify_primary(max_weight)
            } else {
                "single"
            };
            let hard_fact = matches!(
                category,
                FactCategory::OpeningHours
                    | FactCategory::Ticket
                    | FactCategory::Reservation
                    | FactCategory::Transport
            );
            let verified = !hard_fact
                || members.iter().any(|fact| {
                    source_states
                        .get(&fact.source_id)
                        .is_none_or(|state| *state != ContentState::SnippetOnly)
                });
            let confidence = match (count, max_weight) {
                (n, w) if n >= 3 || (n >= 2 && w >= 0.8) => "high",
                (2, _) => "medium",
                _ => "low",
            };
            let candidates_list = candidates.keys().cloned().collect::<Vec<_>>();
            VerifiedFact {
                category,
                subject: subject_normalized,
                value: chosen_value,
                confidence: confidence.to_string(),
                verified_sources: count,
                primary_source: primary.to_string(),
                has_conflict: candidates_list.len() > 1,
                verified,
                candidates: candidates_list,
            }
        })
        .collect();

    verified.sort_by(|a, b| {
        a.category
            .json_name()
            .cmp(b.category.json_name())
            .then_with(|| a.subject.cmp(&b.subject))
    });
    verified
}

fn classify_primary(max_weight: f32) -> &'static str {
    if max_weight >= 1.0 {
        "official"
    } else if max_weight >= 0.85 {
        "authoritative"
    } else if max_weight >= 0.72 {
        "travel_platform"
    } else {
        "ugc"
    }
}

impl crate::travel::model::FactCategory {
    /// 展示用类别名（用于排序 / 前端分组）。
    #[must_use]
    pub fn json_name(self) -> &'static str {
        match self {
            Self::CityOverview => "city_overview",
            Self::Attraction => "attraction",
            Self::Food => "food",
            Self::Transport => "transport",
            Self::Accommodation => "accommodation",
            Self::Weather => "weather",
            Self::OpeningHours => "opening_hours",
            Self::Ticket => "ticket",
            Self::Reservation => "reservation",
            Self::Activity => "activity",
            Self::TravelTip => "travel_tip",
            Self::Warning => "warning",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{dedup_facts, dedup_search_results, normalize_url, verify_facts};
    use crate::travel::model::{ContentState, FactCategory, SearchResult, SourceLevel, TravelFact};

    fn result(url: &str, title: &str) -> SearchResult {
        SearchResult {
            title: title.to_string(),
            url: url.to_string(),
            snippet: "snippet".to_string(),
            provider: "mock".to_string(),
            published_at: None,
            fetched_at: 1_700_000_000,
        }
    }

    fn fact(
        category: FactCategory,
        subject: &str,
        value: &str,
        source: &str,
        conf: f32,
    ) -> TravelFact {
        TravelFact {
            category,
            subject: subject.to_string(),
            value: value.to_string(),
            source_id: source.to_string(),
            confidence: conf,
            fetched_at: 1_700_000_000,
            coordinates: None,
            poi_id: None,
            area: None,
            address: None,
        }
    }

    #[test]
    fn normalize_url_strips_tracking_and_fragments() {
        assert_eq!(
            normalize_url("https://example.com/a/?utm_source=x#top"),
            "https://example.com/a"
        );
        assert_eq!(
            normalize_url("HTTP://Example.com/a?spm=1&page=2"),
            "http://example.com/a?page=2"
        );
    }

    #[test]
    fn search_results_dedup_by_normalized_url() {
        let results = vec![
            result("https://example.com/a?utm_source=baidu", "A"),
            result("https://example.com/a", "A again"),
            result("https://example.com/b", "B"),
        ];
        let deduped = dedup_search_results(results);
        assert_eq!(deduped.len(), 2);
        assert!(deduped.iter().any(|r| r.title == "B"));
    }

    #[test]
    fn facts_dedup_keep_highest_confidence() {
        let facts = vec![
            fact(FactCategory::OpeningHours, "西湖", "07:00-18:00", "s1", 0.5),
            fact(FactCategory::OpeningHours, "西湖", "07:00-18:00", "s2", 0.9),
        ];
        let deduped = dedup_facts(facts);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].source_id, "s2");
    }

    #[test]
    fn verify_facts_prefers_official_when_no_majority() {
        let facts = vec![
            fact(
                FactCategory::OpeningHours,
                "灵隐寺",
                "07:00-18:00",
                "https://ugc.example",
                0.8,
            ),
            fact(
                FactCategory::OpeningHours,
                "灵隐寺",
                "08:00-17:00",
                "https://gov.example",
                0.6,
            ),
        ];
        let levels = std::collections::HashMap::from([
            ("https://ugc.example".to_string(), SourceLevel::C),
            ("https://gov.example".to_string(), SourceLevel::S),
        ]);
        let verified = verify_facts(&facts, &levels);
        assert_eq!(verified.len(), 1);
        let v = &verified[0];
        assert_eq!(v.value, "08:00-17:00"); // 官方优先
        assert_eq!(v.primary_source, "official");
        assert!(v.has_conflict, "两个不同版本应标记冲突");
        assert_eq!(v.confidence, "low"); // 各自只有 1 个来源
    }

    #[test]
    fn verify_facts_majority_wins_without_official() {
        let facts = vec![
            fact(FactCategory::Ticket, "西湖", "免费", "s1", 0.7),
            fact(FactCategory::Ticket, "西湖", "免费", "s2", 0.6),
            fact(FactCategory::Ticket, "西湖", "收费", "s3", 0.9),
        ];
        let verified = verify_facts(&facts, &std::collections::HashMap::new());
        assert_eq!(verified[0].value, "免费");
        assert_eq!(verified[0].verified_sources, 2);
        assert_eq!(verified[0].confidence, "medium");
        assert!(verified[0].has_conflict);
    }

    #[test]
    fn verify_facts_high_confidence_with_official_agreement() {
        let facts = vec![
            fact(
                FactCategory::OpeningHours,
                "岳庙",
                "07:30-18:00",
                "https://gz.gov.example",
                0.5,
            ),
            fact(
                FactCategory::OpeningHours,
                "岳庙",
                "07:30-18:00",
                "https://b1.example",
                0.8,
            ),
            fact(
                FactCategory::OpeningHours,
                "岳庙",
                "07:30-18:00",
                "https://b2.example",
                0.7,
            ),
        ];
        let levels = std::collections::HashMap::from([
            ("https://gz.gov.example".to_string(), SourceLevel::S),
            ("https://b1.example".to_string(), SourceLevel::B),
            ("https://b2.example".to_string(), SourceLevel::B),
        ]);
        let verified = verify_facts(&facts, &levels);
        assert_eq!(verified[0].confidence, "high");
        assert_eq!(verified[0].verified_sources, 3);
        assert_eq!(verified[0].primary_source, "official");
        assert!(!verified[0].has_conflict);
    }

    #[test]
    fn verify_facts_groups_different_subjects() {
        let facts = vec![
            fact(FactCategory::OpeningHours, "A馆", "09:00-17:00", "s1", 0.5),
            fact(FactCategory::OpeningHours, "B馆", "10:00-18:00", "s2", 0.5),
        ];
        let verified = verify_facts(&facts, &std::collections::HashMap::new());
        assert_eq!(verified.len(), 2);
    }

    #[test]
    fn snippet_only_hard_fact_is_not_verified() {
        let facts = vec![fact(
            FactCategory::OpeningHours,
            "红河谷",
            "08:00-17:00",
            "https://search.example/result",
            0.9,
        )];
        let levels = std::collections::HashMap::from([(
            "https://search.example/result".to_string(),
            SourceLevel::B,
        )]);
        let states = std::collections::HashMap::from([(
            "https://search.example/result".to_string(),
            ContentState::SnippetOnly,
        )]);
        let verified = super::verify_facts_with_states(&facts, &levels, &states);
        assert!(!verified[0].verified);
        assert_eq!(verified[0].confidence, "low");
    }
}
