//! Travel 基础设施适配器：搜索 / 抓取 / LLM / 数据 Provider / SQLite 缓存。

pub mod data_provider;
pub mod fetcher;
pub mod llm;
pub mod search;
pub mod store;

// Provider 契约（接口 / 请求 / 错误）由 core 定义（Gate 8：infrastructure 只实现、不定义）。
pub use data_provider::{
    AmapPoiProvider, QWeatherProvider, parse_amap_driving_route, providers_for,
};
pub use devtoolbox_core::settings::TravelSearchBackend;
pub use devtoolbox_core::travel::{
    AMAP_SOURCE_URL, LlmProvider, ProviderError, QWEATHER_SOURCE_URL, SearchOptions,
    SearchProvider, TravelDataProvider, TravelDataRequest, TravelRoute, TravelRouteRequest,
    WebFetcher,
};
pub use fetcher::{HttpWebFetcher, detect_encoding, extract_text};
pub use llm::{LlmConfig, OpenAiCompatibleLlmProvider, extract_chat_content};
pub use search::{
    BaiduSearchProvider, BingChinaSearchProvider, SearXngSearchProvider, build_providers,
    parse_baidu_html, parse_bing_html, parse_searxng_json,
};
pub use store::TravelStore;
