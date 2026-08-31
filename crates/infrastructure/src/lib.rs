//! 本地文件系统、设置与 RSS 持久化适配器。

pub mod document_store;
pub mod error;
pub mod feed_fetcher;
pub mod history;
pub mod language;
pub mod rss_store;
pub mod settings_store;
pub mod travel;
pub mod workspace_scanner;

pub use document_store::{read_utf8, write_utf8_atomic};
pub use error::InfrastructureError;
pub use feed_fetcher::{FetchedEntry, FetchedFeed, feed_client, fetch_feed, parse_feed};
pub use history::HistoryStore;
pub use language::{LanguageStore, SearchHit, sources};
pub use rss_store::{ArticleRow, FeedRepository, FeedRow, now_unix};
pub use settings_store::{AppSettings, SettingsStore, TravelSettings};
pub use travel::{
    AmapPoiProvider, HttpWebFetcher, LlmConfig, LlmProvider, OpenAiCompatibleLlmProvider,
    QWeatherProvider, SearchOptions, SearchProvider, TravelDataProvider, TravelDataRequest,
    TravelSearchBackend, TravelStore, WebFetcher, build_providers, providers_for,
};
pub use workspace_scanner::{WorkspaceFile, scan_markdown_files};
