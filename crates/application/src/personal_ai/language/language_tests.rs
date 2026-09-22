//! Language 模块测试（V5 §70：注册 / 选中文本 context / explain / agent 路由）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use devtoolbox_core::language::{
    DatasetManifest, LanguageCode, LanguageItem, LanguageItemType, LanguageSource, LearningState,
    LearningStateKind, Meaning, ReviewOutcome, ReviewRating, SentenceRecord,
};
use devtoolbox_core::personal_ai::{
    AppContext as AiAppContext, ChatModelProvider, ChatRequest, ChatResponse, ChatToolCall,
    EntityRef,
};
use devtoolbox_core::{AgentRequest, AgentResponse, ToolCallRequest, ToolRisk};

use crate::language::ports::{
    LanguageCount, LanguageDetailRows, LanguageExample, LanguageStorePort, SearchHitModel,
};
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::language::{LanguageProviderOwned, language_tool_names, register_language};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

// ---------------------------------------------------------------------------
// Fake LanguageStore（内存；可记录调用次数）
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct FakeLanguageStore {
    items: HashMap<String, LanguageItem>,
    meanings: HashMap<String, Vec<Meaning>>,
    examples: HashMap<String, Vec<LanguageExample>>,
    sentences: HashMap<String, Vec<SentenceRecord>>,
    due_queue: Mutex<std::collections::VecDeque<LanguageItem>>,
    pub detail_calls: AtomicUsize,
    pub review_calls: AtomicUsize,
}

impl FakeLanguageStore {
    pub fn with_word() -> Self {
        let mut store = Self::default();
        let id = "jmdict:1002990".to_string();
        store.items.insert(
            id.clone(),
            LanguageItem {
                id: id.clone(),
                language: LanguageCode::Jap,
                item_type: LanguageItemType::Word,
                text: "食べる".to_string(),
                reading: Some("たべる".to_string()),
                romanization: Some("taberu".to_string()),
                meta: None,
                source: "jmdict".to_string(),
            },
        );
        store.meanings.insert(
            id.clone(),
            vec![Meaning {
                id: "m1".into(),
                item_id: id.clone(),
                pos: Some("v1".into()),
                gloss: Some("to eat".into()),
                raw: None,
                sense_key: None,
                lang: None,
                rank: 0,
                source: "jmdict".to_string(),
            }],
        );
        store.examples.insert(
            id.clone(),
            vec![LanguageExample {
                text: "ご飯を食べる。".into(),
                translation: Some("吃饭。".into()),
                source: "tatoeba".into(),
            }],
        );
        store.sentences.insert(
            id.clone(),
            vec![SentenceRecord {
                sentence_id: "s1".into(),
                language: LanguageCode::Jap,
                text: "ご飯を食べる。".into(),
                author: None,
                license: "CC0".into(),
                source: "tatoeba".into(),
            }],
        );
        store
            .due_queue
            .lock()
            .unwrap()
            .push_back(store.items.get(&id).cloned().unwrap());
        store
    }
}

impl LanguageStorePort for FakeLanguageStore {
    fn language_counts(&self) -> Result<Vec<LanguageCount>, String> {
        Ok(vec![LanguageCount {
            language: LanguageCode::Jap,
            words: self.items.len() as i64,
            phrases: 0,
            sentences: self.sentences.values().map(|v| v.len() as i64).sum(),
            total: self.items.len() as i64,
        }])
    }
    fn search(
        &self,
        _language: Option<LanguageCode>,
        query: &str,
        _limit: usize,
    ) -> Result<Vec<SearchHitModel>, String> {
        Ok(self
            .items
            .values()
            .filter(|item| item.text.contains(query))
            .map(|item| SearchHitModel {
                item: item.clone(),
                matched: "fuzzy".into(),
            })
            .collect())
    }
    fn item_detail(&self, id: &str) -> Result<LanguageDetailRows, String> {
        self.detail_calls.fetch_add(1, Ordering::SeqCst);
        Ok(LanguageDetailRows {
            item: self.items.get(id).cloned(),
            meanings: self.meanings.get(id).cloned().unwrap_or_default(),
            pronunciations: vec![],
            relations: vec![],
            related_items: vec![],
            examples: self.examples.get(id).cloned().unwrap_or_default(),
            sentences: self.sentences.get(id).cloned().unwrap_or_default(),
            state: None,
            favorite: false,
            extra: None,
        })
    }
    fn source_by_id(&self, _id: &str) -> Result<Option<LanguageSource>, String> {
        Ok(None)
    }
    fn today_plan(
        &self,
        _language: LanguageCode,
        _now: i64,
    ) -> Result<devtoolbox_core::language::TodayPlan, String> {
        // 同一表达式内对同一 Mutex 两次 lock() 会死锁（guard 存活到语句结束），一次取值。
        let due = self.due_queue.lock().unwrap().len() as i64;
        Ok(devtoolbox_core::language::TodayPlan {
            due_reviews: due,
            new_words: 0,
            sentences: self.sentences.values().map(|v| v.len() as i64).sum(),
            listening: 0,
            speaking: 0,
            total: due,
        })
    }
    fn review_next(
        &self,
        _language: LanguageCode,
        _now: i64,
    ) -> Result<Option<LanguageItem>, String> {
        self.review_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.due_queue.lock().unwrap().pop_front())
    }
    fn learning_state(&self, _item_id: &str) -> Result<Option<LearningState>, String> {
        Ok(None)
    }
    fn rate_review(
        &self,
        _item_id: &str,
        _rating: ReviewRating,
        _now: i64,
    ) -> Result<ReviewOutcome, String> {
        Ok(ReviewOutcome {
            state: LearningStateKind::Review,
            interval_days: 1.0,
            ease: 2.5,
            due_at: _now,
            lapses: 0,
        })
    }
    fn toggle_favorite(&self, _item_id: &str, _now: i64) -> Result<bool, String> {
        Ok(true)
    }
    fn favorites(&self, _limit: usize) -> Result<Vec<LanguageItem>, String> {
        Ok(self.items.values().cloned().collect())
    }
    fn set_learning_state(
        &self,
        _item_id: &str,
        _state: LearningStateKind,
        _now: i64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn progress(&self) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({}))
    }
    fn favorites_count(&self) -> Result<i64, String> {
        Ok(self.items.len() as i64)
    }
    fn sources(&self) -> Result<Vec<LanguageSource>, String> {
        Ok(vec![])
    }
    fn manifests(&self) -> Result<Vec<DatasetManifest>, String> {
        Ok(vec![])
    }
    fn count_by_source(&self, _source_id: &str) -> Result<i64, String> {
        Ok(self.items.len() as i64)
    }
    fn sentences_by_language(
        &self,
        _language: LanguageCode,
        _limit: usize,
    ) -> Result<Vec<SentenceRecord>, String> {
        Ok(self.sentences.values().flatten().cloned().collect())
    }
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn registered_with_llm(
    llm: Option<Arc<dyn ChatModelProvider>>,
) -> (ModuleRegistry, ToolRegistry, Arc<FakeLanguageStore>) {
    let inner = Arc::new(FakeLanguageStore::with_word());
    let port: Arc<dyn LanguageStorePort> = inner.clone();
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_language(&mut modules, &mut tools, port, llm).unwrap();
    (modules, tools, inner)
}

fn find_spec(tools: &ToolRegistry, name: &str) -> devtoolbox_core::ToolSpec {
    tools.spec(name).cloned().expect("tool registered")
}

/// 注册后 4 工具齐全、全部 Read；未知工具受控拒绝。
#[test]
fn registration_exposes_four_read_tools() {
    let (modules, tools, _store) = registered_with_llm(None);
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "language");
    assert_eq!(descriptors[0].tools.len(), 4);
    for name in language_tool_names() {
        let spec = find_spec(&tools, name);
        assert_eq!(spec.module, "language");
        assert_eq!(spec.risk, ToolRisk::Read);
    }
    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.ghost".into(),
        arguments: serde_json::json!({}),
    }))
    .unwrap_err();
    assert_eq!(error.code(), "personal_ai_tool_not_found");
}

/// language.get_context 读取词条（id）→ 紧凑上下文。
#[test]
fn get_context_reads_item() {
    let (_modules, tools, _store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.get_context".into(),
        arguments: serde_json::json!({"id": "jmdict:1002990"}),
    }))
    .unwrap();
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["item"]["text"], "食べる");
    assert_eq!(result.data["item"]["reading"], "たべる");
    assert_eq!(result.data["language"], "jpn");
    assert_eq!(result.data["meanings_count"], 1);
    assert_eq!(result.data["examples_count"], 1);
}

/// 未知词条 → 受控失败（不是 panic）。
#[test]
fn get_context_missing_item_is_controlled_failure() {
    let (_modules, tools, _store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.get_context".into(),
        arguments: serde_json::json!({"id": "nope"}),
    }))
    .unwrap();
    assert!(!result.ok);
    assert!(result.error.unwrap().contains("不存在"));
}

/// language.explain 无模型时：词典数据先行，绝不编造（无 AI section、llm_used=false）。
#[test]
fn explain_without_llm_is_dictionary_sourced() {
    let (_modules, tools, _store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.explain".into(),
        arguments: serde_json::json!({"item_id": "jmdict:1002990", "question": "这里为什么用 は?"}),
    }))
    .unwrap();
    assert!(result.ok);
    let sections = result.data["explanation_sections"].as_array().unwrap();
    assert!(
        sections
            .iter()
            .any(|s| s["title"] == "权威词典释义" && s["kind"] == "dictionary")
    );
    assert!(sections.iter().any(|s| s["kind"] == "dictionary"));
    assert!(
        !sections.iter().any(|s| s["kind"] == "ai"),
        "no LLM → no fabricated AI section"
    );
    assert_eq!(result.data["llm_used"], false);
}

/// language.generate_examples：只读已收录例句。
#[test]
fn generate_examples_uses_stored_only() {
    let (_modules, tools, _store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.generate_examples".into(),
        arguments: serde_json::json!({"item_id": "jmdict:1002990", "count": 2}),
    }))
    .unwrap();
    assert!(result.ok);
    let examples = result.data["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 2); // 1 stored example + 1 stored sentence
    assert!(examples.iter().any(|e| e["text"] == "ご飯を食べる。"));
}

/// language.practice：待复习队列（只读，不改状态）。
#[test]
fn practice_returns_due_queue() {
    let (_modules, tools, store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.practice".into(),
        arguments: serde_json::json!({"language": "jpn", "limit": 5}),
    }))
    .unwrap();
    assert!(result.ok);
    let due = result.data["due"].as_array().unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0]["id"], "jmdict:1002990");
    // review 只被消费，不影响 canonical（这里无写路径）。
    assert!(store.review_calls.load(Ordering::SeqCst) >= 1);
}

/// 异常请求 → 受控错误码（参数校验）。
#[test]
fn invalid_args_never_reach_tool() {
    let (_modules, tools, _store) = registered_with_llm(None);
    let error = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.explain".into(),
        arguments: serde_json::json!({}), // 缺 item_id
    }))
    .unwrap_err();
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
}

// ---------------------------------------------------------------------------
// ContextProvider（§53：选中句子指代解析）
// ---------------------------------------------------------------------------

#[test]
fn context_provider_resolves_selected_word() {
    let store = Arc::new(FakeLanguageStore::with_word());
    let provider = LanguageProviderOwned::new(store, None);
    let ctx = AiAppContext {
        module: Some("language".into()),
        page: Some("today".into()),
        entity: Some(EntityRef {
            kind: "word".into(),
            id: "jmdict:1002990".into(),
            label: Some("食べる".into()),
        }),
        selection: None,
        view_state: serde_json::Value::Null,
    };
    let bundle = provider
        .build_context(&ctx, &ContextBudget::default())
        .unwrap();
    assert_eq!(bundle.module, "language");
    assert!(bundle.headline.contains("食べる"));
    assert_eq!(bundle.summary["language"], "jpn");
    assert_eq!(bundle.summary["examples_count"], 1);
}
// ---------------------------------------------------------------------------
// PersonalAgent 路由（§70：language 通过标准模块机制被 Agent 调用）
// ---------------------------------------------------------------------------

/// 可编程模型（与 history 测试同型）。
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
        Ok(self.steps.lock().unwrap().pop_front().unwrap())
    }
}

#[tokio::test]
async fn personal_agent_route_language() {
    use crate::personal_ai::agent::{PersonalAgent, PersonalHub};
    use crate::personal_ai::session::InMemorySessionStore;

    let (modules, tools, store) = registered_with_llm(None);
    let hub = Arc::new(PersonalHub { modules, tools, retrieval: None, orchestration: None });
    let chat = MiniChat {
        steps: Mutex::new(std::collections::VecDeque::new()),
    };
    chat.steps.lock().unwrap().push_back(ChatResponse {
        content: None,
        tool_calls: vec![ChatToolCall {
            id: "call_1".into(),
            name: "language.get_context".into(),
            arguments: serde_json::json!({"id": "jmdict:1002990"}),
        }],
        usage: devtoolbox_core::ChatUsage::default(),
    });
    chat.steps.lock().unwrap().push_back(ChatResponse {
        content: Some(
            r#"{"message":"食べる 是「吃」的意思。","actions":[],"ui_blocks":[]}"#.to_string(),
        ),
        tool_calls: vec![],
        usage: devtoolbox_core::ChatUsage::default(),
    });
    let agent = PersonalAgent::new(
        Arc::new(chat),
        hub,
        Arc::new(InMemorySessionStore::new()),
        Default::default(),
    );
    let response: AgentResponse = agent
        .run(AgentRequest {
            message: "这个单词什么意思？".into(),
            session_id: Some("s1".into()),
            app_context: AiAppContext {
                module: Some("language".into()),
                page: Some("today".into()),
                entity: Some(EntityRef {
                    kind: "word".into(),
                    id: "jmdict:1002990".into(),
                    label: Some("食べる".into()),
                }),
                selection: None,
                view_state: serde_json::Value::Null,
            },
            capabilities: vec!["language".into()],
            locale: Some("zh-CN".into()),
        })
        .await
        .unwrap();
    assert!(response.message.contains("食べる"));
    // context provider 预读也会访问 store（detail_calls≥1）；工具执行以 tool_trace 精确断言
    assert!(
        store.detail_calls.load(Ordering::SeqCst) >= 1,
        "agent path consulted the language store"
    );
    assert_eq!(
        response.tool_trace.len(),
        1,
        "exactly one tool call routed to language module"
    );
    assert_eq!(response.tool_trace[0].tool, "language.get_context");
    assert!(response.tool_trace[0].ok);
}
