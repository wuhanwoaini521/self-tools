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
#[allow(clippy::too_many_arguments)]
pub fn build_hub(
    history: Arc<HistoryDuckDbRepository>,
    runner: Option<Arc<dyn devtoolbox_application::history::enrichment::EnrichmentRunnerPort>>,
    travel: Arc<dyn devtoolbox_application::travel::TravelAiPort>,
    geography: Arc<dyn devtoolbox_application::geography::GeographyQueryPort + Send + Sync>,
    language_store: Arc<dyn devtoolbox_application::language::LanguageStorePort>,
    language_llm: Option<Arc<dyn ChatModelProvider>>,
    knowledge: &crate::knowledge::KnowledgeRuntime,
    server: &crate::server::ServerRuntime,
    study_board_store: Arc<dyn devtoolbox_application::StudyBoardStorePort>,
    settings: &devtoolbox_core::settings::AppSettings,
    client: reqwest::Client,
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
    // V7 Home Server 模块（标准接入；零 PersonalAgent 改动，§106）。
    // 14 个工具全部注册为 Read：SYSTEM 语义在 SafeActionService 的票据里，
    // 由桌面命令 `confirm_action` 强制确认（§53/§108）。
    register_server(
        &mut hub.modules,
        &mut hub.tools,
        Arc::clone(&server.server),
        Arc::clone(&server.services),
        Arc::clone(&server.apps),
        Arc::clone(&server.actions),
        Arc::clone(&server.logs),
        server.trust,
    )
    .expect("register server module");
    // 通用检索增强 stage（V6 §22/§55）：平台可选能力，无业务分支。
    hub.retrieval = Some(Arc::clone(&knowledge.retrieval)
        as Arc<dyn devtoolbox_application::personal_ai::RetrievalAugmenter>);
    // V11-M Study Board：标准模块接入（descriptor + 4 工具 + ContextProvider）。
    let study_board_store = study_board_store.clone();
    devtoolbox_application::personal_ai::register_study_board(
        &mut hub.modules,
        &mut hub.tools,
        study_board_store,
    )
    .expect("register study-board module");
    // V10：决策引擎 + 有界多 Agent 编排（rule / jev-shadow / jev-active）。
    // 决策层只选策略；执行/授权/预算仍由 OrchestrationService + ToolRegistry 强制。
    // `settings` 来自调用方（同 enrichment 模式：每次装配读取一次最新 settings.json）。
    // ToolRegistry::clone 是浅拷贝（内部 Arc<dyn ToolExecutor>）：hub 与编排器
    // 共享同一份工具执行体，无双注册、无第二 capability source。
    let tools = Arc::new(hub.tools.clone());
    let hub_ai = settings.ai.clone();
    let provider = build_provider(client.clone(), &hub_ai);
    let decision = build_decision_engine(client, &settings.decision);
    hub.orchestration = Some(Arc::new(
        devtoolbox_application::agents::OrchestrationService::new(
            Arc::new(devtoolbox_application::agents::default_registry()),
            provider,
            tools,
        )
        .with_decision_engine(decision),
    ));
    Arc::new(hub)
}

/// 装配决策引擎（V10 §13）：rule 基线 + 可选 Jev provider + 模式。
fn build_decision_engine(
    client: reqwest::Client,
    decision: &devtoolbox_core::settings::DecisionSettings,
) -> devtoolbox_application::agents::AgentDecisionEngine {
    let jev: Option<Arc<dyn devtoolbox_core::agents::DecisionProvider>> =
        if decision.jev_configured() {
            Some(Arc::new(
                devtoolbox_infrastructure::agents::JevDecisionProvider::http(
                    client,
                    devtoolbox_infrastructure::agents::JevConfig {
                        base_url: decision.jev_base_url.clone(),
                        api_key: decision.jev_api_key.clone(),
                        model: decision.jev_model.clone(),
                        timeout_secs: Some(decision.jev_timeout_secs),
                    },
                ),
            ))
        } else {
            None
        };
    devtoolbox_application::agents::AgentDecisionEngine::new(
        jev,
        decision.effective_mode(),
        Some(decision.jev_timeout_secs),
        decision.max_workers,
    )
}

/// 装配一次调用的 PersonalAgent（Provider 每次读取最新设置；Hub/Session 恒定）。
pub fn build_agent(
    provider: Arc<dyn ChatModelProvider>,
    hub: Arc<PersonalHub>,
    session: Arc<InMemorySessionStore>,
) -> PersonalAgent {
    PersonalAgent::new(provider, hub, session, AgentConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 装配铁律（V7 Gate 7 / 审查 V7-SEC-001）：每个域模块都必须真正注册进 hub。
    ///
    /// 这条测试存在的原因：`build_hub` 曾经接收 `server` 参数却忘了调用
    /// `register_server`，14 个工具在真实桌面运行里不存在——编译期与其它测试
    /// 都发现不了。模块注册是「装配契约」，必须被断言。
    ///
    /// 用真实 store（tempdir）+ 真实适配器构建，与 `setup` 平行到 hub 为止。
    fn build_test_hub() -> Option<Arc<PersonalHub>> {
        let directory = tempfile::tempdir().unwrap();
        let settings: crate::knowledge::SettingsLoader =
            Arc::new(|| Ok(devtoolbox_core::settings::AppSettings::default()));
        let knowledge = crate::knowledge::KnowledgeRuntime::build(
            directory.path(),
            Arc::clone(&settings),
            devtoolbox_core::knowledge::KnowledgeBudget::default(),
        )
        .expect("knowledge runtime");
        let server = crate::server::ServerRuntime::build(
            directory.path(),
            Arc::clone(&settings),
            crate::server::desktop_trust(),
        )
        .expect("server runtime");

        // DuckDB 需要已存在的 schema 文件（不像 SQLite 会创建）：
        // 复制 dist 库到 tempdir。该产物由 submodule 的
        // `python -m src.history_data_pipeline backbone build` 生成，且在其 .gitignore
        // 里 —— CI checkout（含 submodule）不保证它存在。缺失 = 跳过装配测试
        // （真实运行路径由本地/发布构建覆盖）。
        let history_target = directory.path().join("history.duckdb");
        // cwd 随调用方变化；用 manifest 目录定位仓库内 fixture。
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../history-data-pipeline/dist/history.duckdb");
        if !fixture.is_file() {
            eprintln!(
                "skip: history fixture missing ({}); run `backbone build` in history-data-pipeline",
                fixture.display()
            );
            return None;
        }
        std::fs::copy(&fixture, &history_target).expect("copy history fixture");
        let history_repo = Arc::new(
            devtoolbox_infrastructure::HistoryDuckDbRepository::open(&history_target)
                .expect("history repo"),
        );
        let travel_store = Arc::new(Mutex::new(
            devtoolbox_infrastructure::TravelStore::open(directory.path().join("travel.db"))
                .expect("travel store"),
        ));
        let geography_store = Arc::new(Mutex::new(
            devtoolbox_infrastructure::GeographyStore::open(directory.path().join("geography.db"))
                .expect("geography store"),
        ));
        let language_store = Arc::new(Mutex::new(
            devtoolbox_infrastructure::LanguageStore::open(directory.path().join("language.db"))
                .expect("language store"),
        ));
        let client = devtoolbox_infrastructure::feed_client().expect("http client");

        let travel_ai: Arc<dyn devtoolbox_application::travel::TravelAiPort> = Arc::new(
            crate::travel_ai::TravelAiAdapter::new(client, Arc::clone(&settings), travel_store),
        );
        let geography_port: Arc<
            dyn devtoolbox_application::geography::GeographyQueryPort + Send + Sync,
        > = Arc::new(crate::geography_query::GeographyQueryAdapter::new(
            geography_store,
        ));
        let language_port: Arc<dyn devtoolbox_application::language::LanguageStorePort> = Arc::new(
            crate::composition::LanguageStoreAdapter::new(language_store),
        );

        let hub_settings = devtoolbox_core::settings::AppSettings::default();
        let hub_client = devtoolbox_infrastructure::feed_client().expect("http client");
        let study_board_store = devtoolbox_infrastructure::StudyBoardSqliteStore::open_in_memory()
            .expect("study board store");
        Some(build_hub(
            history_repo,
            None,
            travel_ai,
            geography_port,
            language_port,
            None,
            &knowledge,
            &server,
            Arc::new(crate::composition::StudyBoardStoreAdapter::new(Arc::new(
                study_board_store,
            ))),
            &hub_settings,
            hub_client,
        ))
    }

    #[test]
    fn build_hub_registers_every_domain_module() {
        let Some(hub) = build_test_hub() else { return };
        let modules: Vec<String> = hub
            .modules
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect();
        for expected in [
            "history",
            "travel",
            "geography",
            "language",
            "memory",
            "documents",
            "files",
            "knowledge",
            "server",
            "study-board",
        ] {
            assert!(
                modules.iter().any(|id| id == expected),
                "模块 {expected} 必须注册：{modules:?}"
            );
        }
    }

    #[test]
    fn study_board_module_tools_are_reachable() {
        let Some(hub) = build_test_hub() else { return };
        let names: Vec<String> = hub
            .tools
            .specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect();
        for tool in [
            "study-board.list",
            "study-board.get",
            "study-board.save",
            "study-board.snapshot",
        ] {
            assert!(
                names.iter().any(|name| name == tool),
                "工具 {tool} 必须可触达"
            );
        }
    }

    #[test]
    fn server_module_tools_are_reachable_from_the_agent() {
        let Some(hub) = build_test_hub() else { return };
        let names: Vec<String> = hub
            .tools
            .specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect();
        for tool in [
            "server.get_status",
            "services.list",
            "services.get_logs",
            "services.restart",
            "apps.open",
        ] {
            assert!(
                names.iter().any(|name| name == tool),
                "工具 {tool} 必须可从 PersonalAgent 触达"
            );
        }
        // 风险面：SYSTEM 语义不在工具上（工具全 Read）；写入口只有 confirm_action。
        for spec in hub.tools.specs() {
            if spec.module == "server" {
                assert_eq!(spec.risk, devtoolbox_core::ToolRisk::Read, "{}", spec.name);
            }
        }
    }
}
