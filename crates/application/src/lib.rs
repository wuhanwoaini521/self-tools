//! UI 无关的文档、工作区与任务编辑用例。

pub mod error;
pub mod geography;
pub mod history;

pub mod language;
pub mod personal_ai;
pub(crate) mod time;
pub mod rss;
pub mod travel;
pub mod workflows;

pub use error::{ApplicationError, RssErrorKind, TravelErrorKind, TravelFailure};
pub use geography::{
    GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRecommendation,
    GeoRelation, GeoRelationKind, GeoSearchGroup, GeoSource, GeographyHome, GeographyPortError,
    GeographyQueryPort, GeographyService,
};
pub use language::{LanguageInfo, LanguageSearchHit, LanguageService, TodayView};
pub use rss::{
    ArticleDto, FeedDto, FeedFetchError, FeedFetcherPort, FeedSnapshot, RefreshReport,
    RssRepositoryPort, commit_new_feed, commit_refresh, delete_feed, feed_snapshots,
    fetch_all_feeds, fetch_new_feed, latest_articles, list_articles, list_feeds,
    mark_article_read, validate_feed_url,
};
pub use travel::{TravelResearchRequest, TravelResearchService};
pub use workflows::{
    DocumentDto, cycle_lines, load_document, load_settings, save_document, save_settings,
    scan_workspace,
};
