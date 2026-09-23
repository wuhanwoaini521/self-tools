//! UI 无关的文档、工作区与任务编辑用例。

pub mod agents;
pub mod backup;
pub mod documents;
#[cfg(test)]
pub mod e2e_journeys;
pub mod error;
pub mod files;
pub mod geography;
pub mod history;
pub mod knowledge;
pub mod language;
pub mod mcp;
pub mod memory;
pub mod personal_ai;
pub mod readiness;
pub mod rss;
pub mod search;
pub mod server;
pub mod study_board;
pub(crate) mod text;
pub(crate) mod time;
pub mod travel;
pub mod workflows;

pub use documents::{
    DocumentIndexPort, DocumentIndexStats, DocumentService, DocumentSourcePort, ExtractedContent,
    IndexReport, ScannedDocument,
};
pub use error::{ApplicationError, RssErrorKind, TravelErrorKind, TravelFailure};
pub use files::{
    FileIndexPort, FileIndexStats, FileQuery, FileReadOutcome, FileReadResult, FileService,
    FileSystemPort,
};
pub use geography::{
    GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRecommendation,
    GeoRelation, GeoRelationKind, GeoSearchGroup, GeoSource, GeographyHome, GeographyPortError,
    GeographyQueryPort, GeographyService,
};
pub use knowledge::{
    KnowledgeContext, KnowledgeMetrics, KnowledgeRetrievalService, KnowledgeSourceRetriever,
};
pub use language::{LanguageInfo, LanguageSearchHit, LanguageService, TodayView};
pub use memory::{MemoryConfig, MemoryService, MemoryStats, MemoryStorePort};
pub use rss::{
    ArticleDto, FeedDto, FeedFetchError, FeedFetcherPort, FeedSnapshot, RefreshReport,
    RssRepositoryPort, commit_new_feed, commit_refresh, delete_feed, feed_snapshots,
    fetch_all_feeds, fetch_new_feed, latest_articles, list_articles, list_feeds, mark_article_read,
    validate_feed_url,
};
pub use study_board::{StudyBoardStoreError, StudyBoardStorePort};
pub use travel::{TravelResearchRequest, TravelResearchService};
pub use workflows::{
    DocumentDto, cycle_lines, load_document, load_settings, save_document, save_settings,
    scan_workspace,
};
