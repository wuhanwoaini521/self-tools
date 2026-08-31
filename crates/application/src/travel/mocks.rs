//! 测试用 Mock Provider（仅在 `cfg(test)` 下编译）。
//!
//! 需求 #二十五：测试不得依赖真实百度 / 携程 / 第三方网站。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use devtoolbox_core::travel::{ContentState, SearchResult, TravelDocument, TravelFact};
use devtoolbox_infrastructure::{
    InfrastructureError, LlmProvider, SearchOptions, SearchProvider, TravelDataProvider,
    TravelDataRequest, WebFetcher,
};

/// 可编程搜索 Provider：
/// - `queries` 精确命中查询；`default` 兜底；`errors` 指定某查询直接失败。
pub struct MockSearchProvider {
    pub name: &'static str,
    pub queries: HashMap<String, Vec<SearchResult>>,
    pub default: Vec<SearchResult>,
    pub errors: HashMap<String, String>,
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl MockSearchProvider {
    #[must_use]
    pub fn new(name: &'static str, default: Vec<SearchResult>) -> Self {
        Self {
            name,
            queries: HashMap::new(),
            default,
            errors: HashMap::new(),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl SearchProvider for MockSearchProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn search(
        &self,
        query: &str,
        _options: SearchOptions,
    ) -> Result<Vec<SearchResult>, InfrastructureError> {
        self.calls
            .lock()
            .expect("calls poisoned")
            .push(query.to_string());
        // 精确查询失败优先；"*" 为全量失败（模拟整个 Provider 不可用）
        if let Some(message) = self.errors.get(query).or_else(|| self.errors.get("*")) {
            return Err(InfrastructureError::TravelSearch(message.clone()));
        }
        if let Some(results) = self.queries.get(query) {
            return Ok(results.clone());
        }
        Ok(self.default.clone())
    }
}

/// 可编程网页抓取器：`pages[url] = Ok(content)` 或 Err。
pub struct MockWebFetcher {
    pub pages: HashMap<String, Result<MockPage, String>>,
    pub calls: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
pub struct MockPage {
    pub title: String,
    pub content: String,
}

impl MockWebFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_page(mut self, url: &str, title: &str, content: &str) -> Self {
        self.pages.insert(
            url.to_string(),
            Ok(MockPage {
                title: title.to_string(),
                content: content.to_string(),
            }),
        );
        self
    }

    pub fn with_error(mut self, url: &str, message: &str) -> Self {
        self.pages.insert(url.to_string(), Err(message.to_string()));
        self
    }
}

impl Default for MockWebFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebFetcher for MockWebFetcher {
    async fn fetch(&self, url: &str) -> Result<TravelDocument, InfrastructureError> {
        self.calls
            .lock()
            .expect("calls poisoned")
            .push(url.to_string());
        match self.pages.get(url) {
            Some(Ok(page)) => Ok(TravelDocument {
                url: url.to_string(),
                title: page.title.clone(),
                state: ContentState::Full,
                content: Some(page.content.clone()),
                snippet: None,
                published_at: None,
                fetched_at: 1_700_000_000,
                provider: Some("mock".to_string()),
            }),
            Some(Err(message)) => Err(InfrastructureError::TravelFetch(message.clone())),
            None => Err(InfrastructureError::TravelFetch(
                "page not configured".to_string(),
            )),
        }
    }
}

/// 可编程 LLM：FIFO 响应队列。`ERR:...` 前缀代表调用失败；队列耗尽返回错误。
pub struct MockLlmProvider {
    pub responses: Arc<Mutex<std::collections::VecDeque<String>>>,
    pub calls: Arc<Mutex<usize>>,
}

impl MockLlmProvider {
    #[must_use]
    pub fn new(responses: impl IntoIterator<Item = String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            calls: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn complete(&self, _system: &str, _user: &str) -> Result<String, InfrastructureError> {
        let mut calls = self.calls.lock().expect("calls poisoned");
        *calls += 1;
        let next = self
            .responses
            .lock()
            .expect("responses poisoned")
            .pop_front();
        match next {
            Some(raw) if raw.starts_with("ERR:") => Err(InfrastructureError::TravelLlm(
                raw.trim_start_matches("ERR:").to_string(),
            )),
            Some(raw) => Ok(raw),
            None => Err(InfrastructureError::TravelLlm(
                "mock llm queue exhausted".to_string(),
            )),
        }
    }
}

/// 可编程结构化数据 Provider（模拟高德 / 和风）。
pub struct MockDataProvider {
    pub name: &'static str,
    /// kind → 结果（"poi" / "weather"）。
    pub responses: HashMap<String, Result<Vec<TravelFact>, String>>,
}

impl MockDataProvider {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            responses: HashMap::new(),
        }
    }

    pub fn with_facts(mut self, kind: &str, facts: Vec<TravelFact>) -> Self {
        self.responses.insert(kind.to_string(), Ok(facts));
        self
    }

    pub fn with_error(mut self, kind: &str, message: &str) -> Self {
        self.responses
            .insert(kind.to_string(), Err(message.to_string()));
        self
    }
}

#[async_trait]
impl TravelDataProvider for MockDataProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn fetch(
        &self,
        request: TravelDataRequest,
    ) -> Result<Vec<TravelFact>, InfrastructureError> {
        match self.responses.get(request.kind) {
            Some(Ok(facts)) => Ok(facts.clone()),
            Some(Err(message)) => Err(InfrastructureError::TravelData(message.clone())),
            None => Ok(Vec::new()),
        }
    }
}

/// 便捷构造搜索结果。
#[must_use]
pub fn search_result(url: &str, title: &str, snippet: &str) -> SearchResult {
    SearchResult {
        title: title.to_string(),
        url: url.to_string(),
        snippet: snippet.to_string(),
        provider: "mock".to_string(),
        published_at: None,
        fetched_at: 1_700_000_000,
    }
}
