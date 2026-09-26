//! 共享 Feed 摄取编排（ADR-010：RSS 与 News 共享基础设施，不共享领域语义）。
//!
//! [`fetch_many`] 是**并发抓取骨架**：给一组 URL，保序返回逐条结果，
//! **不含任何落库语义**。两个域各自 commit：
//!
//! - `rss::workflows::commit_refresh` → `feeds` / `articles`
//! - `news::service` 的 refresh → `news_sources` / `news_articles`
//!
//! 这样「重复」只发生在编排一次，领域语义仍然各自独立。

pub mod ports;

pub use ports::{FeedFetchError, FeedFetchErrorKind, FeedFetcherPort};

use devtoolbox_core::feed::FetchedFeed;
use futures_util::future::join_all;

/// 并发抓取一组 URL（保序；单条失败不影响其它）。
///
/// 返回 `(url, Result)` 顺序与 `urls` 一致 —— 调用方据此把结果与自己的
/// 领域对象（`FeedSnapshot` / `NewsSource`）重新配对。
pub async fn fetch_many<F: FeedFetcherPort + ?Sized>(
    urls: &[String],
    fetcher: &F,
) -> Vec<(String, Result<FetchedFeed, FeedFetchError>)> {
    let fetches = urls.iter().map(|url| async move {
        let result = fetcher.fetch_feed(url).await;
        (url.clone(), result)
    });
    join_all(fetches).await
}
