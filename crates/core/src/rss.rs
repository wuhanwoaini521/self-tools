//! RSS 领域契约（Gate 7.6：`ArticleRow` 等行/抓取模型从 infrastructure 上移）。
//!
//! 纯数据模型，无任何 I/O。infrastructure 的 SQLite 仓储与网络抓取适配器
//! 产出/消费这些类型；application 的 RSS 用例也只依赖这些类型与端口。

use serde::{Deserialize, Serialize};

/// 订阅源行（feeds 表）。
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

/// 文章行（feeds × articles join；`feed_title` 冗余便于列表直接展示）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArticleRow {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub guid: String,
    pub url: String,
    pub title: String,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
}

/// 归一化后的一篇文章（来自 RSS item 或 Atom entry）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedEntry {
    /// 去重主键：id → url → title+published 三级回退。
    pub guid: String,
    pub url: String,
    pub title: String,
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
