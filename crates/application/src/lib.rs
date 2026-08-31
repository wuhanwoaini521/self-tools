//! UI 无关的文档、工作区与任务编辑用例。

pub mod error;
pub mod history;
pub mod language;
pub mod rss_workflows;
pub mod travel;
pub mod workflows;

pub use error::ApplicationError;
pub use history::{HistoryHome, HistoryService};
pub use language::{LanguageInfo, LanguageSearchHit, LanguageService, TodayView};
pub use rss_workflows::{
    ArticleDto, FeedDto, FeedSnapshot, RefreshReport, commit_new_feed, commit_refresh, delete_feed,
    feed_snapshots, fetch_all_feeds, fetch_new_feed, latest_articles, list_articles, list_feeds,
    mark_article_read, validate_feed_url,
};
pub use travel::{TravelResearchRequest, TravelResearchService};
pub use workflows::{
    DocumentDto, convert_lines_to_tasks, cycle_lines, load_document, load_settings, save_document,
    save_settings, scan_workspace,
};
