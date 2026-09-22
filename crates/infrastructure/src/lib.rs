//! 本地文件系统、设置与 RSS 持久化适配器。

pub mod agents;
pub mod document_store;
pub mod documents;
pub mod error;
pub mod feed_fetcher;
pub mod files;
pub mod geography;
pub mod history;
pub mod history_enrichment;
pub mod language;
pub mod memory;
pub mod personal_ai;
pub mod rss_store;
pub mod server;
pub mod settings_store;
pub mod travel;
pub mod workspace_scanner;

pub use agents::{
    DEFAULT_BASE_URL, DEFAULT_MODEL, FakeJevTransport, JevConfig, JevDecisionProvider,
    JevHttpTransport, JevQuestion, JevRequestBody, JevResponseBody, JevTransport, JevUsage,
};
pub use document_store::{read_utf8, write_utf8_atomic};
pub use documents::{
    DOCUMENTS_SCHEMA_VERSION, DocumentIndexError, DocumentIndexSqliteStore, LocalDocumentSource,
    is_binary, modified_timestamp,
};
pub use error::InfrastructureError;
pub use server::ServerActionAuditSqlite;
pub use files::{FILES_SCHEMA_VERSION, FileIndexError, FileIndexSqliteStore, LocalFileSystem};
pub use memory::{MEMORY_SCHEMA_VERSION, MemorySqliteStore};
pub use feed_fetcher::{FetchedEntry, FetchedFeed, feed_client, fetch_feed, parse_feed};
pub use geography::GeographyStore;
pub use history::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult,
    HistoryDuckDbRepository, PeriodEventItem, PeriodPersonItem, PeriodResult, PersonEventResult,
    PersonPlaceResult, PersonRelationResult, PersonResult, PersonStoryResult, RegimeResult,
    SourceResult, StoryEventResult, StoryResult, WorkResult,
};
pub use history_enrichment::EnrichmentSqliteStore;
pub use language::{LanguageStore, SearchHit, sources};
pub use personal_ai::{AiModelConfig, OpenAiCompatibleChatModelProvider, parse_chat_response};
pub use rss_store::{ArticleRow, FeedRepository, FeedRow, now_unix};
pub use settings_store::SettingsStore;
pub use travel::{
    AmapPoiProvider, HttpWebFetcher, QWeatherProvider, SearchOptions, SearchProvider,
    TravelDataProvider, TravelDataRequest, TravelRoute, TravelRouteRequest, TravelSearchBackend,
    TravelStore, WebFetcher, build_providers, parse_amap_driving_route, providers_for,
};
pub use workspace_scanner::scan_markdown_files;

// 应用级中性契约（历史记录 / 设置 / 工作区文件）由 core 持有；
// 此处保留 re-export，使依赖 infrastructure 的代码无需改动。
pub use devtoolbox_core::settings::{AppSettings, GeographySettings, TravelSettings};
pub use devtoolbox_core::workspace::WorkspaceFile;
