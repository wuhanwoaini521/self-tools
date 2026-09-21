//! History Enrichment 桌面适配器 + 组合根（V5 Gate 4）。
//!
//! - SearchAdapter：复用 travel 搜索生态（`build_providers`，按 TravelSettings 装配）；
//! - LlmAdapter：统一 `ChatModelProvider`（按 AiSettings 装配）；
//! - StoreAdapter：`Arc<Mutex<EnrichmentSqliteStore>>` → application `EnrichmentStore`；
//! - Runner：跨调用单飞 + 按调用读取设置，实现 `EnrichmentRunnerPort`（命令与
//!   `history.ensure_enrichment` 工具共用）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use devtoolbox_application::history::enrichment::{
    EnrichmentConfig, EnrichmentEntityPort, EnrichmentLlmPort, EnrichmentRunnerPort,
    EnrichmentSearchPort, EnrichmentStore, HistoryEnrichmentService, SourceEvidence, SourceType,
    entity_port_from_history, normalize_domain,
};
use devtoolbox_application::history::{
    EnrichmentKey, EnrichmentSection, EnrichmentState, EnrichmentView,
};
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::travel::SearchOptions;
use devtoolbox_core::{ChatMessage, ChatModelProvider, ChatRequest};
use devtoolbox_infrastructure::{
    AiModelConfig, EnrichmentSqliteStore, OpenAiCompatibleChatModelProvider, build_providers,
};

/// 设置读取器（desktop 命令层注入；闭包捕获 AppHandle）。
pub type SettingsLoader = Arc<dyn Fn() -> Result<AppSettings, String> + Send + Sync>;

/// 搜索端口适配器：复用旅行搜索 Provider（Bing/Baidu/SearXNG）。
pub struct EnrichmentSearchAdapter {
    client: reqwest::Client,
    settings: SettingsLoader,
}

#[async_trait]
impl EnrichmentSearchPort for EnrichmentSearchAdapter {
    fn configured(&self) -> bool {
        match self.settings() {
            Ok(settings) => !build_providers(
                settings.travel.search_backend.clone(),
                settings.travel.searxng_url.clone(),
                &self.client,
            )
            .is_empty(),
            Err(_) => false,
        }
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SourceEvidence>, String> {
        let settings = self.settings()?;
        let providers = build_providers(
            settings.travel.search_backend.clone(),
            settings.travel.searxng_url.clone(),
            &self.client,
        );
        let mut last_error = String::new();
        for provider in &providers {
            match provider
                .search(query, SearchOptions { count: limit })
                .await
            {
                Ok(results) if !results.is_empty() => {
                    let mut evidence: Vec<SourceEvidence> = results
                        .into_iter()
                        .map(|result| SourceEvidence {
                            title: result.title,
                            url: result.url.clone(),
                            domain: normalize_domain(&result.url),
                            snippet: result.snippet,
                            published_at: result.published_at,
                            source_type: classify_domain(&result.url),
                        })
                        .collect();
                    evidence.truncate(limit);
                    return Ok(evidence);
                }
                Ok(_) => continue,
                Err(error) => last_error = error.to_string(),
            }
        }
        if last_error.is_empty() {
            Err("search returned no results".to_string())
        } else {
            Err(last_error)
        }
    }
}

impl EnrichmentSearchAdapter {
    #[must_use]
    pub fn new(client: reqwest::Client, settings: SettingsLoader) -> Self {
        Self { client, settings }
    }

    fn settings(&self) -> Result<AppSettings, String> {
        (self.settings)()
    }
}

/// 简单来源类型分类（复用 enrichment::classify_source 规则；这里是域名级初步分类）。
fn classify_domain(url: &str) -> SourceType {
    let domain = normalize_domain(url);
    if domain.ends_with("gov.cn") || domain.ends_with(".gov") {
        return SourceType::Official;
    }
    if domain.contains("museum") || domain.contains("baike") || domain.contains("archive") {
        return SourceType::Reference;
    }
    if domain.ends_with(".edu.cn") || domain.ends_with(".edu") || domain.ends_with("ac.cn") {
        return SourceType::University;
    }
    SourceType::General
}

/// LLM 端口适配器：统一 ChatModelProvider（AiSettings）。
pub struct EnrichmentLlmAdapter {
    client: reqwest::Client,
    ai: devtoolbox_core::settings::AiSettings,
}

impl EnrichmentLlmAdapter {
    #[must_use]
    pub fn new(client: reqwest::Client, ai: devtoolbox_core::settings::AiSettings) -> Self {
        Self { client, ai }
    }
}

#[async_trait]
impl EnrichmentLlmPort for EnrichmentLlmAdapter {
    fn configured(&self) -> bool {
        self.ai.is_configured()
    }

    fn describe(&self) -> (Option<String>, Option<String>) {
        (Some("openai-compatible".to_string()), self.ai.model.clone())
    }

    async fn generate(&self, system: &str, user: &str) -> Result<String, String> {
        if !self.ai.is_configured() {
            return Err("enrichment llm is not configured".to_string());
        }
        let provider = OpenAiCompatibleChatModelProvider::new(
            self.client.clone(),
            AiModelConfig {
                base_url: self.ai.base_url.clone(),
                api_key: self.ai.api_key.clone(),
                model: self.ai.model.clone(),
                timeout_secs: self.ai.timeout_secs,
            },
        );
        let response = provider
            .chat(ChatRequest {
                messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
                tools: Vec::new(),
                temperature: Some(0.2),
                max_tokens: None,
            })
            .await
            .map_err(|error| error.message)?;
        response
            .content
            .ok_or_else(|| "enrichment llm returned empty response".to_string())
    }
}

/// 存储端口适配器（`Arc<Mutex<EnrichmentSqliteStore>>` → application `EnrichmentStore`）。
pub struct EnrichmentStoreAdapter {
    store: Arc<Mutex<EnrichmentSqliteStore>>,
}

impl EnrichmentStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<EnrichmentSqliteStore>>) -> Self {
        Self { store }
    }
}

impl EnrichmentStore for EnrichmentStoreAdapter {
    fn load_best(&self, key: &EnrichmentKey) -> Result<Option<devtoolbox_core::history_enrichment::EnrichmentRecord>, String> {
        self.store.lock().expect("enrichment store poisoned").load_best(key)
    }
    fn load_revision(
        &self,
        key: &EnrichmentKey,
        revision: u32,
    ) -> Result<Option<devtoolbox_core::history_enrichment::EnrichmentRecord>, String> {
        self.store.lock().expect("enrichment store poisoned").load_revision(key, revision)
    }
    fn list_revisions(&self, key: &EnrichmentKey) -> Result<Vec<devtoolbox_core::history_enrichment::EnrichmentRecord>, String> {
        self.store.lock().expect("enrichment store poisoned").list_revisions(key)
    }
    fn next_revision(&self, key: &EnrichmentKey) -> Result<u32, String> {
        self.store.lock().expect("enrichment store poisoned").next_revision(key)
    }
    fn put(&self, record: &devtoolbox_core::history_enrichment::EnrichmentRecord) -> Result<(), String> {
        self.store.lock().expect("enrichment store poisoned").put(record)
    }
    fn mark_reviewed(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        self.store.lock().expect("enrichment store poisoned").mark_reviewed(key, revision)
    }
    fn delete_revision(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        self.store.lock().expect("enrichment store poisoned").delete_revision(key, revision)
    }
}

/// 富化运行器：跨调用单飞 + 每次调用按最新设置装配 search/llm。
pub struct EnrichmentRunner {
    client: reqwest::Client,
    settings: SettingsLoader,
    store: Arc<Mutex<EnrichmentSqliteStore>>,
    entity: Arc<dyn EnrichmentEntityPort>,
    in_flight: Mutex<std::collections::HashSet<EnrichmentKey>>,
}

impl EnrichmentRunner {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: reqwest::Client,
        settings: SettingsLoader,
        store: Arc<Mutex<EnrichmentSqliteStore>>,
        entity: Arc<dyn EnrichmentEntityPort>,
    ) -> Self {
        Self {
            client,
            settings,
            store,
            entity,
            in_flight: Mutex::new(std::collections::HashSet::new()),
        }
    }

    fn service(&self, ai: devtoolbox_core::settings::AiSettings) -> HistoryEnrichmentService {
        HistoryEnrichmentService::new(
            Arc::new(EnrichmentStoreAdapter::new(Arc::clone(&self.store))),
            Arc::new(EnrichmentSearchAdapter::new(self.client.clone(), Arc::clone(&self.settings))),
            Arc::new(EnrichmentLlmAdapter::new(self.client.clone(), ai)),
            Arc::clone(&self.entity),
            EnrichmentConfig::default(),
        )
    }
}

#[async_trait]
impl EnrichmentRunnerPort for EnrichmentRunner {
    fn get(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        let settings = (self.settings)().unwrap_or_default();
        let ai = settings.ai;
        self.service(ai).get(key)
    }

    async fn ensure(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        {
            let mut flight = self.in_flight.lock().expect("enrichment flight poisoned");
            if !flight.insert(key.clone()) {
                return Ok(EnrichmentView {
                    key: key.clone(),
                    state: EnrichmentState::Generating,
                    payload: None,
                    metadata: None,
                    error: None,
                });
            }
        }
        let result = {
            let settings = (self.settings)().unwrap_or_default();
            let ai = settings.ai;
            self.service(ai).ensure(key).await
        };
        self.in_flight.lock().expect("enrichment flight poisoned").remove(key);
        result
    }

    async fn refresh(&self, key: &EnrichmentKey) -> Result<EnrichmentView, String> {
        {
            let mut flight = self.in_flight.lock().expect("enrichment flight poisoned");
            if !flight.insert(key.clone()) {
                return Ok(EnrichmentView {
                    key: key.clone(),
                    state: EnrichmentState::Generating,
                    payload: None,
                    metadata: None,
                    error: None,
                });
            }
        }
        let result = {
            let settings = (self.settings)().unwrap_or_default();
            let ai = settings.ai;
            self.service(ai).refresh(key).await
        };
        self.in_flight.lock().expect("enrichment flight poisoned").remove(key);
        result
    }

    fn sections(
        &self,
        entity_type: &str,
        entity_id: &str,
        locale: &str,
    ) -> Result<Vec<(EnrichmentSection, EnrichmentState)>, String> {
        self.service((self.settings)().unwrap_or_default().ai)
            .sections(entity_type, entity_id, locale)
    }

    fn mark_reviewed(&self, key: &EnrichmentKey) -> Result<(), String> {
        self.service((self.settings)().unwrap_or_default().ai).mark_reviewed(key)
    }
}

/// 组合根装配入口（setup 调用）。
pub fn build_runner(
    client: reqwest::Client,
    settings: SettingsLoader,
    config_dir: &std::path::Path,
    history_port: Arc<dyn devtoolbox_application::history::HistoryQueryPort>,
) -> Result<Arc<dyn EnrichmentRunnerPort>, devtoolbox_infrastructure::InfrastructureError> {
    let store = EnrichmentSqliteStore::open(config_dir.join("history_enrichment.db"))?;
    let entity = entity_port_from_history(history_port);
    Ok(Arc::new(EnrichmentRunner::new(
        client,
        settings,
        Arc::new(Mutex::new(store)),
        entity,
    )))
}