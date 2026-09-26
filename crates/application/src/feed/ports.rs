//! 共享 Feed 摄取端口（ADR-010：RSS 与 News **共享基础设施，不共享领域语义**）。
//!
//! 抓取能力属于基础设施，不属于任何产品域：
//!
//! - RSS 抓 `FeedSnapshot` 的 URL → 落 `feeds` / `articles`；
//! - News 抓 `NewsSource` 的 URL → 落 `news_sources` / `news_articles`。
//!
//! 两边都**只依赖本端口 + `core::feed::{FetchedFeed, FetchedEntry}` 归一契约**，
//! 不互相依赖对方的 repository。

use devtoolbox_core::feed::{FetchedEntry, FetchedFeed};

pub use devtoolbox_core::feed::{FetchedEntry as FeedEntry, FetchedFeed as FeedResult};

/// 抓取失败分类：传输失败（网络/DNS/超时）与解析失败（非 Feed 内容）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedFetchErrorKind {
    Fetch,
    Parse,
}

/// 抓取端口错误：应用层可见的传输失败描述。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedFetchError {
    pub kind: FeedFetchErrorKind,
    pub message: String,
}

impl std::fmt::Display for FeedFetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl FeedFetchError {
    /// 稳定分类标签（错误文本前缀；News / RSS 共用）。
    #[must_use]
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            FeedFetchErrorKind::Fetch => "fetch",
            FeedFetchErrorKind::Parse => "parse",
        }
    }
}

/// 抓取端口（异步；具体传输在 runtime 适配器）。
///
/// 显式 `+ Send`：既有实现（`FeedFetcherAdapter` 持 `reqwest::Client`）的
/// future 本就是 `Send`，这里把隐含能力写成契约 —— 让该端口可被任何要求
/// `Send` 的 async 入口（Tauri 命令 / `ToolExecutor::execute`）消费。
/// 非 dyn-compatible（RPITIT）：调用方只通过具体类型使用，符合既有约定。
#[allow(async_fn_in_trait)]
pub trait FeedFetcherPort: Send + Sync {
    fn fetch_feed(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<FetchedFeed, FeedFetchError>> + Send;
}

/// `FetchedEntry` 便捷别名（News 侧引用共享契约时用更短的名字）。
pub type SharedFeedEntry = FetchedEntry;
