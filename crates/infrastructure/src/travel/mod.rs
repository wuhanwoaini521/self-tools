//! Travel 基础设施适配器：搜索 / 抓取 / LLM / 数据 Provider / SQLite 缓存。

pub mod data_provider;
pub mod fetcher;
pub mod llm;
pub mod search;
pub mod store;

pub use data_provider::{
    AmapPoiProvider, QWeatherProvider, TravelDataProvider, TravelDataRequest, providers_for,
};
pub use fetcher::{HttpWebFetcher, WebFetcher, detect_encoding, extract_text};
pub use llm::{LlmConfig, LlmProvider, OpenAiCompatibleLlmProvider, extract_chat_content};
pub use search::{
    BaiduSearchProvider, BingChinaSearchProvider, SearXngSearchProvider, SearchOptions,
    SearchProvider, TravelSearchBackend, build_providers, parse_baidu_html, parse_bing_html,
    parse_searxng_json,
};
pub use store::TravelStore;
