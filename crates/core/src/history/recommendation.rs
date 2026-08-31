use std::collections::{HashMap, HashSet};

use super::{HistoryNode, HistoryNodeKind};

/// V1 确定性推荐：避免连续重复、优先未浏览、兼顾时期与内容类型。
pub struct HistoryRecommendationService;

impl HistoryRecommendationService {
    #[must_use]
    pub fn recommend(
        nodes: &[HistoryNode],
        recent_ids: &[String],
        cursor: u64,
    ) -> Option<HistoryNode> {
        if nodes.is_empty() {
            return None;
        }
        let recent: HashSet<&str> = recent_ids.iter().map(String::as_str).collect();
        let recent_periods: HashSet<&str> = nodes
            .iter()
            .filter(|node| recent.contains(node.id.as_str()))
            .filter_map(|node| node.period_id.as_deref())
            .collect();
        let recent_kinds: HashSet<HistoryNodeKind> = nodes
            .iter()
            .filter(|node| recent.contains(node.id.as_str()))
            .map(|node| node.kind)
            .collect();
        let positions: HashMap<&str, usize> = recent_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        let mut candidates: Vec<&HistoryNode> = nodes.iter().collect();
        candidates.sort_by_key(|node| {
            let viewed_penalty = usize::from(recent.contains(node.id.as_str())) * 8;
            let period_penalty = usize::from(
                node.period_id
                    .as_deref()
                    .is_some_and(|id| recent_periods.contains(id)),
            ) * 3;
            let kind_penalty = usize::from(recent_kinds.contains(&node.kind)) * 2;
            let recency_penalty = positions.get(node.id.as_str()).copied().unwrap_or(99);
            (
                viewed_penalty + period_penalty + kind_penalty,
                recency_penalty,
                node.title.as_str(),
            )
        });
        let best_score = candidates.first().map(|node| {
            let viewed_penalty = usize::from(recent.contains(node.id.as_str())) * 8;
            let period_penalty = usize::from(
                node.period_id
                    .as_deref()
                    .is_some_and(|id| recent_periods.contains(id)),
            ) * 3;
            let kind_penalty = usize::from(recent_kinds.contains(&node.kind)) * 2;
            viewed_penalty + period_penalty + kind_penalty
        })?;
        let equally_good: Vec<&HistoryNode> = candidates
            .into_iter()
            .filter(|node| {
                let score = usize::from(recent.contains(node.id.as_str())) * 8
                    + usize::from(
                        node.period_id
                            .as_deref()
                            .is_some_and(|id| recent_periods.contains(id)),
                    ) * 3
                    + usize::from(recent_kinds.contains(&node.kind)) * 2;
                score == best_score
            })
            .collect();
        equally_good
            .get((cursor as usize) % equally_good.len())
            .cloned()
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryNodeKind;

    fn node(id: &str, kind: HistoryNodeKind, period: &str) -> HistoryNode {
        HistoryNode {
            id: id.into(),
            kind,
            title: id.into(),
            period_id: Some(period.into()),
            start_year: None,
            end_year: None,
            summary: String::new(),
            tags: vec![],
            source_ids: vec![],
        }
    }

    #[test]
    fn avoids_recent_item_and_period_when_possible() {
        let nodes = vec![
            node("tang-event", HistoryNodeKind::Event, "tang"),
            node("tang-person", HistoryNodeKind::Person, "tang"),
            node("jin-war", HistoryNodeKind::War, "eastern-jin"),
        ];
        let result = HistoryRecommendationService::recommend(&nodes, &["tang-event".into()], 0)
            .expect("recommendation");
        assert_eq!(result.id, "jin-war");
    }
}
