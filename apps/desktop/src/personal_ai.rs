//! Personal AI 组合根（V4 Gate 4/6）。
//!
//! 具体实现（Provider / Hub / Session / History 端口）在这里装配：
//! - Provider：按 `AiSettings` 决定真实 OpenAI-Compatible 实现或未配置降级桩；
//! - Hub：注册 History 标准模块（descriptor + 4 个 Read 工具 + ContextProvider）；
//! - Session：内存会话存储（持久化 = P1）。
//!
//! 命令层只做参数透传与配置读取，不承担 agent 装配（PATTERN：travel_providers.rs）。

use std::sync::Arc;

use async_trait::async_trait;
use devtoolbox_application::personal_ai::{
    AgentConfig, InMemorySessionStore, PersonalAgent, PersonalHub, register_documents,
    register_files, register_geography, register_history, register_knowledge, register_language,
    register_memory, register_server, register_travel,
};
use devtoolbox_core::personal_ai::{ChatModelProvider, ChatRequest, ChatResponse, ProviderError};
use devtoolbox_core::settings::AiSettings;
use devtoolbox_infrastructure::{
    AiModelConfig, HistoryDuckDbRepository, OpenAiCompatibleChatModelProvider,
};

use crate::history_query::HistoryQueryAdapter;

/// 未配置时的降级 Provider：Agent 收到 `ModelUnavailable` 受控错误，
/// App 其他功能完全正常（V4 §63/§85）。
pub struct UnconfiguredModelProvider;

#[async_trait]
impl ChatModelProvider for UnconfiguredModelProvider {
    fn name(&self) -> &'static str {
        "unconfigured"
    }

    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::unavailable(
            "AI provider 未配置：请在 设置 → AI 中填写 base_url 与 model（本地 Ollama 可留空 key）",
        ))
    }
}

/// 按设置构造 Provider（未配置 → 降级桩）。
pub fn build_provider(client: reqwest::Client, ai: &AiSettings) -> Arc<dyn ChatModelProvider> {
    if ai.is_configured() {
        Arc::new(OpenAiCompatibleChatModelProvider::new(
            client,
            AiModelConfig {
                base_url: ai.base_url.clone(),
                api_key: ai.api_key.clone(),
                model: ai.model.clone(),
                timeout_secs: ai.timeout_secs,
            },
        ))
    } else {
        Arc::new(UnconfiguredModelProvider)
    }
}

/// 装配注册中心：注册全部标准模块（V4 §41 + V5 模块 + V6 知识层）。
///
/// 模块接入一律走 `ModuleDescriptor + tools + ContextProvider + register_*`
/// （V6 §99/§100）：`PersonalAgent` 核心不含任何模块业务分支。
pub fn build_hub(
    history: Arc<HistoryDuckDbRepository>,
    runner: Option<Arc<dyn devtoolbox_application::history::enrichment::EnrichmentRunnerPort>>,
    travel: Arc<dyn devtoolbox_application::travel::TravelAiPort>,
    geography: Arc<dyn devtoolbox_application::geography::GeographyQueryPort + Send + Sync>,
    language_store: Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    language_llm: Option<Arc<dyn ChatModelProvider>>,
    knowledge: &crate::knowledge::KnowledgeRuntime,
    server: &crate::server::ServerRuntime,
) -> Arc<PersonalHub> {
    let port: Arc<dyn devtoolbox_application::history::HistoryQueryPort> =
        Arc::new(HistoryQueryAdapter::new(history));
    let mut hub = PersonalHub::default();
    register_history(&mut hub.modules, &mut hub.tools, port, runner)
        .expect("register history module");
    register_travel(&mut hub.modules, &mut hub.tools, travel).expect("register travel module");
    register_geography(&mut hub.modules, &mut hub.tools, geography)
        .expect("register geography module");
    register_language(
        &mut hub.modules,
        &mut hub.tools,
        language_store,
        language_llm,
    )
    .expect("register language module");
    // V6 知识层：三个独立模块 + 一个 facade 模块。
    register_memory(
        &mut hub.modules,
        &mut hub.tools,
        Arc::clone(&knowledge.memory),
    )
    .expect("register memory module");
    register_documents(
        &mut hub.modules,
        &mut hub.tools,
        Arc::clone(&knowledge.documents),
        Arc::clone(&knowledge.settings),
    )
    .expect("register documents module");
    register_files(
        &mut hub.modules,
        &mut hub.tools,
        Arc::clone(&knowledge.files),
        Arc::clone(&knowledge.settings),
    )
    .expect("register files module");
    register_knowledge(
        &mut hub.modules,
        &mut hub.tools,
        Arc::clone(&knowledge.retrieval),
    )
    .expect("register knowledge module");
    // 通用检索增强 stage（V6 §22/§55）：平台可选能力，无业务分支。
    hub.retrieval = Some(Arc::clone(&knowledge.retrieval)
        as Arc<dyn devtoolbox_application::personal_ai::RetrievalAugmenter>);
    Arc::new(hub)
}

/// 装配一次调用的 PersonalAgent（Provider 每次读取最新设置；Hub/Session 恒定）。
pub fn build_agent(
    provider: Arc<dyn ChatModelProvider>,
    hub: Arc<PersonalHub>,
    session: Arc<InMemorySessionStore>,
) -> PersonalAgent {
    PersonalAgent::new(provider, hub, session, AgentConfig::default())
}
