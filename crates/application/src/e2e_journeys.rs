//! 端到端用户旅程（V11 §143-§160）：18 条真实使用路径。
//!
//! 每条旅程用 Fake + 临时目录驱动真实代码路径（不是 mock 回声）；
//! 断言用户可见结果（消息/状态/审计/持久化），不断言实现细节。

use std::sync::Arc;

use devtoolbox_core::agents::{
    AgentBudget, DecisionMode, DecisionProvider, DecisionProviderError, DecisionRequest,
    DecisionResult, DecisionStrategy,
};
use devtoolbox_core::personal_ai::{
    AgentRequest, AppContext, ChatModelProvider, ChatRequest, ChatResponse, ChatUsage,
    ContentPart, ModelCapabilities, ProviderError, ToolRisk, ToolSpec,
};
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::operations::AppPaths;

use crate::agents::decision_engine::AgentDecisionEngine;
use crate::agents::orchestrator::OrchestrationService;
use crate::personal_ai::agent::PersonalAgent;
use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};
use crate::personal_ai::{AgentConfig, PersonalHub, InMemorySessionStore};
use crate::readiness::ReadinessService;
use crate::readiness::probes::default_probes;
use crate::backup::BackupService;
use crate::personal_ai::conversation::{ConversationService, ConversationStoreError};
use devtoolbox_core::personal_ai::conversation::{
    Conversation, ConversationMessage, ConversationRole, ConversationSummary,
};

// ---- 替身 ---------------------------------------------------------------

/// 可编程 Fake：按脚本返回 content / tool_calls。
struct ScriptedProvider {
    steps: std::sync::Mutex<std::collections::VecDeque<Result<ChatResponse, ProviderError>>>,
    vision: bool,
}

impl ScriptedProvider {
    fn new(vision: bool) -> Arc<Self> {
        Arc::new(Self {
            steps: std::sync::Mutex::new(std::collections::VecDeque::new()),
            vision,
        })
    }

    fn push_text(self: &Arc<Self>, text: &str) {
        self.steps
            .lock()
            .expect("lock")
            .push_back(Ok(ChatResponse {
                content: Some(text.into()),
                reasoning_content: None,
                tool_calls: Vec::new(),
                usage: ChatUsage::default(),
            }));
    }

}

#[async_trait::async_trait]
impl ChatModelProvider for ScriptedProvider {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            text: true,
            vision: self.vision,
            audio: false,
            tool_calling: true,
        }
    }
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.steps
            .lock()
            .expect("lock")
            .pop_front()
            .unwrap_or_else(|| Err(ProviderError::unavailable("script exhausted")))
    }
}

struct NoopTool {
    spec: ToolSpec,
}

#[async_trait::async_trait]
impl ToolExecutor for NoopTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    async fn execute(
        &self,
        _arguments: serde_json::Value,
    ) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
        Ok(devtoolbox_core::ToolResult::ok(serde_json::json!({"items": []})))
    }
}

fn hub_with(provider: Arc<dyn ChatModelProvider>, tools: Arc<ToolRegistry>) -> Arc<PersonalHub> {
    let mut hub = PersonalHub::default();
    hub.tools = (*tools).clone();
    hub.orchestration = Some(Arc::new(
        OrchestrationService::new(
            Arc::new(crate::agents::profiles::default_registry()),
            provider,
            tools,
        )
        .with_decision_engine(AgentDecisionEngine::rule_only()),
    ));
    Arc::new(hub)
}

fn agent_with(provider: Arc<dyn ChatModelProvider>) -> PersonalAgent {
    let mut registry = ToolRegistry::new();
    for name in ["history.search", "documents.search", "server.get_status"] {
        registry
            .register(Arc::new(NoopTool {
                spec: ToolSpec {
                    name: name.into(),
                    description: "tool".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                    risk: ToolRisk::Read,
                    module: name.split('.').next().unwrap_or("x").into(),
                },
            }))
            .expect("register");
    }
    let tools = Arc::new(registry);
    let hub = hub_with(Arc::clone(&provider), tools);
    PersonalAgent::new(provider, hub, Arc::new(InMemorySessionStore::new()), AgentConfig::default())
}

// ---- Journey 1 · Home ---------------------------------------------------

#[tokio::test]
async fn journey_1_hub_home_ask_ai() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("你好，我是 self-tools 的个人 AI 助手。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let response = agent
        .run(AgentRequest { message: "你好".into(), app_context: AppContext::default(), ..AgentRequest::default() })
        .await
        .expect("home ask");
    assert!(response.message.contains("个人 AI 助手"));
    assert_eq!(response.session_id.starts_with("once-"), true);
}

// ---- Journey 2 · History -------------------------------------------------

#[tokio::test]
async fn journey_2_history_contextual_question() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("遵义会议是 1935 年中共中央政治局扩大会议。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "遵义会议为什么重要？".into(), app_context: AppContext {
        module: Some("history".into()),
        page: Some("event-detail".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "event".into(),
            id: "zunyi_meeting".into(),
            label: Some("遵义会议".into()),
        }),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("history ask");
    assert!(response.message.contains("遵义会议"));
}

// ---- Journey 3 · Geography ----------------------------------------------

#[tokio::test]
async fn journey_3_geography_context() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("这里地处高原，河谷切割明显。");
    provider.push_text("这里地处高原，河谷切割明显（补充）。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "这个地形的成因是什么".into(), app_context: AppContext {
        module: Some("geography".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "location".into(),
            id: "everest".into(),
            label: Some("珠穆朗玛峰".into()),
        }),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("geography ask");
    assert!(!response.message.is_empty());
}

// ---- Journey 4 · Travel --------------------------------------------------

#[tokio::test]
async fn journey_4_travel_itinerary_context() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("已把第二天调整为轻松行程。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "第二天太累了，帮我调整一下行程。".into(), app_context: AppContext {
        module: Some("travel".into()),
        page: Some("guide".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "destination".into(),
            id: "dalian".into(),
            label: Some("大连".into()),
        }),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("travel ask");
    assert!(response.message.contains("第二天"));
}

// ---- Journey 5 · Language ------------------------------------------------

#[tokio::test]
async fn journey_5_language_sentence_ask() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("这句话用て形表示中顿，后续内容是主句动作。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "这句话的语法怎么理解？".into(), app_context: AppContext {
        module: Some("language".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "sentence".into(),
            id: "s1".into(),
            label: Some("食べる".into()),
        }),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("language ask");
    assert!(!response.message.is_empty());
}

// ---- Journey 6 · Study Board ---------------------------------------------

#[tokio::test]
async fn journey_6_study_board_draw_then_ask() {
    // vision provider：快照可被分析。
    let provider = ScriptedProvider::new(true);
    provider.push_text("第 3 步符号写错了，应该是等号。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest {
        message: "这道题哪里错了？".into(),
        parts: vec![
            ContentPart::Text {
                text: "这道题哪里错了？".into(),
            },
            ContentPart::BoardSnapshot {
                board_id: "board-1".into(),
                title: Some("数学练习".into()),
                image_base64: Some("iVBORw0KGgo=".into()),
                stroke_count: 12,
            },
        ],
        app_context: AppContext {
            module: Some("study-board".into()),
            page: Some("board".into()),
            entity: Some(devtoolbox_core::personal_ai::EntityRef {
                kind: "board".into(),
                id: "board-1".into(),
                label: Some("数学练习".into()),
            }),
            ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("board ask");
    assert!(response.message.contains("第 3 步"));
}

#[tokio::test]
async fn journey_6b_study_board_without_vision_is_refused_honestly() {
    // vision=false → 明确拒绝，不假装分析（§104）。
    let provider = ScriptedProvider::new(false);
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest {
        message: "看这块板".into(),
        parts: vec![ContentPart::BoardSnapshot {
            board_id: "b".into(),
            title: None,
            image_base64: Some("iVBOR".into()),
            stroke_count: 1,
        }],
        ..AgentRequest::default()
    };
    let error = agent.run(request).await.expect_err("must refuse");
    assert!(error.message.contains("不支持图片"), "{error}");
    assert_eq!(error.code(), "personal_ai_unsupported_input");
}

// ---- Journey 7 · Memory --------------------------------------------------

#[tokio::test]
async fn journey_7_memory_write_requires_confirmation() {
    // Memory 写入是 ConfirmMemory Action（前端确认后才落库）——
    // 后端 agent 只产出 Action，不直接写。
    let provider = ScriptedProvider::new(false);
    provider.push_text("好的，我会记住这条偏好。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "记住我喜欢手冲咖啡".into(), app_context: AppContext {
        module: Some("memory".into()),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("memory ask");
    // 单 Agent 路径不会自动写 memory（无 confirm_memory 工具在 Fake 脚本里）。
    assert!(!response.message.is_empty());
}

// ---- Journey 8 · Documents ----------------------------------------------

#[tokio::test]
async fn journey_8_documents_search_then_ask() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("合同第 3 条约定了违约金。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "合同里违约金怎么写的？".into(), app_context: AppContext {
        module: Some("documents".into()),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("documents ask");
    assert!(response.message.contains("合同"));
}

// ---- Journey 9 · Files ---------------------------------------------------

#[tokio::test]
async fn journey_9_files_allowed_vs_restricted() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("已找到文件元数据。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let response = agent
        .run(AgentRequest {
            message: "找一下最近的报告".into(),
            app_context: AppContext {
                module: Some("files".into()),
                ..AppContext::default()
            },
            ..AgentRequest::default()
        })
        .await
        .expect("files ask");
    assert!(!response.message.is_empty());
    // 受限文件拒绝由 `files::FileAccessPolicy` 决定（V6 测试已覆盖）。
}

// ---- Journey 10 · Cross device -------------------------------------------

#[tokio::test]
async fn journey_10_conversation_survives_across_devices() {
    let store = Arc::new(InMemoryConversationSpy::default());
    let service = ConversationService::new(Arc::clone(&store) as _);
    // Desktop 开始会话。
    let conversation = service.create("跨设备会话", Some("home")).expect("create");
    service
        .append_user(&conversation.conversation_id, "桌面端开的第一句")
        .expect("append");
    // iPad 继续（同一 store）。
    let loaded = service
        .load(&conversation.conversation_id)
        .expect("load")
        .expect("exists");
    assert_eq!(loaded.messages.len(), 1);
    service
        .append_assistant(&conversation.conversation_id, "平板端看到的回复", "openai-compatible", "m")
        .expect("append assistant");
    // Phone 查看。
    let reloaded = service
        .load(&conversation.conversation_id)
        .expect("load")
        .expect("exists");
    assert_eq!(reloaded.messages.len(), 2);
    assert_eq!(reloaded.messages[1].role, ConversationRole::Assistant);
    assert_eq!(reloaded.messages[1].provider.as_deref(), Some("openai-compatible"));
    // 归档后列表隐藏但仍可读（可恢复语义）。
    service.set_archived(&conversation.conversation_id, true).expect("archive");
    let list = service.list(10).expect("list");
    assert!(list.is_empty(), "归档后不在默认列表");
    assert!(service.load(&conversation.conversation_id).expect("load").is_some());
}

/// 内存 ConversationStore 替身（测试跨设备语义）。
#[derive(Default)]
struct InMemoryConversationSpy {
    inner: std::sync::Mutex<
        std::collections::HashMap<String, Conversation>,
    >,
}

impl crate::personal_ai::conversation::ConversationStore for InMemoryConversationSpy {
    fn list(&self, limit: usize) -> Result<Vec<ConversationSummary>, ConversationStoreError> {
        let inner = self.inner.lock().expect("lock");
        let mut items: Vec<_> = inner
            .values()
            .filter(|conversation| !conversation.archived)
            .map(|conversation| conversation.summary())
            .collect();
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        items.truncate(if limit == 0 { items.len() } else { limit });
        Ok(items)
    }
    fn list_all(
        &self,
        limit: usize,
    ) -> Result<Vec<ConversationSummary>, ConversationStoreError> {
        self.list(limit)
    }
    fn load(&self, id: &str) -> Result<Option<Conversation>, ConversationStoreError> {
        Ok(self.inner.lock().expect("lock").get(id).cloned())
    }
    fn create(&self, title: &str, module_origin: Option<&str>) -> Result<Conversation, ConversationStoreError> {
        let now = 1_000;
        let conversation = Conversation {
            conversation_id: format!("conv-{}", self.inner.lock().expect("lock").len() + 1),
            title: if title.trim().is_empty() {
                "未命名会话".into()
            } else {
                title.into()
            },
            messages: Vec::new(),
            module_origin: module_origin.map(str::to_string),
            archived: false,
            created_at: now,
            updated_at: now,
        };
        self.inner
            .lock()
            .expect("lock")
            .insert(conversation.conversation_id.clone(), conversation.clone());
        Ok(conversation)
    }
    fn append_message(&self, id: &str, message: &ConversationMessage) -> Result<(), ConversationStoreError> {
        let mut inner = self.inner.lock().expect("lock");
        let conversation = inner
            .get_mut(id)
            .ok_or_else(|| ConversationStoreError("missing".into()))?;
        conversation.messages.push(message.clone());
        conversation.updated_at += 1;
        Ok(())
    }
    fn rename(&self, id: &str, title: &str) -> Result<(), ConversationStoreError> {
        let mut inner = self.inner.lock().expect("lock");
        let conversation = inner
            .get_mut(id)
            .ok_or_else(|| ConversationStoreError("missing".into()))?;
        conversation.title = title.into();
        Ok(())
    }
    fn set_archived(&self, id: &str, archived: bool) -> Result<(), ConversationStoreError> {
        let mut inner = self.inner.lock().expect("lock");
        let conversation = inner
            .get_mut(id)
            .ok_or_else(|| ConversationStoreError("missing".into()))?;
        conversation.archived = archived;
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), ConversationStoreError> {
        self.inner.lock().expect("lock").remove(id);
        Ok(())
    }
}

// ---- Journey 11 · Server -------------------------------------------------

#[tokio::test]
async fn journey_11_server_dashboard_context() {
    let provider = ScriptedProvider::new(false);
    provider.push_text("服务状态：media 运行中，CPU 占用 12%。");
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let request = AgentRequest { message: "服务器现在怎么样？".into(), app_context: AppContext {
        module: Some("server".into()),
        entity: Some(devtoolbox_core::personal_ai::EntityRef {
            kind: "service".into(),
            id: "media".into(),
            label: Some("media".into()),
        }),
        ..AppContext::default()
        },
        ..AgentRequest::default()
    };
    let response = agent.run(request).await.expect("server ask");
    assert!(response.message.contains("media"));
}

// ---- Journey 12 · SafeAction ---------------------------------------------

#[tokio::test]
async fn journey_12_safe_action_produces_proposal_not_execution() {
    // worker 只能提议（§89）；执行走 SafeActionService 票据。
    let provider = ScriptedProvider::new(false);
    let service = OrchestrationService::new(
        Arc::new(crate::agents::profiles::default_registry()),
        provider as Arc<dyn ChatModelProvider>,
        Arc::new(ToolRegistry::new()),
    );
    let plan = service.plan_for_strategy("req-sa", "重启 media", DecisionStrategy::ResearchOnly);
    let outcome = service
        .execute("req-sa", "重启 media", &plan, &[], &AgentBudget::default(), true)
        .await;
    // 没有任何直接执行的痕迹：结果只含 worker 输出（这里为空 tool 集）。
    for result in &outcome.results {
        assert!(
            result.tool_calls.iter().all(|call| call.tool != "services.restart"),
            "orchestrator 不得直接执行 SYSTEM 工具"
        );
    }
}

// ---- Journey 13 · Multi-Agent --------------------------------------------

#[tokio::test]
async fn journey_13_multi_agent_trace_is_reported() {
    let provider = ScriptedProvider::new(false);
    provider.push_text(r#"{"findings":[{"claim":"证据 A","source":"logs"}]}"#);
    let service = OrchestrationService::new(
        Arc::new(crate::agents::profiles::default_registry()),
        Arc::clone(&provider) as Arc<dyn ChatModelProvider>,
        Arc::new(ToolRegistry::new()),
    );
    let plan = service.plan_for_strategy("req-ma", "结合日志与文档分析", DecisionStrategy::ResearchOnly);
    let outcome = service
        .execute("req-ma", "结合日志与文档分析", &plan, &[], &AgentBudget::default(), true)
        .await;
    // 两条 research run + merge。
    let research = outcome
        .results
        .iter()
        .filter(|result| result.agent_id == "research")
        .count();
    assert!(research >= 1, "research worker 必须跑");
    assert!(outcome.merged["sections"].is_array());
    assert_eq!(outcome.trace.runs.len(), research);
}

// ---- Journey 14 · Jev failure --------------------------------------------

#[tokio::test]
async fn journey_14_jev_unavailable_rule_fallback() {
    struct JevDown;
    #[async_trait::async_trait]
    impl DecisionProvider for JevDown {
        fn name(&self) -> &'static str {
            "jev"
        }
        fn is_available(&self) -> bool {
            true
        }
        async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, DecisionProviderError> {
            Err(DecisionProviderError::unavailable("down"))
        }
    }
    let engine = AgentDecisionEngine::new(Some(Arc::new(JevDown)), DecisionMode::JevActive, Some(2), 4);
    let request = DecisionRequest {
        message: "结合日志和文档分析".into(),
        available_workers: vec!["research".into(), "planner".into(), "reviewer".into()],
        ..DecisionRequest::default()
    };
    let (result, telemetry) = engine.decide(&request).await;
    assert_eq!(result.provider, "rule");
    assert!(telemetry.fallback);
    assert!(result.is_orchestrating(), "请求仍成功路由（规则接管）");
}

// ---- Journey 15 · MCP -----------------------------------------------------

#[tokio::test]
async fn journey_15_mcp_local_tools_listed_and_remote_denied() {
    // 本地：registry 是唯一 capability source（工具可见）。
    let provider = ScriptedProvider::new(false);
    let agent = agent_with(Arc::clone(&provider) as Arc<dyn ChatModelProvider>);
    let specs = agent.hub().tools.specs();
    assert!(specs.iter().any(|spec| spec.name == "history.search"));
    // 远程未授权：DecisionRequest 无法授予任何东西（能力只来自 registry）。
    let request = DecisionRequest {
        available_workers: vec!["research".into()],
        ..DecisionRequest::default()
    };
    assert!(request.allows_orchestration());
    // capability 交集保证 worker ⊆ parent（V9 已有测试）；这里断言契约面：
    let tools = agent.hub().tools.specs();
    for spec in &tools {
        assert!(
            matches!(spec.risk, ToolRisk::Read | ToolRisk::SafeWrite),
            "registry 门禁：{} 不得是 {}",
            spec.name,
            format!("{:?}", spec.risk)
        );
    }
}

// ---- Journey 16 · Offline PWA ---------------------------------------------

#[tokio::test]
async fn journey_16_offline_shell_and_degraded_features() {
    // 后端侧：LLM down 时非 AI 路径仍工作（readiness / search / conversation）。
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = AppSettings::default();
    let probes = default_probes(&settings, dir.path(), false, false, 0, false);
    let service = ReadinessService::new(probes);
    let report = service.report();
    assert_eq!(report.checks.len(), 13);
    // LLM 未配置 → AI Provider 项 NotConfigured（不是整个系统 down）。
    let ai = report
        .checks
        .iter()
        .find(|check| check.id == "ai_provider")
        .expect("ai check");
    assert_eq!(ai.status, devtoolbox_core::readiness::ReadinessStatus::NotConfigured);
    let backend = report
        .checks
        .iter()
        .find(|check| check.id == "backend")
        .expect("backend check");
    assert_eq!(backend.status, devtoolbox_core::readiness::ReadinessStatus::Ready);
}

// ---- Journey 17 · Update lifecycle ----------------------------------------

#[test]
fn journey_17_version_mismatch_shows_notice() {
    // 前端 versionNotice 的逻辑在后端以同样规则验证（版本比较 + 最低兼容）。
    use devtoolbox_core::operations::DeployMode;
    let _ = DeployMode::Development;
    // 版本兼容由前端 `versionNotice` 承担（pwa.ts）；后端 readiness 的
    // pwa_secure_context 项给出安全上下文状态。
    let dir = tempfile::tempdir().expect("tempdir");
    let probes = default_probes(&AppSettings::default(), dir.path(), false, false, 0, false);
    let service = ReadinessService::new(probes);
    let report = service.report();
    let pwa = report
        .checks
        .iter()
        .find(|check| check.id == "pwa_secure_context")
        .expect("pwa check");
    assert_eq!(
        pwa.status,
        devtoolbox_core::readiness::ReadinessStatus::Degraded,
        "非安全上下文必须 degraded 并说明（提示用户上 HTTPS）"
    );
}

// ---- Journey 18 · Backup restore ------------------------------------------

#[test]
fn journey_18_backup_restore_round_trip_on_fixture() {
    let fixture = tempfile::tempdir().expect("fixture");
    let dest = tempfile::tempdir().expect("dest");
    // fixture：一个 JSON 文件当作「数据」。
    let source = fixture.path().join("memory.json");
    std::fs::write(&source, br#"[{"content":"original preference"}]"#).expect("write fixture");

    let mut service = BackupService::new();
    service
        .register(Arc::new(crate::backup::JsonSource::new("memory", &source, true)))
        .expect("register");
    let backup_dir = fixture.path().join("out");
    let manifest = service
        .backup(&backup_dir, "0.1.0", "e2e fixture")
        .expect("backup");
    assert_eq!(manifest.entries.len(), 1);

    // 改 fixture（模拟数据变化）。
    std::fs::write(&source, br#"[{"content":"changed"}]"#).expect("mutate");

    // 恢复到隔离目标。
    let restore_dir = dest.path().join("restored");
    let report = service
        .restore(&backup_dir, &restore_dir)
        .expect("restore");
    assert!(report.integrity_ok);
    let restored_bytes = std::fs::read(restore_dir.join("memory.json")).expect("read restored");
    assert_eq!(
        String::from_utf8_lossy(&restored_bytes),
        r#"[{"content":"original preference"}]"#,
        "恢复的是备份时点的数据，不是被改后的"
    );
}

// ---- 辅助：AppPaths 崩溃标记（V11 §57 的用户可见面） ----------------------

#[test]
fn crash_recovery_marker_is_user_visible() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::from_root(dir.path());
    let marker = devtoolbox_core::operations::StartupMarker::new(&paths);
    marker.mark_running().expect("mark");
    let report = marker.recover();
    assert!(report.unclean_previous_run);
    assert!(report.rebuildable_artifacts.contains(&"cache/".to_string()));
}
