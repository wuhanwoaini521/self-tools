//! RSS 领域契约（Gate 7.6：`ArticleRow` 等行/抓取模型从 infrastructure 上移）。
//!
//! **Bounded context：个人订阅阅读器**（ADR-010）。本模块的行模型 =
//! `RssSubscription`（`FeedRow`）与 `RssEntry`（`ArticleRow`），语义是
//! 「用户主动决定自己订阅什么」。**不含任何 News 概念** —— 新闻归
//! `core::news`，两者共享 `infrastructure::feed_fetcher` 但领域不互通。
//!
//! 纯数据模型，无任何 I/O。infrastructure 的 SQLite 仓储与网络抓取适配器
//! 产出/消费这些类型；application 的 RSS 用例也只依赖这些类型与端口。
//! 共享摄取契约 [`crate::feed::FetchedEntry`] / [`crate::feed::FetchedFeed`] 在此。

/// 订阅源行（feeds 表）= **`RssSubscription`** 的载体。
///
/// 命名保留 `FeedRow`（引用量大，改名是纯 churn），但语义由 ADR-010 锁定
/// 为「一条 RSS 订阅」。用户自己加的博客/论坛/新闻源都在这里。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedRow {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub site_url: Option<String>,
    pub last_updated: Option<i64>,
    pub last_error: Option<String>,
    pub unread_count: i64,
}

/// 文章行（feeds × articles join；`feed_title` 冗余便于列表直接展示）= **`RssEntry`** 的载体。
///
/// 语义由 ADR-010 锁定为「一条订阅里的条目」。新闻条目不在此 —— 归 `core::news::NewsArticle`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArticleRow {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub guid: String,
    pub url: String,
    pub title: String,
    /// 署名（RSS `<author>` / Atom `<author><name>` / JSON Feed `author.name`）。
    pub author: Option<String>,
    /// 缩略图（MediaRSS `media:thumbnail` / itunes:image；无则回退正文首图）。
    pub image_url: Option<String>,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
    /// 用户收藏（稍后读）。**不是** Feed 数据：仅本地状态。
    pub starred: bool,
}
