//! Cross-module Geography → History tool adapter.
//!
//! The existing `ExplorationService` owns reviewed stable-ID matching and History's
//! verified-place filtering. This module only projects that result for the agent and
//! supplies canonical, client-neutral source references.

use std::sync::Arc;

use devtoolbox_core::{
    AgentCitation, AgentCitationRole, AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec,
};

use crate::exploration::ExplorationService;
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};

const TOOL_LOCATION_HISTORY: &str = "exploration.location_history";
const MAX_EVENTS: usize = 20;

#[derive(Clone, Debug, serde::Serialize)]
pub struct VerifiedLocationHistory {
    pub geography: GeographyReference,
    pub crosswalks: Vec<ReviewedCrosswalk>,
    pub events: Vec<VerifiedHistoryEvent>,
    pub story_context: Vec<HistoryStoryContext>,
    pub truncated: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GeographyReference {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub summary: String,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ReviewedCrosswalk {
    pub id: String,
    pub geography_entity_id: String,
    pub history_place_id: String,
    pub relation_type: String,
    pub status: String,
    pub evidence_time_scope: Option<serde_json::Value>,
    pub source_ids: Vec<String>,
    pub provenance_record_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct VerifiedHistoryEvent {
    pub id: String,
    pub title: String,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
    pub summary: Option<String>,
    pub quality_status: Option<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct HistoryStoryContext {
    pub id: String,
    pub title: String,
    pub related_event_id: String,
    pub summary: Option<String>,
    pub source_ids: Vec<String>,
}

/// Narrow query interface used by the tool; production delegates to ExplorationService.
pub trait LocationHistoryLookup: Send + Sync {
    fn location_history(
        &self,
        geography_id: &str,
    ) -> Result<Option<VerifiedLocationHistory>, String>;
}

impl LocationHistoryLookup for ExplorationService {
    fn location_history(
        &self,
        geography_id: &str,
    ) -> Result<Option<VerifiedLocationHistory>, String> {
        let Some(context) = self
            .location_history(geography_id)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };

        let geography = GeographyReference {
            id: context.geography.entity.id.clone(),
            name: context.geography.entity.name.clone(),
            entity_type: serde_json::to_value(context.geography.entity.entity_type)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            summary: context.geography.entity.summary.clone(),
            source_ids: context.geography.entity.source_ids.clone(),
        };
        let crosswalks = context
            .crosswalks
            .iter()
            .map(|link| ReviewedCrosswalk {
                id: link.id.to_string(),
                geography_entity_id: link.geography_entity_id.to_string(),
                history_place_id: link.history_place_id.to_string(),
                relation_type: format!("{:?}", link.relation_type).to_lowercase(),
                status: format!("{:?}", link.status).to_lowercase(),
                evidence_time_scope: link.evidence_time_scope.map(|scope| {
                    serde_json::json!({
                        "start_year": scope.start_year,
                        "end_year": scope.end_year,
                        "precision": scope.precision,
                    })
                }),
                source_ids: link
                    .geography_source_ids
                    .iter()
                    .chain(link.history_source_ids.iter())
                    .map(|id| (*id).to_string())
                    .collect(),
                provenance_record_ids: link
                    .provenance
                    .iter()
                    .map(|record| record.record_id.to_string())
                    .collect(),
            })
            .collect();

        let total_events = context.events.len();
        let mut events = Vec::new();
        let mut story_context = Vec::new();
        for detail in context.events.into_iter().take(MAX_EVENTS) {
            let mut source_ids = parse_source_ids(detail.event.source_ids.as_deref());
            source_ids.extend(detail.sources.iter().map(|source| source.id.clone()));
            source_ids.sort();
            source_ids.dedup();
            events.push(VerifiedHistoryEvent {
                id: detail.event.id.clone(),
                title: detail.event.name_zh_cn.clone(),
                start_year: detail.event.start_year,
                end_year: detail.event.end_year,
                summary: detail.event.summary_zh_cn.clone(),
                quality_status: detail.event.quality_status.clone(),
                source_ids,
            });
            for relation in detail.related_stories {
                let mut story_sources = parse_source_ids(relation.story.source_ids.as_deref());
                story_sources.sort();
                story_sources.dedup();
                story_context.push(HistoryStoryContext {
                    id: relation.story_id,
                    title: relation.story.title_zh_cn,
                    related_event_id: relation.event_id,
                    summary: relation.story.summary_zh_cn,
                    source_ids: story_sources,
                });
            }
        }
        let truncated = total_events > MAX_EVENTS;
        Ok(Some(VerifiedLocationHistory {
            geography,
            crosswalks,
            events,
            story_context,
            truncated,
        }))
    }
}

fn parse_source_ids(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

struct LocationHistoryTool {
    query: Arc<dyn LocationHistoryLookup>,
    spec: ToolSpec,
}

#[async_trait::async_trait]
impl ToolExecutor for LocationHistoryTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        let geography_id = arguments
            .get("geography_entity_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        if geography_id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "exploration.location_history: geography_entity_id is required",
            ));
        }
        let Some(result) = self
            .query
            .location_history(geography_id)
            .map_err(|error| AgentError::tool_execution_failed(format!("exploration: {error}")))?
        else {
            return Ok(ToolResult::fail(format!(
                "Geography entity `{geography_id}` 不存在；未按名称推断其他 ID"
            )));
        };
        let mut citations = vec![AgentCitation {
            module: "geography".to_string(),
            entity_type: "location".to_string(),
            entity_id: result.geography.id.clone(),
            title: result.geography.name.clone(),
            role: AgentCitationRole::CurrentEntity,
            source_ids: result.geography.source_ids.clone(),
        }];
        for event in &result.events {
            citations.push(AgentCitation {
                module: "history".to_string(),
                entity_type: "event".to_string(),
                entity_id: event.id.clone(),
                title: event.title.clone(),
                role: AgentCitationRole::VerifiedAssociation,
                source_ids: event.source_ids.clone(),
            });
        }
        for story in &result.story_context {
            citations.push(AgentCitation {
                module: "history".to_string(),
                entity_type: "story".to_string(),
                entity_id: story.id.clone(),
                title: story.title.clone(),
                role: AgentCitationRole::StoryContext,
                source_ids: story.source_ids.clone(),
            });
        }
        let data = serde_json::to_value(result)
            .map_err(|error| AgentError::tool_execution_failed(error.to_string()))?;
        let mut tool_result = ToolResult::ok(data);
        tool_result.citations = citations;
        Ok(tool_result)
    }
}

pub fn register_exploration(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    query: Arc<dyn LocationHistoryLookup>,
) -> Result<(), AgentError> {
    let spec = ToolSpec {
        name: TOOL_LOCATION_HISTORY.to_string(),
        description: "按 Geography canonical stable ID 读取已审核 crosswalk、对应 History 中 reviewed 事件与来源；故事只作背景并与事件证据分开。不按地点名称猜测关联。".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["geography_entity_id"],
            "properties": {
                "geography_entity_id": {"type": "string", "minLength": 1, "maxLength": 512}
            },
            "additionalProperties": false
        }),
        risk: ToolRisk::Read,
        module: "exploration".to_string(),
    };
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "exploration".to_string(),
            display_name: "Geography ↔ History".to_string(),
            description: "只使用已审核 stable-ID crosswalk 探索地点关联的 History 事件、来源与分离标注的故事背景".to_string(),
            capabilities: vec!["reviewed_crosswalk".to_string(), "history_sources".to_string()],
            tools: vec![TOOL_LOCATION_HISTORY.to_string()],
        },
        context_provider: None,
    })?;
    tools.register(Arc::new(LocationHistoryTool { query, spec }))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::registry::ToolRegistry;
    use devtoolbox_core::ToolCallRequest;

    struct FakeLookup;
    impl LocationHistoryLookup for FakeLookup {
        fn location_history(
            &self,
            geography_id: &str,
        ) -> Result<Option<VerifiedLocationHistory>, String> {
            if geography_id != "luoyang" {
                return Ok(None);
            }
            Ok(Some(VerifiedLocationHistory {
                geography: GeographyReference {
                    id: "luoyang".into(),
                    name: "洛阳".into(),
                    entity_type: "city".into(),
                    summary: "现代地理实体".into(),
                    source_ids: vec!["geo-source".into()],
                },
                crosswalks: vec![ReviewedCrosswalk {
                    id: "geography:luoyang|history:cbdb-place-14693".into(),
                    geography_entity_id: "luoyang".into(),
                    history_place_id: "cbdb-place-14693".into(),
                    relation_type: "modernurbanareaanchor".into(),
                    status: "reviewed".into(),
                    evidence_time_scope: Some(
                        serde_json::json!({"start_year": 756, "end_year": 756}),
                    ),
                    source_ids: vec!["crosswalk-source".into()],
                    provenance_record_ids: vec!["event-anlu-luoyang".into()],
                }],
                events: vec![VerifiedHistoryEvent {
                    id: "event-anlu-luoyang".into(),
                    title: "洛阳失守".into(),
                    start_year: Some(756),
                    end_year: Some(756),
                    summary: Some("reviewed event".into()),
                    quality_status: Some("reviewed".into()),
                    source_ids: vec!["history-source".into()],
                }],
                story_context: vec![HistoryStoryContext {
                    id: "story-an-lushan".into(),
                    title: "安史之乱故事".into(),
                    related_event_id: "event-anlu-luoyang".into(),
                    summary: Some("background narrative".into()),
                    source_ids: vec!["story-source".into()],
                }],
                truncated: false,
            }))
        }
    }

    #[test]
    fn exploration_tool_returns_only_stable_linked_events_and_separates_story_citations() {
        let mut modules = ModuleRegistry::new();
        let mut tools = ToolRegistry::new();
        register_exploration(&mut modules, &mut tools, Arc::new(FakeLookup)).unwrap();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(tools.execute(&ToolCallRequest {
                id: "c1".into(),
                name: TOOL_LOCATION_HISTORY.into(),
                arguments: serde_json::json!({"geography_entity_id": "luoyang"}),
            }))
            .unwrap();
        assert!(result.ok);
        assert_eq!(
            result.data["crosswalks"][0]["history_place_id"],
            "cbdb-place-14693"
        );
        assert_eq!(result.data["events"][0]["id"], "event-anlu-luoyang");
        assert_eq!(
            result.citations[1].role,
            AgentCitationRole::VerifiedAssociation
        );
        assert_eq!(result.citations[1].entity_id, "event-anlu-luoyang");
        assert_eq!(result.citations[2].role, AgentCitationRole::StoryContext);
        assert_eq!(result.citations[2].entity_id, "story-an-lushan");
    }

    #[test]
    fn unknown_geography_id_does_not_fall_back_to_a_matching_name() {
        let tool = LocationHistoryTool {
            query: Arc::new(FakeLookup),
            spec: ToolSpec {
                name: TOOL_LOCATION_HISTORY.into(),
                description: "d".into(),
                input_schema: serde_json::json!({"type": "object", "required": ["geography_entity_id"], "properties": {"geography_entity_id": {"type": "string"}}}),
                risk: ToolRisk::Read,
                module: "exploration".into(),
            },
        };
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(tool.execute(serde_json::json!({"geography_entity_id": "洛阳"})))
            .unwrap();
        assert!(!result.ok);
        assert!(result.error.unwrap().contains("未按名称推断"));
    }
}
