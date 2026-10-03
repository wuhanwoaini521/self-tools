//! 共享端口适配器：HTTP 抓取 → `FeedFetcherPort`。

use devtoolbox_application::feed::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort};
use devtoolbox_core::feed::FetchedFeed;

use crate::error::InfrastructureError;

/// 把共享 `reqwest::Client` + `fetch_feed` 包装成 application 的抓取端口。
pub struct FeedFetcherAdapter {
    client: reqwest::Client,
}

impl FeedFetcherAdapter {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl FeedFetcherPort for FeedFetcherAdapter {
    async fn fetch_feed(&self, url: &str) -> Result<FetchedFeed, FeedFetchError> {
        crate::feed_fetcher::fetch_feed(url, &self.client)
            .await
            .map_err(|error| {
                let (kind, message) = match &error {
                    InfrastructureError::FeedFetch(message) => {
                        (FeedFetchErrorKind::Fetch, message.clone())
                    }
                    InfrastructureError::FeedParse(message) => {
                        (FeedFetchErrorKind::Parse, message.clone())
                    }
                    other => (FeedFetchErrorKind::Fetch, other.to_string()),
                };
                FeedFetchError { kind, message }
            })
    }
}
