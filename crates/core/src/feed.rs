//! 共享 Feed 摄取契约（ADR-010：RSS 与 News **共享基础设施，不共享领域语义**）。
//!
//! 本模块是 `FeedFetcherPort` / `feed-rs` 解析器的输出形状 —— 属于
//! **基础设施层契约**，不带任何产品语义：
//!
//! - RSS 用它落地成 `core::rss::ArticleRow`（`RssEntry`）；
//! - News 用它落地成 `core::news::NewsArticle`。
//!
//! 两者对同一 URL 的解析结果相同，但**存进各自的表、属于两个 bounded context**。
//! 同一个 URL 同时存在于 RssSubscription 与 NewsSource 是允许的。

use serde::{Deserialize, Serialize};

/// 归一化后的一篇文章（来自 RSS item、Atom entry 或 JSON Feed item）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedEntry {
    /// 去重主键：id → url → title+published 三级回退。
    pub guid: String,
    pub url: String,
    pub title: String,
    /// 署名（RSS `<author>` / Atom `<author><name>` / JSON Feed `author.name`）。
    pub author: Option<String>,
    /// 缩略图（MediaRSS `media:thumbnail` / itunes:image）。
    pub image_url: Option<String>,
    /// Unix 秒。
    pub published_at: Option<i64>,
    /// 展示正文：content 优先、summary 兜底（可能含 HTML，由前端净化后渲染）。
    pub summary: Option<String>,
}

/// 归一化后的一个 Feed。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub entries: Vec<FetchedEntry>,
}
