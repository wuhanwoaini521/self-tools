//! Geography 模块测试（V5 §69：标准接入）。
//!
//! 覆盖：模块注册 / 工具发现 / 搜索 / 地点详情 / context provider /
//! PersonalAgent 工具路由（MiniChat 脚本驱动，不触网）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use devtoolbox_core::geography::{
    GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelation, GeoRelationKind, GeoSource,
};
use devtoolbox_core::personal_ai::{
    AgentRequest, AgentResponse, AppContext, ChatModelProvider, ChatRequest, ChatResponse,
    ChatToolCall, EntityRef,
};
use devtoolbox_core::{ToolCallRequest, ToolResult, ToolRisk};

use crate::geography::{GeographyPortError, GeographyQueryPort};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::geography::{
    GeographyProviderOwned, geography_tool_names, register_geography,
};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

// ---------------------------------------------------------------------------
// FakeGeographyPort
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct FakeGeographyPort {
    entities: HashMap<String, GeoEntity>,
    relations: HashMap<String, Vec<GeoRelation>>,
    sources: Vec<GeoSource>,
    pub get_calls: Arc<Mutex<usize>>,
    pub search_calls: Arc<Mutex<usize>>,
}

impl FakeGeographyPort {
    fn everest() -> GeoEntity {
        GeoEntity {
            id: "everest".into(),
            entity_type: GeoEntityType::Mountain,
            name: "珠穆朗玛峰".into(),
            name_en: Some("Mount Everest".into()),
            aliases: vec!["圣母峰".into(), "珠峰".into()],
            coordinates: None,
            geometry: None,
            parent_id: Some("himalayas".into()),
            properties: vec![],
            summary: "世界最高峰，海拔约 8848.86 米，位于喜马拉雅山脉。".into(),
            source_ids: vec!["src_1".into()],
        }
    }

    fn himalayas() -> GeoEntity {
        GeoEntity {
            id: "himalayas".into(),
            entity_type: GeoEntityType::MountainRange,
            name: "喜马拉雅山脉".into(),
            name_en: Some("Himalayas".into()),
            aliases: vec!["喜马拉雅山".into()],
            coordinates: None,
            geometry: None,
            parent_id: None,
            properties: vec![],
            summary: "亚洲主要山脉，由印度板块与欧亚板块碰撞形成。".into(),
            source_ids: vec!["src_1".into()],
        }
    }
}

impl FakeGeographyPort {
    fn with_demo() -> Self {
        let everest = Self::everest();
        let himalayas = Self::himalayas();
        let mut port = FakeGeographyPort::default();
        port.entities.insert(everest.id.clone(), everest);
        port.entities.insert(himalayas.id.clone(), himalayas);
        port.relations.insert(
            "everest".into(),
            vec![GeoRelation {
                from_id: "everest".into(),
                to_id: "himalayas".into(),
                kind: GeoRelationKind::LocatedIn,
                note: Some("主峰位于喜马拉雅山脉".into()),
                source_ids: vec!["src_1".into()],
            }],
        );
        port.sources = vec![GeoSource {
            id: "src_1".into(),
            dataset: "demo".into(),
            version: "1".into(),
            url: "https://example.com/geo/everest".into(),
            license: "demo".into(),
            updated_at: "2026".into(),
            fields: vec![],
        }];
        port
    }
}

impl GeographyQueryPort for FakeGeographyPort {
    fn all_entities(&self) -> Result<Vec<GeoEntity>, GeographyPortError> {
        Ok(self.entities.values().cloned().collect())
    }
    fn recent_ids(&self, _limit: i64) -> Result<Vec<String>, GeographyPortError> {
        Ok(vec![])
    }
    fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError> {
        Ok(vec![])
    }
    fn map_snapshot(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), GeographyPortError> {
        Ok((vec![], vec![]))
    }
    fn search(
        &self,
        query: &str,
        _entity_type: Option<GeoEntityType>,
        _limit: usize,
    ) -> Result<Vec<GeoEntity>, GeographyPortError> {
        *self.search_calls.lock().unwrap() += 1;
        Ok(self
            .entities
            .values()
            .filter(|entity| {
                entity.name.contains(query)
                    || entity
                        .name_en
                        .as_deref()
                        .is_some_and(|value| value.contains(query))
            })
            .cloned()
            .collect())
    }
    fn entity(&self, id: &str) -> Result<Option<GeoEntity>, GeographyPortError> {
        *self.get_calls.lock().unwrap() += 1;
        Ok(self.entities.get(id).cloned())
    }
    fn record_view(&self, _id: &str) -> Result<(), GeographyPortError> {
        Ok(())
    }
    fn relations_for(&self, id: &str) -> Result<Vec<GeoRelation>, GeographyPortError> {
        Ok(self.relations.get(id).cloned().unwrap_or_default())
    }
    fn sources(&self, ids: &[String]) -> Result<Vec<GeoSource>, GeographyPortError> {
        Ok(self
            .sources
            .iter()
            .filter(|source| ids.contains(&source.id))
            .cloned()
            .collect())
    }
    fn toggle_favorite(&self, _id: &str) -> Result<bool, GeographyPortError> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// 工具（block_on 驱动 async ToolExecutor）
// ---------------------------------------------------------------------------

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn registered() -> (
    ModuleRegistry,
    ToolRegistry,
    Arc<dyn GeographyQueryPort + Send + Sync>,
) {
    let port: Arc<dyn GeographyQueryPort + Send + Sync> = Arc::new(FakeGeographyPort::with_demo());
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_geography(&mut modules, &mut tools, Arc::clone(&port)).unwrap();
    (modules, tools, port)
}

fn call_tool(tools: &ToolRegistry, name: &str, args: serde_json::Value) -> ToolResult {
    block_on(tools.execute(&ToolCallRequest {
        id: "t".into(),
        name: name.into(),
        arguments: args,
    }))
    .unwrap()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[test]
fn module_registered_with_four_read_tools() {
    let (modules, tools, _port) = registered();
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "geography");
    assert_eq!(
        descriptors[0].capabilities,
        vec!["search", "entity", "exploration"]
    );
    assert_eq!(descriptors[0].tools.len(), 4);
    assert_eq!(tools.len(), 4);
    for name in geography_tool_names() {
        let spec = tools.spec(name).expect("missing tool");
        assert_eq!(spec.risk, ToolRisk::Read);
        assert_eq!(spec.module, "geography");
    }
    // 未知工具受控拒绝
    let unknown = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "geography.ghost".into(),
        arguments: serde_json::json!({}),
    }))
    .unwrap_err();
    assert_eq!(unknown.code(), "personal_ai_tool_not_found");
}

#[test]
fn geography_search_returns_matches() {
    let (_modules, tools, _port) = registered();
    let result = call_tool(
        &tools,
        "geography.search",
        serde_json::json!({"query": "珠穆朗玛"}),
    );
    assert!(result.ok);
    let hits = result.data.as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], "everest");
    assert_eq!(hits[0]["name"], "珠穆朗玛峰");
    assert_eq!(result.metadata["count"], 1);
}

#[test]
fn geography_get_location_and_context() {
    let (_modules, tools, _port) = registered();
    // location
    let location = call_tool(
        &tools,
        "geography.get_location",
        serde_json::json!({"id": "everest"}),
    );
    assert!(location.ok);
    assert_eq!(location.data["name"], "珠穆朗玛峰");
    assert_eq!(location.data["entity_type"], "山体");
    assert_eq!(location.data["parent_id"], "himalayas");
    // missing
    let missing = call_tool(
        &tools,
        "geography.get_location",
        serde_json::json!({"id": "nope"}),
    );
    assert!(!missing.ok);
    assert!(missing.error.unwrap().contains("不存在"));
    // context 含关系与来源
    let context = call_tool(
        &tools,
        "geography.get_context",
        serde_json::json!({"id": "everest"}),
    );
    assert!(context.ok);
    let relations = context.data["relations"].as_array().unwrap();
    assert!(!relations.is_empty());
    assert_eq!(relations[0]["to_id"], "himalayas");
    assert!(!context.data["sources"].as_array().unwrap().is_empty());
    // validation: 缺 id → 参数错误
    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "geography.get_location".into(),
        arguments: serde_json::json!({}),
    }))
    .unwrap_err();
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
}

#[test]
fn context_provider_resolves_location() {
    let (_modules, _tools, port) = registered();
    let provider = GeographyProviderOwned::new(port);
    let ctx = AppContext {
        module: Some("geography".into()),
        page: Some("explore".into()),
        entity: Some(EntityRef {
            kind: "location".into(),
            id: "everest".into(),
            label: Some("珠穆朗玛峰".into()),
        }),
        selection: None,
        view_state: serde_json::Value::Null,
    };
    let bundle = provider
        .build_context(&ctx, &ContextBudget::default())
        .unwrap();
    assert_eq!(bundle.module, "geography");
    assert!(bundle.headline.contains("珠穆朗玛峰"));
    assert_eq!(bundle.summary["canonical"]["entity_type"], "山体");
    assert!(!bundle.summary["relations"].as_array().unwrap().is_empty());
    assert!(bundle.summary["sources_count"].is_number());
}

#[test]
fn context_provider_without_entity_is_graceful() {
    let (_modules, _tools, port) = registered();
    let provider = GeographyProviderOwned::new(port);
    let ctx = AppContext {
        module: Some("geography".into()),
        ..AppContext::default()
    };
    let bundle = provider
        .build_context(&ctx, &ContextBudget::default())
        .unwrap();
    assert_eq!(bundle.headline, "Geography");
}

// ---------------------------------------------------------------------------
// PersonalAgent 路由（V5 §69：agent 无需业务 hardcode 即可调 geography 工具）
// ---------------------------------------------------------------------------

struct MiniChat {
    steps: Mutex<std::collections::VecDeque<ChatResponse>>,
}

#[async_trait]
impl ChatModelProvider for MiniChat {
    fn name(&self) -> &'static str {
        "mini"
    }
    async fn chat(
        &self,
        _request: ChatRequest,
    ) -> Result<ChatResponse, devtoolbox_core::ProviderError> {
        Ok(self.steps.lock().unwrap().pop_front().unwrap())
    }
}

#[tokio::test]
async fn personal_agent_routes_geography_tool() {
    use crate::personal_ai::agent::{PersonalAgent, PersonalHub};
    use crate::personal_ai::session::InMemorySessionStore;

    // 通过共享计数器验证工具被调用
    let fake: Arc<FakeGeographyPort> = Arc::new(FakeGeographyPort::with_demo());
    let port: Arc<dyn GeographyQueryPort + Send + Sync> = fake.clone();
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_geography(&mut modules, &mut tools, port).unwrap();
    let hub = Arc::new(PersonalHub { modules, tools, retrieval: None, orchestration: None });

    let chat = MiniChat {
        steps: Mutex::new(std::collections::VecDeque::new()),
    };
    chat.steps.lock().unwrap().push_back(ChatResponse {
        content: None,
        reasoning_content: None,
        tool_calls: vec![ChatToolCall {
            id: "call_1".into(),
            name: "geography.get_location".into(),
            arguments: serde_json::json!({"id": "everest"}),
        }],
        usage: devtoolbox_core::ChatUsage::default(),
    });
    chat.steps.lock().unwrap().push_back(ChatResponse {
        content: Some(
            r#"{"message":"珠穆朗玛峰是世界最高峰，海拔约 8848.86 米。","actions":[],"ui_blocks":[]}"#
                .to_string(),
        ),
        reasoning_content: None,
        tool_calls: vec![],
        usage: devtoolbox_core::ChatUsage::default(),
    });

    let agent = PersonalAgent::new(
        Arc::new(chat),
        hub,
        Arc::new(InMemorySessionStore::new()),
        Default::default(),
    );
    let request = AgentRequest {
        message: "珠穆朗玛峰有多高？".into(),
        session_id: Some("s1".into()),
        app_context: AppContext {
            module: Some("geography".into()),
            page: Some("explore".into()),
            entity: Some(EntityRef {
                kind: "location".into(),
                id: "everest".into(),
                label: Some("珠穆朗玛峰".into()),
            }),
            selection: None,
            view_state: serde_json::Value::Null,
        },
        capabilities: vec!["geography".into()],
        locale: Some("zh-CN".into()),
        parts: Vec::new(),
    };
    let response: AgentResponse = agent.run(request).await.unwrap();
    assert!(response.message.contains("珠穆朗玛峰"));
    // 工具/上下文读取过 fake（≥1：系统组装读取上下文 + 工具执行读取实体）
    assert!(*fake.get_calls.lock().unwrap() >= 1);
    assert_eq!(response.tool_trace.len(), 1);
    assert_eq!(response.tool_trace[0].tool, "geography.get_location");
    assert!(response.tool_trace[0].ok);
}
