//! Travel provider 组合根（Gate 7）。
//!
//! 具体的搜索 / 抓取 / LLM / 数据 Provider 全部在组合根装配；Tauri 命令不承担
//! provider 装配，只负责参数校验、会话登记与后台任务调度。Application 层只
//! 看到 capability 接口（`SearchProvider` / `WebFetcher` / `LlmProvider` /
//! `TravelDataProvider`），不接触具体实现。

use std::sync::{Arc, Mutex};

use crate::composition::TravelStoreAdapter;
use devtoolbox_application::travel::TravelResearchService;
use devtoolbox_core::settings::TravelSettings;
use devtoolbox_infrastructure::{
    AmapPoiProvider, HttpWebFetcher, LlmConfig, LlmProvider, OpenAiCompatibleLlmProvider,
    QWeatherProvider, TravelStore, build_providers, providers_for,
};

/// 按设置装配一个完整研究服务（未配置项自动降级为 None / 空列表）。
pub fn travel_research_service(
    client: &reqwest::Client,
    travel: &TravelSettings,
    store: Arc<Mutex<TravelStore>>,
) -> TravelResearchService {
    TravelResearchService::new(
        build_providers(
            travel.search_backend.clone(),
            travel.searxng_url.clone(),
            client,
        ),
        http_fetcher(client),
        llm_provider(client, travel),
        providers_for(
            travel.amap_api_key.clone(),
            travel.qweather_api_key.clone(),
            travel.qweather_api_host.clone(),
            travel.baidu_map_api_key.clone(),
            client,
        ),
        Arc::new(TravelStoreAdapter::new(store)),
    )
}

fn http_fetcher(client: &reqwest::Client) -> Box<dyn devtoolbox_infrastructure::WebFetcher> {
    Box::new(HttpWebFetcher::new((*client).clone()))
}

fn llm_provider(client: &reqwest::Client, travel: &TravelSettings) -> Option<Box<dyn LlmProvider>> {
    if travel.llm_base_url.is_none() && travel.llm_model.is_none() {
        return None;
    }
    Some(Box::new(OpenAiCompatibleLlmProvider::new(
        (*client).clone(),
        LlmConfig {
            base_url: travel.llm_base_url.clone(),
            api_key: travel.llm_api_key.clone(),
            model: travel.llm_model.clone(),
        },
    )))
}

/// 连通性测试 Provider 的构造入口（测试命令内不内联构造 Provider）。
pub fn llm_test_provider(
    client: &reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
) -> OpenAiCompatibleLlmProvider {
    OpenAiCompatibleLlmProvider::new(
        (*client).clone(),
        LlmConfig {
            base_url: Some(base_url),
            api_key,
            model: Some(model),
        },
    )
}

pub fn amap_test_provider(client: &reqwest::Client, api_key: String) -> AmapPoiProvider {
    AmapPoiProvider::new((*client).clone(), api_key)
}

pub fn qweather_test_provider(
    client: &reqwest::Client,
    api_key: String,
    host: String,
) -> QWeatherProvider {
    QWeatherProvider::new((*client).clone(), api_key, host)
}
