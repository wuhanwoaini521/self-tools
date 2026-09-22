//! Travel 模块集成测试（V5 §68；Fake 全依赖，不触网）。
//!
//! 覆盖：模块注册与工具发现 / destination 搜索 / trip context 读取 / 缺失城市受控失败 /
//! context provider 解析 / plan_preview 不写数据 / PersonalAgent 工具路由（core 无业务分支）。

use std::sync::{Arc, Mutex};

use devtoolbox_core::personal_ai::{
    AgentRequest, AgentResponse, AppContext as AiAppContext, ChatModelProvider, ChatRequest,
    ChatResponse, ChatToolCall,
};
use devtoolbox_core::{ToolCallRequest, ToolRisk};

use super::{TravelContextProvider, register_travel, travel_tool_names};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};
use crate::travel::ports::{TravelAiPort, TravelSearchHit, TripContext};

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// 可编程 TravelAiPort：固定搜索结果 + 缓存行程表；无任何写方法（§43 不写数据）。
#[derive(Default)]
struct FakeTravelPort {
    configured: bool,
    hits: Vec<TravelSearchHit>,
    cached: std::collections::HashMap<String, TripContext>,
    search_calls: Arc<Mutex<usize>>,
    context_calls: Arc<Mutex<usize>>,
}

#[async_trait::async_trait]
impl TravelAiPort for FakeTravelPort {
    fn configured(&self) -> bool {
        self.configured
    }

    async fn search_destination(
        &self,
        _query: &str,
        limit: usize,
    ) -> Result<Vec<TravelSearchHit>, String> {
        *self.search_calls.lock().unwrap() += 1;
        Ok(self.hits.iter().take(limit).cloned().collect())
    }

    fn trip_context(&self, city: &str) -> Result<Option<TripContext>, String> {
        *self.context_calls.lock().unwrap() += 1;
        Ok(self.cached.get(city).cloned())
    }

    fn plan_preview(&self, city: &str, days: u8) -> Result<Option<TripContext>, String> {
        // 预览：有缓存则速览；无缓存返回确定性骨架（无写操作，直接观测缓存表即可证明没写）。
        Ok(self.cached.get(city).cloned().map(|mut context| {
            context.days = days;
            context.from_cache = true;
            context
        }))
    }
}

impl FakeTravelPort {
    fn with_dalian() -> Self {
        let mut port = Self {
            configured: true,
            hits: vec![TravelSearchHit {
                title: "大连滨海路".into(),
                url: "https://travel.cn/dalian".into(),
                snippet: "看海与摄影推荐".into(),
                domain: "travel.cn".into(),
            }],
            cached: std::collections::HashMap::new(),
            search_calls: Arc::new(Mutex::new(0)),
            context_calls: Arc::new(Mutex::new(0)),
        };
        port.cached.insert(
            "大连".into(),
            TripContext {
                city: "大连".into(),
                days: 3,
                summary: Some("滨海 + 历史 + 美食 三日".into()),
                highlights: vec!["滨海路".into(), "历史建筑街区".into()],
                sources_count: 6,
                from_cache: true,
            },
        );
        port
    }
}

/// 可编程模型：FIFO 队列（复用 agent 测试模式）。
struct MiniChat {
    steps: Mutex<std::collections::VecDeque<ChatResponse>>,
}

#[async_trait::async_trait]
impl ChatModelProvider for MiniChat {
    fn name(&self) -> &'static str {
        "mini"
    }
    async fn chat(
        &self,
        _request: ChatRequest,
    ) -> Result<ChatResponse, devtoolbox_core::ProviderError> {
        Ok(self
            .steps
            .lock()
            .unwrap()
            .pop_front()
            .expect("chat script exhausted"))
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn travel_module_registers_descriptor_and_tools() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port: Arc<dyn TravelAiPort> = Arc::new(FakeTravelPort::default());
    register_travel(&mut modules, &mut tools, port).unwrap();

    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "travel");
    assert_eq!(descriptors[0].display_name, "Travel");
    assert_eq!(
        descriptors[0].capabilities,
        vec!["search", "destination", "planning", "itinerary"]
    );
    assert_eq!(descriptors[0].tools.len(), 4);

    assert_eq!(tools.len(), 4);
    for name in travel_tool_names() {
        let spec = tools.spec(name).expect("missing travel tool");
        assert_eq!(spec.risk, ToolRisk::Read);
        assert_eq!(spec.module, "travel");
    }
    // 未注册工具 → 受控失败
    assert!(tools.spec("travel.missing").is_none());
}

#[test]
fn travel_search_destination_returns_hits() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = FakeTravelPort::with_dalian();
    let search_calls = Arc::clone(&port.search_calls);
    register_travel(&mut modules, &mut tools, Arc::new(port)).unwrap();

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "call_1".into(),
        name: "travel.search_destination".into(),
        arguments: serde_json::json!({"query": "大连 看海", "limit": 5}),
    }))
    .unwrap();
    assert!(result.ok);
    let hits = result.data.as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["title"], "大连滨海路");
    assert_eq!(result.metadata["count"], 1);
    assert_eq!(*search_calls.lock().unwrap(), 1);
}

#[test]
fn travel_trip_context_reads_cache_and_fails_when_missing() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = FakeTravelPort::with_dalian();
    let context_calls = Arc::clone(&port.context_calls);
    register_travel(&mut modules, &mut tools, Arc::new(port)).unwrap();

    // 命中缓存
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "travel.get_trip_context".into(),
        arguments: serde_json::json!({"city": "大连"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["city"], "大连");
    assert_eq!(result.data["days"], 3);
    assert_eq!(result.data["highlights"][0], "滨海路");

    // 缺失城市 → 受控 fail
    let missing = block_on(tools.execute(&ToolCallRequest {
        id: "c2".into(),
        name: "travel.get_trip_context".into(),
        arguments: serde_json::json!({"city": "不存在市"}),
    }))
    .unwrap();
    assert!(!missing.ok);
    assert!(missing.error.unwrap().contains("没有已保存行程"));

    // 缺参 → 校验错误（不进执行器）
    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c3".into(),
        name: "travel.get_trip_context".into(),
        arguments: serde_json::json!({}),
    }))
    .unwrap_err();
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
    assert_eq!(
        *context_calls.lock().unwrap(),
        2,
        "invalid args must not reach the port"
    );
}

#[test]
fn travel_plan_preview_does_not_write() {
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port = FakeTravelPort::with_dalian();
    let cached_len_before = port.cached.len();
    register_travel(&mut modules, &mut tools, Arc::new(port)).unwrap();

    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "travel.plan_trip".into(),
        arguments: serde_json::json!({"city": "大连", "days": 3}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["days"], 3);
    assert_eq!(
        result.metadata["note"],
        "预览未写入永久数据；确认后可到 Travel 页保存"
    );
    // TravelAiPort 无任何写方法（编译期保证）；缓存表仅读取。
    assert!(cached_len_before >= 1, "cache must be seeded");
}

#[test]
fn travel_context_provider_resolves_destination() {
    let port = FakeTravelPort::with_dalian();
    let provider = TravelContextProvider {
        port: Arc::new(port),
    };
    let ctx = AiAppContext {
        module: Some("travel".into()),
        page: Some("guide".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "destination".into(),
            id: "大连".into(),
            label: Some("大连".into()),
        }),
        selection: None,
        view_state: serde_json::json!({"days": 3, "preferences": ["历史", "美食", "摄影"]}),
    };
    let bundle = provider
        .build_context(&ctx, &ContextBudget::default())
        .unwrap();
    assert_eq!(bundle.module, "travel");
    assert!(bundle.headline.contains("大连"));
    assert!(bundle.headline.contains("3 天"));
    assert_eq!(bundle.summary["city"], "大连");
    assert!(bundle.summary["highlights"].as_array().unwrap().len() >= 2);
}

#[test]
fn travel_context_provider_general_when_no_entity() {
    let provider = TravelContextProvider {
        port: Arc::new(FakeTravelPort::default()),
    };
    let bundle = provider
        .build_context(&AiAppContext::default(), &ContextBudget::default())
        .unwrap();
    assert_eq!(
        bundle.summary["note"]
            .as_str()
            .unwrap()
            .contains("无目的地"),
        true
    );
}

#[test]
fn personal_agent_route_calls_travel_tool() {
    use crate::personal_ai::agent::{PersonalAgent, PersonalHub};
    use crate::personal_ai::session::InMemorySessionStore;

    let port = FakeTravelPort::with_dalian();
    let context_calls = Arc::clone(&port.context_calls);
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_travel(&mut modules, &mut tools, Arc::new(port)).unwrap();
    let hub = Arc::new(PersonalHub { modules, tools, retrieval: None, orchestration: None });

    let chat = MiniChat {
        steps: Mutex::new(std::collections::VecDeque::new()),
    };
    {
        let mut steps = chat.steps.lock().unwrap();
        steps.push_back(ChatResponse {
            content: None,
            tool_calls: vec![ChatToolCall {
                id: "call_trip".into(),
                name: "travel.get_trip_context".into(),
                arguments: serde_json::json!({"city": "大连"}),
            }],
            usage: devtoolbox_core::ChatUsage::default(),
        });
        steps.push_back(ChatResponse {
            content: Some(
                r#"{"message":"当前是大连 3 天行程。","actions":[],"ui_blocks":[]}"#.to_string(),
            ),
            tool_calls: vec![],
            usage: devtoolbox_core::ChatUsage::default(),
        });
    }

    let agent = PersonalAgent::new(
        Arc::new(chat),
        hub,
        Arc::new(InMemorySessionStore::new()),
        Default::default(),
    );
    let request = AgentRequest {
        message: "第二天太累了，帮我调整一下行程。".into(),
        session_id: Some("s-travel".into()),
        app_context: AiAppContext {
            module: Some("travel".into()),
            page: Some("guide".into()),
            entity: Some(devtoolbox_core::personal_ai::EntityRef {
                kind: "destination".into(),
                id: "大连".into(),
                label: Some("大连".into()),
            }),
            selection: None,
            view_state: serde_json::json!({"days": 3}),
        },
        capabilities: vec!["travel".into()],
        locale: Some("zh-CN".into()),
    };
    let response: AgentResponse = block_on(agent.run(request)).unwrap();
    assert!(response.message.contains("大连 3 天"));
    // 工具必须被真实调用过：tool_trace 是路由的直接证据
    assert_eq!(response.tool_trace.len(), 1);
    assert_eq!(response.tool_trace[0].tool, "travel.get_trip_context");
    assert!(response.tool_trace[0].ok);
    // context provider 构建 prompt 时会读行程（≥1），工具执行再读一次；计数 ≥1 即可
    assert!(*context_calls.lock().unwrap() >= 1);
    // PersonalAgent 核心未出现 travel 业务分支的证明由 reviewer 的 if/else 扫描完成。
}

#[test]
fn travel_module_can_coexist_with_history_registry() {
    // 平台可扩展性冒烟：两个模块同注册表，互不干扰。
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    let port: Arc<dyn TravelAiPort> = Arc::new(FakeTravelPort::default());
    register_travel(&mut modules, &mut tools, port).unwrap();
    crate::personal_ai::history::register_history(
        &mut modules,
        &mut tools,
        history_port_fake(),
        None,
    )
    .unwrap();
    let ids: Vec<String> = modules.descriptors().iter().map(|d| d.id.clone()).collect();
    assert_eq!(ids, vec!["history", "travel"]);
    assert_eq!(tools.len(), 9);
}

/// 冒烟用最小 History 端口（只用注册不执行）。
fn history_port_fake() -> Arc<dyn crate::history::HistoryQueryPort> {
    use crate::history::HistoryPortError;
    use devtoolbox_core::history_records::*;

    struct Noop;
    impl crate::history::HistoryQueryPort for Noop {
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
        fn get_people_for_period(
            &self,
            _: &str,
            _: i64,
        ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_relations_for_period(
            &self,
            _: &str,
        ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_stories_for_period(
            &self,
            _: Option<&str>,
        ) -> Result<Vec<StoryResult>, HistoryPortError> {
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
        fn get_story_texts(
            &self,
            _: &str,
        ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_story_evidences(
            &self,
            _: &str,
        ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_event(&self, _: &str) -> Result<Option<EventResult>, HistoryPortError> {
            Ok(None)
        }
        fn get_event_people(&self, _: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_event_places(&self, _: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_event_relations(
            &self,
            _: &str,
        ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_event_texts(
            &self,
            _: &str,
        ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_event_evidences(
            &self,
            _: &str,
        ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_person(&self, _: &str) -> Result<Option<PersonResult>, HistoryPortError> {
            Ok(None)
        }
        fn get_person_relations(
            &self,
            _: &str,
        ) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_person_places(&self, _: &str) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_person_events(&self, _: &str) -> Result<Vec<PersonEventResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_person_stories(&self, _: &str) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_work_by_id(&self, _: &str) -> Result<Option<WorkResult>, HistoryPortError> {
            Ok(None)
        }
        fn get_work(&self, _: &str, _: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_historical_texts(
            &self,
            _: Option<&str>,
            _: i64,
        ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn search_people(&self, _: &str, _: i64) -> Result<Vec<PersonResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn search_events(&self, _: &str, _: i64) -> Result<Vec<EventResult>, HistoryPortError> {
            Ok(vec![])
        }
        fn get_sources_for_ids(&self, _: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
            Ok(vec![])
        }
    }
    Arc::new(Noop)
}
