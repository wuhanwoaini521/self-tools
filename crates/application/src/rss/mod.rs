//! RSS 用例端口与编排（Gate 7.6）。

pub mod ports;
pub mod workflows;

pub use ports::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort, RssRepositoryPort};
pub use workflows::{
    ArticleDto, FeedDto, FeedSnapshot, RefreshFailure, RefreshReport, commit_new_feed,
    commit_refresh, delete_feed, feed_snapshots, fetch_all_feeds, fetch_new_feed,
    latest_articles, list_articles, list_feeds, mark_article_read, validate_feed_url,
};
#[cfg(test)]
mod tests;
