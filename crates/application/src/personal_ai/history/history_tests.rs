//! History 模块集成测试（V4 §80：Case A-D）。
//!
//! 使用与 `crate::history::tests` 相同的 Fake HistoryQueryPort 模式
//! （Arc 端口 + 结构化夹具），不触真实 DuckDB。验证：
//! - B: history.search 返回精简命中列表
//! - C: history.get_event 含 canonical + enrichment_state，**不触发** enrichment
//! -   : history.get_person 含 canonical + relations + events + stories
//! - A: HistoryContextProvider 可解析 Person 上下文（指代问题可答）

use std::sync::Arc;

use devtoolbox_core::history_records::*;
use devtoolbox_core::personal_ai::{AppContext, EntityRef};
use devtoolbox_core::{ToolCallRequest, ToolResult, ToolRisk};

use crate::history::{HistoryPortError, HistoryQueryPort};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::history::{HistoryProviderOwned, history_tool_names, register_history};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

/// Fake HistoryQueryPort：search / event / person 场景数据 + 默认空。
struct FakeHistoryPort {
    people: Vec<PersonResult>,
    events: Vec<EventResult>,
    event_people: Vec<EventPersonResult>,
    event_relations: Vec<EventRelationResult>,
    event_evidences: Vec<EventEvidenceResult>,
    person_relations: Vec<PersonRelationResult>,
    person_events: Vec<PersonEventResult>,
    person_stories: Vec<PersonStoryResult>,
}

impl FakeHistoryPort {
    fn empty() -> Self {
        Self {
            people: vec![],
            events: vec![],
            event_people: vec![],
            event_relations: vec![],
            event_evidences: vec![],
            person_relations: vec![],
            person_events: vec![],
            person_stories: vec![],
        }
    }

    fn with_demo() -> (Self, String, String) {
        let person_id = "mao_zedong".to_string();
        let event_id = "zunyi_meeting".to_string();
        let mut port = Self::empty();
        port.people = vec![PersonResult {
            id: person_id.clone(),
            canonical_name_zh_cn: "毛泽东".into(),
            name_raw: None,
            birth_year: Some(1893),
            death_year: Some(1976),
            gender: Some("男".into()),
            quality_status: Some("verified".into()),
            created_from_source: None,
            intro_zh_cn: Some("中国共产党主要领导人之一".into()),
        }];
        port.events = vec![EventResult {
            id: event_id.clone(),
            name_zh_cn: "遵义会议".into(),
            event_type: Some("meeting".into()),
            start_year: Some(1935),
            end_year: None,
            date_precision: Some("exact".into()),
            period_id: Some("p1".into()),
            regime_id: None,
            summary_zh_cn: Some("长征途中一次重要会议，确立毛泽东的领导地位。".into()),
            background_zh_cn: None,
            process_zh_cn: None,
            result_zh_cn: None,
            impact_zh_cn: None,
            importance: Some("critical".into()),
            quality_status: Some("reviewed".into()),
            source_type: Some("curated".into()),
            source_ids: Some(r#"["src_1"]"#.into()),
            period_ids: None,
            dynasty_ids: None,
            regime_ids: None,
            source_reference: Some("《毛泽东年谱》".into()),
        }];
        port.event_people = vec![EventPersonResult {
            event_id: event_id.clone(),
            person_id: person_id.clone(),
            role: "participant".into(),
            role_zh_cn: Some("参与者".into()),
            side: None,
            importance: Some("major".into()),
            source_id: None,
            quality_status: None,
            person_name: Some("毛泽东".into()),
            description: None,
            link_quality_status: None,
            link_confidence: Some(0.9),
            link_reason: None,
            birth_year: None,
            death_year: None,
            person_quality_status: None,
        }];
        port.event_relations = vec![EventRelationResult {
            source_event_id: event_id.clone(),
            target_event_id: "cross_luding_bridge".into(),
            relation_type: "caused".into(),
            confidence: Some(0.8),
            description_zh_cn: Some("随后发生".into()),
            source_id: None,
            quality_status: None,
            source_event_name: Some("遵义会议".into()),
            target_event_name: Some("强渡大渡河".into()),
        }];
        port.event_evidences = vec![EventEvidenceResult {
            id: "ev_1".into(),
            event_id: event_id.clone(),
            historical_text_id: None,
            work: Some("毛泽东年谱".into()),
            term: Some("遵义会议".into()),
            chapter_hint: Some("1935 年卷".into()),
            context_keywords: None,
            evidence_role: Some("primary".into()),
            link_status: Some("linked".into()),
            link_quality_status: None,
            link_confidence: None,
            review_note: None,
            source_type: None,
            source_id: None,
            quality_status: None,
            rejected_text_ids: None,
        }];
        port.person_relations = vec![PersonRelationResult {
            person_a_id: person_id.clone(),
            person_a_name: Some("毛泽东".into()),
            person_b_id: "zhou_enlai".into(),
            person_b_name: Some("周恩来".into()),
            relation_type: "comrade".into(),
            start_year: None,
            end_year: None,
            source_ids: None,
            confidence: Some(0.95),
            relation_name_zh_cn: Some("战友".into()),
            relation_category: Some("political".into()),
        }];
        port.person_events = vec![PersonEventResult {
            event_id: event_id.clone(),
            event_name: "遵义会议".into(),
            start_year: Some(1935),
            end_year: None,
            summary_zh_cn: Some("确立领导地位".into()),
            role_zh_cn: Some("参与者".into()),
        }];
        port.person_stories = vec![PersonStoryResult {
            story_id: "story_1".into(),
            title_zh_cn: "长征".into(),
            start_year: Some(1934),
            end_year: Some(1936),
            summary_zh_cn: None,
        }];
        (port, person_id, event_id)
    }

    fn find_person(&self, id: &str) -> Option<PersonResult> {
        self.people.iter().find(|p| p.id == id).cloned()
    }
    fn find_event(&self, id: &str) -> Option<EventResult> {
        self.events.iter().find(|e| e.id == id).cloned()
    }
}

impl HistoryQueryPort for FakeHistoryPort {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
        Ok(DatasetStats::default())
    }
    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_regimes_by_period(&self, _: &str) -> Result<Vec<RegimeResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_events_for_period(&self, _: &str) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_people_for_period(&self, _: &str, _: i64) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_relations_for_period(&self, _: &str) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_stories_for_period(&self, _: Option<&str>) -> Result<Vec<StoryResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_story(&self, _: &str) -> Result<Option<StoryResult>, HistoryPortError> {
        Ok(None)
    }
    fn get_story_events(&self, _: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_story_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_story_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_story_texts(&self, _: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_story_evidences(&self, _: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_event(&self, id: &str) -> Result<Option<EventResult>, HistoryPortError> {
        Ok(self.find_event(id))
    }
    fn get_event_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        Ok(self.event_people.clone())
    }
    fn get_event_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_event_relations(&self, _: &str) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        Ok(self.event_relations.clone())
    }
    fn get_event_texts(&self, _: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_event_evidences(&self, _: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        Ok(self.event_evidences.clone())
    }
    fn get_person(&self, id: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        Ok(self.find_person(id))
    }
    fn get_person_relations(&self, _: &str) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        Ok(self.person_relations.clone())
    }
    fn get_person_places(&self, _: &str) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_person_events(&self, _: &str) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        Ok(self.person_events.clone())
    }
    fn get_person_stories(&self, _: &str) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        Ok(self.person_stories.clone())
    }
    fn get_work_by_id(&self, _: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        Ok(None)
    }
    fn get_work(&self, _: &str, _: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn get_historical_texts(&self, _: Option<&str>, _: i64) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        Ok(vec![])
    }
    fn search_people(&self, query: &str, _: i64) -> Result<Vec<PersonResult>, HistoryPortError> {
        Ok(self.people.iter().filter(|p| p.canonical_name_zh_cn.contains(query)).cloned().collect())
    }
    fn search_events(&self, query: &str, _: i64) -> Result<Vec<EventResult>, HistoryPortError> {
        Ok(self.events.iter().filter(|e| e.name_zh_cn.contains(query)).cloned().collect())
    }
    fn get_sources_for_ids(&self, _: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        Ok(vec![])
    }
}

fn registered() -> (ModuleRegistry, ToolRegistry, Arc<dyn HistoryQueryPort>) {
    let (port, _person_id, _event_id) = FakeHistoryPort::with_demo();
    let port: Arc<dyn HistoryQueryPort> = Arc::new(port);
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_history(&mut modules, &mut tools, Arc::clone(&port)).unwrap();
    (modules, tools, port)
}

fn call_tool(tools: &ToolRegistry, name: &str, args: serde_json::Value) -> ToolResult {
    tools
        .execute(&ToolCallRequest { id: "t".into(), name: name.into(), arguments: args })
        .unwrap()
}

// Case B：history.search 返回精简命中列表（真实 repository 数据）。
#[test]
fn history_search_tool_returns_compact_hits() {
    let (_modules, tools, _port) = registered();
    let result = call_tool(&tools, "history.search", serde_json::json!({"query": "遵义"}));
    assert!(result.ok);
    let hits = result.data.as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], "zunyi_meeting");
    assert_eq!(hits[0]["kind"], "event");
    assert_eq!(hits[0]["title"], "遵义会议");
    assert_eq!(result.metadata["count"], 1);
}

// Case C：history.get_event 返回 canonical + relations + enrichment_state，
// 且**不触发**额外 enrichment（V3 无 on-demand，availability=batch）。
#[test]
fn history_get_event_returns_canonical_and_state() {
    let (_modules, tools, _port) = registered();
    let result = call_tool(&tools, "history.get_event", serde_json::json!({"id": "zunyi_meeting"}));
    assert!(result.ok);
    assert_eq!(result.data["canonical"]["name_zh_cn"], "遵义会议");
    assert_eq!(result.data["canonical"]["start_year"], 1935);
    assert_eq!(result.data["relations"][0]["target_event_name"], "强渡大渡河");
    assert_eq!(result.data["enrichment_state"]["availability"], "batch");
    assert_eq!(result.data["enrichment_state"]["quality_status"], "reviewed");
    assert_eq!(result.data["evidence"][0]["work"], "毛泽东年谱");

    let missing = call_tool(&tools, "history.get_event", serde_json::json!({"id": "nope"}));
    assert!(!missing.ok);
    assert!(missing.error.unwrap().contains("不存在"));
}

// history.get_person：canonical + relations + events + stories
#[test]
fn history_get_person_returns_canonical_and_timeline() {
    let (_modules, tools, _port) = registered();
    let result = call_tool(&tools, "history.get_person", serde_json::json!({"id": "mao_zedong"}));
    assert!(result.ok);
    assert_eq!(result.data["canonical"]["canonical_name_zh_cn"], "毛泽东");
    assert_eq!(result.data["canonical"]["birth_year"], 1893);
    assert_eq!(result.data["relations"][0]["relation_name_zh_cn"], "战友");
    assert_eq!(result.data["events"][0]["event_name"], "遵义会议");
    assert_eq!(result.data["stories"][0]["title_zh_cn"], "长征");
}

// Case A：HistoryContextProvider 解析 Person 上下文（"他参加了什么事件？" 的可答输入）。
#[test]
fn history_context_provider_resolves_person() {
    let (_modules, _tools, port) = registered();
    let provider = HistoryProviderOwned::new(port);
    let ctx = AppContext {
        module: Some("history".into()),
        page: Some("person-detail".into()),
        entity: Some(EntityRef { kind: "person".into(), id: "mao_zedong".into(), label: Some("毛泽东".into()) }),
        selection: None,
        view_state: serde_json::Value::Null,
    };
    let bundle = provider.build_context(&ctx, &ContextBudget::default()).unwrap();
    assert_eq!(bundle.module, "history");
    assert!(bundle.headline.contains("毛泽东"));
    assert!(bundle.headline.contains("1893"));
    let events = bundle.summary["events"].as_array().unwrap();
    assert!(!events.is_empty());
}

// 不支持的 entity kind → controlled context error
#[test]
fn history_context_rejects_unknown_entity_kind() {
    let (_modules, _tools, port) = registered();
    let provider = HistoryProviderOwned::new(port);
    let ctx = AppContext {
        module: Some("history".into()),
        page: Some("x".into()),
        entity: Some(EntityRef { kind: "place".into(), id: "changsha".into(), label: None }),
        selection: None,
        view_state: serde_json::Value::Null,
    };
    let error = provider.build_context(&ctx, &ContextBudget::default()).unwrap_err();
    assert_eq!(error.code(), "personal_ai_context_error");
}

// 注册后模块描述与工具齐全；未注册模块无 context provider
#[test]
fn registration_exposes_descriptor_and_tools() {
    let (modules, tools, _port) = registered();
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "history");
    assert_eq!(descriptors[0].tools.len(), 4);
    assert_eq!(tools.len(), 4);
    for name in history_tool_names() {
        let spec = tools.spec(name).expect("missing tool");
        assert_eq!(spec.risk, ToolRisk::Read);
    }
}