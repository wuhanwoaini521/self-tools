//! Language 模块测试（V5 §70：注册 / 选中文本 context / explain / agent 路由）。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;

use devtoolbox_core::language::{
    DatasetManifest, Difficulty, LanguageCode, LanguageItem, LanguageItemType,
    LanguageLearningItem, LanguageSource, Lesson, LessonPosition, Meaning, Mistake, SentenceRecord,
    SentenceStudy,
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
    mistakes: Mutex<Vec<Mistake>>,
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
        store.mistakes.lock().push(Mistake {
            id: "card_mistake_1".into(),
            item_id: id.clone(),
            item_type: devtoolbox_core::language::LearningItemType::Word,
            language: LanguageCode::Jap,
            content: "食べる".into(),
            question: "「to eat」对应哪个词？".into(),
            user_answer: "飲む".into(),
            correct_answer: "食べる".into(),
            error_count: 1,
            last_missed_at: 1_700_000_000,
        });
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
            extra: None,
        })
    }
    fn source_by_id(&self, _id: &str) -> Result<Option<LanguageSource>, String> {
        Ok(None)
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

    // ---- 学习内容（本 AI 工具面只读错题与句子；不写任何学习状态）----

    fn learning_item(&self, item_id: &str) -> Result<Option<LanguageLearningItem>, String> {
        let Some(item) = self.items.get(item_id).cloned() else {
            return Ok(None);
        };
        let meaning = self
            .meanings
            .get(item_id)
            .and_then(|meanings| meanings.first())
            .and_then(|meaning| meaning.gloss.clone());
        Ok(LanguageLearningItem::from_item(
            &item,
            meaning,
            item.reading.clone(),
            Difficulty::Unknown,
        ))
    }
    fn learning_items(&self, item_ids: &[String]) -> Result<Vec<LanguageLearningItem>, String> {
        Ok(item_ids
            .iter()
            .filter_map(|id| self.learning_item(id).ok().flatten())
            .collect())
    }
    fn sentence_study(&self, sentence_id: &str) -> Result<Option<SentenceStudy>, String> {
        let Some(first) = self
            .sentences
            .get(sentence_id)
            .and_then(|sentences| sentences.first())
        else {
            return Ok(None);
        };
        Ok(Some(SentenceStudy::new(
            sentence_id,
            first.language,
            first.text.clone(),
        )))
    }
    fn next_new_items(
        &self,
        _language: LanguageCode,
        _exclude: &[String],
        _limit: usize,
    ) -> Result<Vec<devtoolbox_core::language::LanguageLearningItem>, String> {
        Ok(Vec::new())
    }
    fn upsert_lesson(&self, _lesson: &Lesson) -> Result<(), String> {
        Ok(())
    }
    fn lesson(&self, _lesson_id: &str) -> Result<Option<Lesson>, String> {
        Ok(None)
    }
    fn lessons(
        &self,
        _language: Option<LanguageCode>,
        _limit: usize,
    ) -> Result<Vec<Lesson>, String> {
        Ok(vec![])
    }
    fn delete_lesson(&self, _lesson_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn save_lesson_position(&self, _position: &LessonPosition) -> Result<(), String> {
        Ok(())
    }
    fn lesson_position(&self, _lesson_id: &str) -> Result<Option<LessonPosition>, String> {
        Ok(None)
    }
    fn recent_lesson_positions(&self, _limit: usize) -> Result<Vec<LessonPosition>, String> {
        Ok(vec![])
    }
    fn record_mistake(&self, _mistake: &Mistake, _card_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, String> {
        self.review_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.mistakes.lock().iter().take(limit).cloned().collect())
    }
    fn resolve_mistake(&self, _item_id: &str, _card_id: &str) -> Result<(), String> {
        Ok(())
    }
    fn mistake_count(&self) -> Result<i64, String> {
        Ok(self.mistakes.lock().len() as i64)
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

/// language.practice：返回真实待复习错题（只读，不改状态）。
#[test]
fn practice_returns_pending_mistakes() {
    let (_modules, tools, store) = registered_with_llm(None);
    let result = block_on(tools.execute(&ToolCallRequest {
        id: "c".into(),
        name: "language.practice".into(),
        arguments: serde_json::json!({"language": "jpn", "limit": 5}),
    }))
    .unwrap();
    assert!(result.ok);
    let mistakes = result.data["mistakes"].as_array().unwrap();
    assert_eq!(mistakes.len(), 1, "应返回夹具里的那条错题");
    assert_eq!(mistakes[0]["id"], "jmdict:1002990");
    assert_eq!(mistakes[0]["text"], "食べる");
    assert_eq!(mistakes[0]["error_count"], 1);
    // practice 是只读的：错题只被读取，不被消费
    assert!(store.review_calls.load(Ordering::SeqCst) >= 1);
    assert_eq!(store.mistake_count().unwrap(), 1, "出题不应消耗错题");
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
        Ok(self.steps.lock().pop_front().unwrap())
    }
}

#[tokio::test]
async fn personal_agent_route_language() {
    use crate::personal_ai::agent::{PersonalAgent, PersonalHub};
    use crate::personal_ai::session::InMemorySessionStore;

    let (modules, tools, store) = registered_with_llm(None);
    let hub = Arc::new(PersonalHub {
        modules,
        tools,
        retrieval: None,
        orchestration: None,
    });
    let chat = MiniChat {
        steps: Mutex::new(std::collections::VecDeque::new()),
    };
    chat.steps.lock().push_back(ChatResponse {
        content: None,
        reasoning_content: None,
        tool_calls: vec![ChatToolCall {
            id: "call_1".into(),
            name: "language.get_context".into(),
            arguments: serde_json::json!({"id": "jmdict:1002990"}),
        }],
        usage: devtoolbox_core::ChatUsage::default(),
    });
    chat.steps.lock().push_back(ChatResponse {
        content: Some(
            r#"{"message":"食べる 是「吃」的意思。","actions":[],"ui_blocks":[]}"#.to_string(),
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
            parts: Vec::new(),
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
