//! History 模块适配器（V4 §41-§47，Gate 5）。
//!
//! History 是 V4 第一个标准模块：注册 descriptor + 4 个 Read 工具 +
//! ContextProvider。**不重新设计 V3**：所有数据经 `HistoryService`
//! （V3 use case，端口 + 只读 DuckDB）复用；不新建 V3 数据面。
//!
//! - `history.search(query, entity_type?, limit?)` → 精炼命中列表
//! - `history.get_event(id)` → canonical + relations + enrichment_state + evidence 元数据
//! - `history.get_person(id)` → canonical + relations + events + stories
//! - `history.get_context(app_context)` → 当前实体 compact context
//!
//! 全部 `ToolRisk::Read`（V4 §21）。V3 无 on-demand enrichment（审计结论），
//! 故本模块**不注册** `history.ensure_enrichment`；`enrichment_state` 如实
//! 透传库内 quality_status + evidence 概况。

use std::sync::Arc;

use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};
use devtoolbox_core::personal_ai::{AppContext, EntityRef};
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::history::enrichment::EnrichmentRunnerPort;
use crate::history::{
    EnrichmentKey, EnrichmentSection, HistoryPortError, HistoryQueryPort, HistoryService,
};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::ToolExecutor;

// ---------------------------------------------------------------------------
// 端口引用适配器（application 内持有 Arc 端口，避免反复构造 service 成本）
// ---------------------------------------------------------------------------

/// `Arc<dyn HistoryQueryPort>` 的 newtype 适配：让工具/上下文提供方在每次
/// 调用时构造 `HistoryService`（V3 冻结签名：`new(Box<dyn HistoryQueryPort>)`）。
struct PortRef(Arc<dyn HistoryQueryPort>);

fn err(message: String) -> HistoryPortError {
    HistoryPortError(message)
}

impl HistoryQueryPort for PortRef {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
        self.0.get_dataset_stats()
    }
    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
        self.0.get_periods()
    }
    fn get_regimes_by_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<RegimeResult>, HistoryPortError> {
        self.0.get_regimes_by_period(period_id)
    }
    fn get_events_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        self.0.get_events_for_period(period_id)
    }
    fn get_people_for_period(
        &self,
        period_id: &str,
        limit: i64,
    ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        self.0.get_people_for_period(period_id, limit)
    }
    fn get_relations_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        self.0.get_relations_for_period(period_id)
    }
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.0.get_stories()
    }
    fn get_stories_for_period(
        &self,
        period_id: Option<&str>,
    ) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.0.get_stories_for_period(period_id)
    }
    fn get_story(&self, story_id: &str) -> Result<Option<StoryResult>, HistoryPortError> {
        self.0.get_story(story_id)
    }
    fn get_story_events(&self, story_id: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
        self.0.get_story_events(story_id)
    }
    fn get_story_people(&self, story_id: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        self.0.get_story_people(story_id)
    }
    fn get_story_places(&self, story_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        self.0.get_story_places(story_id)
    }
    fn get_story_texts(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.0.get_story_texts(story_id)
    }
    fn get_story_evidences(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.0.get_story_evidences(story_id)
    }
    fn get_event(&self, event_id: &str) -> Result<Option<EventResult>, HistoryPortError> {
        self.0.get_event(event_id)
    }
    fn get_event_people(&self, event_id: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        self.0.get_event_people(event_id)
    }
    fn get_event_places(&self, event_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        self.0.get_event_places(event_id)
    }
    fn get_event_relations(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        self.0.get_event_relations(event_id)
    }
    fn get_event_texts(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.0.get_event_texts(event_id)
    }
    fn get_event_evidences(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.0.get_event_evidences(event_id)
    }
    fn get_person(&self, person_id: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        self.0.get_person(person_id)
    }
    fn get_person_relations(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        self.0.get_person_relations(person_id)
    }
    fn get_person_places(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        self.0.get_person_places(person_id)
    }
    fn get_person_events(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        self.0.get_person_events(person_id)
    }
    fn get_person_stories(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        self.0.get_person_stories(person_id)
    }
    fn get_work_by_id(&self, work_id: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        self.0.get_work_by_id(work_id)
    }
    fn get_work(&self, title: &str, limit: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        self.0.get_work(title, limit)
    }
    fn get_historical_texts(
        &self,
        work: Option<&str>,
        limit: i64,
    ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        self.0.get_historical_texts(work, limit)
    }
    fn search_people(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<PersonResult>, HistoryPortError> {
        self.0.search_people(query, limit)
    }
    fn search_events(&self, query: &str, limit: i64) -> Result<Vec<EventResult>, HistoryPortError> {
        self.0.search_events(query, limit)
    }
    fn get_sources_for_ids(&self, ids: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        self.0.get_sources_for_ids(ids)
    }
}

// ---------------------------------------------------------------------------
// 工具实现
// ---------------------------------------------------------------------------

const TOOL_SEARCH: &str = "history.search";
const TOOL_GET_EVENT: &str = "history.get_event";
const TOOL_GET_PERSON: &str = "history.get_person";
const TOOL_GET_CONTEXT: &str = "history.get_context";
const TOOL_ENSURE_ENRICHMENT: &str = "history.ensure_enrichment";

/// History 工具执行器（一个结构体、四个身份；dispatch 属模块内部实现细节，
/// 不是 PersonalAgent 的分支）。
pub struct HistoryTools {
    port: Arc<dyn HistoryQueryPort>,
    budget: ContextBudget,
    /// 可选富化运行器（V5 Gate 4；None 时 ensure_enrichment 返回受控失败）。
    runner: Option<Arc<dyn EnrichmentRunnerPort>>,
}

impl HistoryTools {
    #[must_use]
    pub fn new(port: Arc<dyn HistoryQueryPort>) -> Self {
        Self::with_runner(port, None)
    }

    #[must_use]
    pub fn with_runner(
        port: Arc<dyn HistoryQueryPort>,
        runner: Option<Arc<dyn EnrichmentRunnerPort>>,
    ) -> Self {
        Self {
            port,
            budget: ContextBudget::default(),
            runner,
        }
    }

    fn service(&self) -> HistoryService {
        HistoryService::new(Box::new(PortRef(Arc::clone(&self.port))))
    }

    fn spec_for(&self, name: &str) -> ToolSpec {
        let input_schema = match name {
            TOOL_SEARCH => serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词"},
                    "entity_type": {"type": "string", "enum": ["person", "event", "work", "story"]},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20}
                }
            }),
            TOOL_GET_EVENT | TOOL_GET_PERSON => serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}}
            }),
            TOOL_ENSURE_ENRICHMENT => serde_json::json!({
                "type": "object",
                "required": ["entity", "section"],
                "properties": {
                    "entity": {
                        "type": "object",
                        "required": ["kind", "id"],
                        "properties": {
                            "kind": {"type": "string", "enum": ["event"]},
                            "id": {"type": "string"}
                        }
                    },
                    "section": {"type": "string", "enum": ["overview", "background", "impact"]},
                    "locale": {"type": "string"}
                }
            }),
            _ => serde_json::json!({
                "type": "object",
                "required": ["module", "entity"],
                "properties": {
                    "module": {"type": "string", "enum": ["history"]},
                    "entity": {
                        "type": "object",
                        "required": ["kind", "id"],
                        "properties": {
                            "kind": {"type": "string", "enum": ["person", "event"]},
                            "id": {"type": "string"}
                        }
                    }
                }
            }),
        };
        let description = match name {
            TOOL_SEARCH => "搜索历史知识库（人物/事件/著作/故事），返回精炼命中列表",
            TOOL_GET_EVENT => "获取历史事件的 canonical 事实、关系、证据与富化状态",
            TOOL_GET_PERSON => "获取历史人物的 canonical 事实、关系、事件与故事时间线",
            TOOL_ENSURE_ENRICHMENT => {
                "按需生成历史事件某 section 的 AI 富化（只写派生缓存，不改写 canonical；仅当缺失/过期/生成失败时调用）"
            }
            _ => "获取当前页面实体的紧凑上下文（供指代解析）",
        };
        let risk = if name == TOOL_ENSURE_ENRICHMENT {
            ToolRisk::SafeWrite
        } else {
            ToolRisk::Read
        };
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            risk,
            module: "history".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 每个工具一个薄执行器（满足 ToolRegistry 的 ToolExecutor 契约）
// ---------------------------------------------------------------------------

struct ToolImpl {
    name: &'static str,
    tools: Arc<HistoryTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        // 静态 spec 缓存：按 name 构造（工具名固定）。
        static CACHE: std::sync::OnceLock<[ToolSpec; 5]> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| {
            let tools = HistoryTools {
                port: Arc::<UnavailablePort>::new(UnavailablePort),
                budget: ContextBudget::default(),
                runner: None,
            };
            [
                tools.spec_for(TOOL_SEARCH),
                tools.spec_for(TOOL_GET_EVENT),
                tools.spec_for(TOOL_GET_PERSON),
                tools.spec_for(TOOL_GET_CONTEXT),
                tools.spec_for(TOOL_ENSURE_ENRICHMENT),
            ]
        });
        match self.name {
            TOOL_SEARCH => &cache[0],
            TOOL_GET_EVENT => &cache[1],
            TOOL_GET_PERSON => &cache[2],
            TOOL_ENSURE_ENRICHMENT => &cache[4],
            _ => &cache[3],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_GET_EVENT => self.tools.get_event(&arguments),
            TOOL_GET_PERSON => self.tools.get_person(&arguments),
            TOOL_ENSURE_ENRICHMENT => self.tools.ensure_enrichment(&arguments).await,
            _ => self.tools.get_context(&arguments),
        }
    }
}

/// spec 缓存占位端口（永不执行）。
struct UnavailablePort;
impl HistoryQueryPort for UnavailablePort {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_regimes_by_period(&self, _: &str) -> Result<Vec<RegimeResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_events_for_period(&self, _: &str) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_people_for_period(
        &self,
        _: &str,
        _: i64,
    ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_relations_for_period(
        &self,
        _: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_stories_for_period(
        &self,
        _: Option<&str>,
    ) -> Result<Vec<StoryResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story(&self, _: &str) -> Result<Option<StoryResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story_events(&self, _: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story_texts(&self, _: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_story_evidences(&self, _: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event(&self, _: &str) -> Result<Option<EventResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event_relations(&self, _: &str) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event_texts(&self, _: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_event_evidences(&self, _: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_person(&self, _: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_person_relations(&self, _: &str) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_person_places(&self, _: &str) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_person_events(&self, _: &str) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_person_stories(&self, _: &str) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_work_by_id(&self, _: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_work(&self, _: &str, _: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_historical_texts(
        &self,
        _: Option<&str>,
        _: i64,
    ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn search_people(&self, _: &str, _: i64) -> Result<Vec<PersonResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn search_events(&self, _: &str, _: i64) -> Result<Vec<EventResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
    fn get_sources_for_ids(&self, _: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        Err(err("unavailable".into()))
    }
}

// ---------------------------------------------------------------------------
// 具体工具逻辑
// ---------------------------------------------------------------------------

impl HistoryTools {
    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let query = arguments
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let entity_type = arguments
            .get("entity_type")
            .and_then(serde_json::Value::as_str);
        let limit = arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(8);
        if query.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "history.search: query is empty",
            ));
        }
        let groups = self.service().search(query).map_err(agent_error)?;
        let mut hits: Vec<serde_json::Value> = Vec::new();
        for group in groups {
            if let Some(kind) = entity_type
                && group.kind != kind
            {
                continue;
            }
            for item in group.items {
                hits.push(serde_json::json!({
                    "id": item.id,
                    "kind": item.kind,
                    "title": item.title,
                    "subtitle": item.subtitle,
                    "start_year": item.start_year,
                    "end_year": item.end_year,
                }));
                if hits.len() >= limit {
                    break;
                }
            }
            if hits.len() >= limit {
                break;
            }
        }
        Ok(ToolResult::ok_with_metadata(
            hits.clone().into(),
            serde_json::json!({"count": hits.len()}),
        ))
    }

    pub fn get_event(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "history.get_event: id is empty",
            ));
        }
        let detail = self.service().event_detail(id).map_err(agent_error)?;
        let Some(detail) = detail else {
            return Ok(ToolResult::fail(format!("event `{id}` 不存在")));
        };
        let budget = self.budget;
        let event = &detail.event;
        let data = serde_json::json!({
            "canonical": {
                "id": event.id,
                "name_zh_cn": event.name_zh_cn,
                "event_type": event.event_type,
                "start_year": event.start_year,
                "end_year": event.end_year,
                "date_precision": event.date_precision,
                "importance": event.importance,
                "summary_zh_cn": cap_chars(event.summary_zh_cn.as_deref(), budget.max_chars / 3),
                "background_zh_cn": cap_chars(event.background_zh_cn.as_deref(), budget.max_chars / 3),
                "process_zh_cn": cap_chars(event.process_zh_cn.as_deref(), budget.max_chars / 3),
                "result_zh_cn": cap_chars(event.result_zh_cn.as_deref(), budget.max_chars / 3),
                "impact_zh_cn": cap_chars(event.impact_zh_cn.as_deref(), budget.max_chars / 3),
                "source_reference": event.source_reference,
            },
            "relations": cap_list(
                &detail.relations,
                budget.max_items,
                |relation| serde_json::json!({
                    "relation_type": relation.relation_type,
                    "target_event_id": relation.target_event_id,
                    "target_event_name": relation.target_event_name,
                    "description_zh_cn": relation.description_zh_cn,
                }),
            ),
            "people": cap_list(&detail.people, budget.max_items, |person| serde_json::json!({
                "person_id": person.person_id,
                "person_name": person.person_name,
                "role_zh_cn": person.role_zh_cn,
            })),
            "places": cap_list(&detail.places, budget.max_items, |place| serde_json::json!({
                "place_id": place.place_id,
                "place_name": place.place_name,
            })),
            "enrichment_state": {
                "availability": "batch",   // V3 无 on-demand enrichment（审计结论），如实标注
                "quality_status": event.quality_status,
                "evidence_count": detail.evidences.len(),
                "source_type": event.source_type,
                "source_ids": event.source_ids,
            },
            "evidence": cap_list(&detail.evidences, budget.max_items, |evidence| serde_json::json!({
                "work": evidence.work,
                "term": evidence.term,
                "chapter_hint": evidence.chapter_hint,
                "evidence_role": evidence.evidence_role,
                "link_status": evidence.link_status,
            })),
        });
        Ok(ToolResult::ok(data))
    }

    pub fn get_person(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "history.get_person: id is empty",
            ));
        }
        let detail = self.service().person_detail(id).map_err(agent_error)?;
        let Some(detail) = detail else {
            return Ok(ToolResult::fail(format!("person `{id}` 不存在")));
        };
        let budget = self.budget;
        let person = &detail.person;
        let data = serde_json::json!({
            "canonical": {
                "id": person.id,
                "canonical_name_zh_cn": person.canonical_name_zh_cn,
                "name_raw": person.name_raw,
                "birth_year": person.birth_year,
                "death_year": person.death_year,
                "gender": person.gender,
                "quality_status": person.quality_status,
                "intro_zh_cn": cap_chars(person.intro_zh_cn.as_deref(), budget.max_chars / 2),
            },
            "relations": cap_list(&detail.relations, budget.max_items, |relation| serde_json::json!({
                "relation_name_zh_cn": relation.relation_name_zh_cn,
                "relation_type": relation.relation_type,
                "person_a_name": relation.person_a_name,
                "person_b_name": relation.person_b_name,
                "start_year": relation.start_year,
                "end_year": relation.end_year,
                "confidence": relation.confidence,
            })),
            "events": cap_list(&detail.events, budget.max_items, |event| serde_json::json!({
                "event_id": event.event_id,
                "event_name": event.event_name,
                "start_year": event.start_year,
                "end_year": event.end_year,
                "role_zh_cn": event.role_zh_cn,
                "summary_zh_cn": cap_chars(event.summary_zh_cn.as_deref(), 600),
            })),
            "stories": cap_list(&detail.stories, budget.max_items, |story| serde_json::json!({
                "story_id": story.story_id,
                "title_zh_cn": story.title_zh_cn,
                "start_year": story.start_year,
                "end_year": story.end_year,
            })),
        });
        Ok(ToolResult::ok(data))
    }

    /// history.get_context：依据 app_context（module/entity）返回紧凑上下文。
    pub fn get_context(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let module = arguments
            .get("module")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let entity = arguments.get("entity");
        if module != "history" {
            return Err(AgentError::tool_invalid_argument(format!(
                "history.get_context: unsupported module `{module}`"
            )));
        }
        let Some(entity) = entity else {
            return Err(AgentError::tool_invalid_argument(
                "history.get_context: entity is required",
            ));
        };
        let kind = entity
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let id = entity
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let app_context = AppContext {
            module: Some("history".into()),
            page: None,
            entity: Some(EntityRef {
                kind: kind.to_string(),
                id: id.to_string(),
                label: None,
            }),
            selection: None,
            view_state: serde_json::Value::Null,
        };
        let bundle =
            HistoryContextProvider { tools: self }.build_context(&app_context, &self.budget)?;
        Ok(ToolResult::ok(bundle.summary))
    }

    /// history.ensure_enrichment（V5 Gate 4）：仅写派生缓存；Agent 可调用后继续回答。
    pub async fn ensure_enrichment(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        let entity = arguments.get("entity").ok_or_else(|| {
            AgentError::tool_invalid_argument("history.ensure_enrichment: entity required")
        })?;
        let kind = entity
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let id = entity
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if kind != "event" {
            return Err(AgentError::tool_invalid_argument(format!(
                "history.ensure_enrichment: entity kind `{kind}` not supported (only event)"
            )));
        }
        if id.is_empty() {
            return Err(AgentError::tool_invalid_argument(
                "history.ensure_enrichment: entity id is empty",
            ));
        }
        let section = match arguments.get("section").and_then(serde_json::Value::as_str) {
            Some("overview") => EnrichmentSection::Overview,
            Some("background") => EnrichmentSection::Background,
            Some("impact") => EnrichmentSection::Impact,
            other => {
                return Err(AgentError::tool_invalid_argument(format!(
                    "history.ensure_enrichment: unsupported section `{}`",
                    other.unwrap_or_default()
                )));
            }
        };
        let locale = arguments
            .get("locale")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("zh-CN")
            .to_string();
        let key = EnrichmentKey::new("event", id, section, &locale);

        let Some(runner) = &self.runner else {
            return Ok(ToolResult::fail("enrichment runner 未配置（组合根未装配）"));
        };
        match runner.ensure(&key).await {
            Ok(view) => {
                let data = serde_json::to_value(&view)
                    .unwrap_or_else(|_| serde_json::json!({"state": "ready"}));
                Ok(ToolResult::ok(data))
            }
            Err(error) => Ok(ToolResult::fail(format!("enrichment failed: {error}"))),
        }
    }
}

fn agent_error(error: crate::error::ApplicationError) -> AgentError {
    AgentError::tool_execution_failed(error.to_string())
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// History 模块上下文提供方（V4 §25/§46）：把 UI AppContext 变紧凑上下文。
pub struct HistoryContextProvider<'a> {
    tools: &'a HistoryTools,
}

impl ModuleContextProvider for HistoryContextProvider<'_> {
    fn module_id(&self) -> &str {
        "history"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let Some(entity) = &app_context.entity else {
            return Ok(ContextBundle {
                module: "history".to_string(),
                headline: "History".to_string(),
                summary: serde_json::json!({"note": "当前无实体上下文（History 总览）"}),
            });
        };
        match entity.kind.as_str() {
            "person" => self.person_bundle(entity, budget),
            "event" => self.event_bundle(entity, budget),
            other => Err(AgentError::context(format!(
                "history context: unsupported entity kind `{other}`"
            ))),
        }
    }
}

impl HistoryContextProvider<'_> {
    fn person_bundle(
        &self,
        entity: &EntityRef,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let detail = self
            .tools
            .service()
            .person_detail(&entity.id)
            .map_err(agent_error)?;
        let Some(detail) = detail else {
            return Err(AgentError::context(format!(
                "person `{}` 不存在",
                entity.id
            )));
        };
        let person = &detail.person;
        let headline = format!(
            "History · {}（{}–{}）",
            person.canonical_name_zh_cn,
            person
                .birth_year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "?".into()),
            person
                .death_year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "?".into()),
        );
        let summary = serde_json::json!({
            "module": "history",
            "entity": {"kind": "person", "id": person.id, "label": person.canonical_name_zh_cn},
            "canonical": {
                "canonical_name_zh_cn": person.canonical_name_zh_cn,
                "birth_year": person.birth_year,
                "death_year": person.death_year,
                "intro_zh_cn": cap_chars(person.intro_zh_cn.as_deref(), budget.max_chars / 3),
            },
            "events": cap_list(&detail.events, budget.max_items, |event| serde_json::json!({
                "event_id": event.event_id,
                "event_name": event.event_name,
                "start_year": event.start_year,
                "role_zh_cn": event.role_zh_cn,
            })),
            "relations": cap_list(&detail.relations, budget.max_items, |relation| serde_json::json!({
                "relation_name_zh_cn": relation.relation_name_zh_cn,
                "with": relation.person_b_name,
            })),
        });
        Ok(ContextBundle {
            module: "history".into(),
            headline,
            summary,
        })
    }

    fn event_bundle(
        &self,
        entity: &EntityRef,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let detail = self
            .tools
            .service()
            .event_detail(&entity.id)
            .map_err(agent_error)?;
        let Some(detail) = detail else {
            return Err(AgentError::context(format!("event `{}` 不存在", entity.id)));
        };
        let event = &detail.event;
        let headline = format!("History · 事件 {}", event.name_zh_cn);
        let summary = serde_json::json!({
            "module": "history",
            "entity": {"kind": "event", "id": event.id, "label": event.name_zh_cn},
            "canonical": {
                "name_zh_cn": event.name_zh_cn,
                "start_year": event.start_year,
                "end_year": event.end_year,
                "importance": event.importance,
                "summary_zh_cn": cap_chars(event.summary_zh_cn.as_deref(), budget.max_chars / 3),
            },
            "people": cap_list(&detail.people, budget.max_items, |person| serde_json::json!({
                "person_name": person.person_name,
                "role_zh_cn": person.role_zh_cn,
            })),
            "relations": cap_list(&detail.relations, budget.max_items, |relation| serde_json::json!({
                "relation_type": relation.relation_type,
                "target_event_name": relation.target_event_name,
                "description_zh_cn": relation.description_zh_cn,
            })),
            "enrichment_state": {
                "availability": "batch",
                "quality_status": event.quality_status,
                "evidence_count": detail.evidences.len(),
            },
        });
        Ok(ContextBundle {
            module: "history".into(),
            headline,
            summary,
        })
    }
}

/// 注册 History 模块（descriptor + 4 工具 + context provider）。
#[allow(clippy::too_many_arguments)]
pub fn register_history(
    modules: &mut crate::personal_ai::registry::ModuleRegistry,
    tools: &mut crate::personal_ai::registry::ToolRegistry,
    port: Arc<dyn HistoryQueryPort>,
    runner: Option<Arc<dyn EnrichmentRunnerPort>>,
) -> Result<(), AgentError> {
    let history = Arc::new(HistoryTools::with_runner(Arc::clone(&port), runner));
    modules.register(crate::personal_ai::registry::ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "history".into(),
            display_name: "History".into(),
            description: "历史知识库：人物、事件、著作、故事与史料证据".into(),
            capabilities: vec!["search".into(), "entity".into(), "enrichment_state".into()],
            tools: vec![
                TOOL_SEARCH.into(),
                TOOL_GET_EVENT.into(),
                TOOL_GET_PERSON.into(),
                TOOL_GET_CONTEXT.into(),
                TOOL_ENSURE_ENRICHMENT.into(),
            ],
        },
        context_provider: Some(Arc::new(HistoryProviderOwned::new(Arc::clone(&port)))),
    })?;
    for name in [
        TOOL_SEARCH,
        TOOL_GET_EVENT,
        TOOL_GET_PERSON,
        TOOL_GET_CONTEXT,
        TOOL_ENSURE_ENRICHMENT,
    ] {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&history),
        }))?;
    }
    Ok(())
}

/// 组合根可见的 owned ContextProvider（Arc 化 HistoryTools）。
pub struct HistoryProviderOwned {
    tools: HistoryTools,
}
impl HistoryProviderOwned {
    #[must_use]
    pub fn new(port: Arc<dyn HistoryQueryPort>) -> Self {
        Self {
            tools: HistoryTools::new(port),
        }
    }
}
impl ModuleContextProvider for HistoryProviderOwned {
    fn module_id(&self) -> &str {
        "history"
    }
    fn build_context(
        &self,
        ctx: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        HistoryContextProvider { tools: &self.tools }.build_context(ctx, budget)
    }
}

/// 导出工具常量（外部测试 / 组合根引用）。
#[must_use]
pub fn history_tool_names() -> [&'static str; 5] {
    [
        TOOL_SEARCH,
        TOOL_GET_EVENT,
        TOOL_GET_PERSON,
        TOOL_GET_CONTEXT,
        TOOL_ENSURE_ENRICHMENT,
    ]
}

// ---------------------------------------------------------------------------
// 预算辅助
// ---------------------------------------------------------------------------

fn cap_chars(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|t| {
        if t.chars().count() <= max {
            t.to_string()
        } else {
            let head: String = t.chars().take(max).collect();
            format!("{head}…[截断]")
        }
    })
}

fn cap_list<T>(
    items: &[T],
    max: usize,
    map: impl Fn(&T) -> serde_json::Value,
) -> Vec<serde_json::Value> {
    items.iter().take(max).map(map).collect()
}
#[cfg(test)]
mod history_tests;
