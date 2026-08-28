//! RSS 抓取与解析适配器：reqwest 拉取 + feed-rs 统一解析 RSS 2.0 / Atom。
//!
//! 设计要点：
//! - 网络与解析分离：`parse_feed` 是纯函数，可离线测试；
//! - 解析产出归一化的 `FetchedFeed`，上游不感知 RSS/Atom 差异；
//! - guid 去重键按 `entry.id → 链接 → 标题+时间` 三级回退，保证稳定。

use std::time::Duration;

use feed_rs::model::{Feed as ParsedFeed, Text};
use feed_rs::parser;
use serde::{Deserialize, Serialize};

use crate::error::InfrastructureError;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("DevToolbox/", env!("CARGO_PKG_VERSION"), " (+rss reader)");

/// 归一化后的一篇文章（来自 RSS item 或 Atom entry）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedEntry {
    /// 去重主键：id → url → title+published 三级回退
    pub guid: String,
    pub url: String,
    pub title: String,
    /// Unix 秒
    pub published_at: Option<i64>,
    /// 原始摘要 / 内容（可能含 HTML，由前端净化后渲染）
    pub summary: Option<String>,
}

/// 归一化后的一个 Feed。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub entries: Vec<FetchedEntry>,
}

fn text_content(text: Option<Text>) -> Option<String> {
    text.map(|value| value.content.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn entry_link(entry: &feed_rs::model::Entry) -> String {
    entry
        .links
        .iter()
        .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
        .or_else(|| entry.links.first())
        .map(|link| link.href.clone())
        .unwrap_or_default()
}

fn entry_published_at(entry: &feed_rs::model::Entry) -> Option<i64> {
    entry
        .published
        .or(entry.updated)
        .map(|time| time.timestamp())
}

/// 去重键三级回退：entry.id → 链接 → 标题+发布时间。
fn entry_guid(entry: &feed_rs::model::Entry) -> String {
    if !entry.id.trim().is_empty() {
        return format!("id:{}", entry.id.trim());
    }
    let url = entry_link(entry);
    if !url.is_empty() {
        return format!("url:{url}");
    }
    let published = entry_published_at(entry).unwrap_or_default();
    format!(
        "title:{}@{}",
        entry
            .title
            .as_ref()
            .map_or(String::new(), |t| t.content.trim().to_string()),
        published
    )
}

fn normalize(parsed: ParsedFeed) -> FetchedFeed {
    let site_url = parsed
        .links
        .iter()
        .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
        .or_else(|| parsed.links.first())
        .map(|link| link.href.clone());
    FetchedFeed {
        title: text_content(parsed.title).unwrap_or_else(|| "Untitled Feed".to_string()),
        site_url,
        entries: parsed
            .entries
            .into_iter()
            .map(|entry| {
                let guid = entry_guid(&entry);
                let url = entry_link(&entry);
                let published_at = entry_published_at(&entry);
                let title = text_content(entry.title).unwrap_or_else(|| "(untitled)".to_string());
                let summary = text_content(entry.summary);
                FetchedEntry {
                    guid,
                    url,
                    title,
                    published_at,
                    summary,
                }
            })
            .collect(),
    }
}

/// 解析 RSS 2.0 / Atom 字节流（与网络解耦，便于离线测试）。
pub fn parse_feed(bytes: &[u8]) -> Result<FetchedFeed, InfrastructureError> {
    let parsed = parser::parse(bytes)
        .map_err(|source| InfrastructureError::FeedParse(source.to_string()))?;
    Ok(normalize(parsed))
}

/// 抓取并解析一个 Feed URL。超时、非 2xx、XML 异常均返回错误。
pub async fn fetch_feed(
    url: &str,
    client: &reqwest::Client,
) -> Result<FetchedFeed, InfrastructureError> {
    let response = client
        .get(url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|source| InfrastructureError::FeedFetch(source.to_string()))?;
    if !response.status().is_success() {
        return Err(InfrastructureError::FeedFetch(format!(
            "server returned {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|source| InfrastructureError::FeedFetch(source.to_string()))?;
    parse_feed(&bytes)
}

/// 全局共享的 HTTP 客户端（连接池 + 统一 UA）。
pub fn feed_client() -> Result<reqwest::Client, InfrastructureError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|source| InfrastructureError::FeedFetch(source.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{entry_guid, parse_feed};
    use feed_rs::model::{Entry, Link};

    const RSS_XML: &str = r#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
            <title>Tech Blog</title>
            <link>https://example.com/</link>
            <item>
                <title>Hello RSS</title>
                <link>https://example.com/hello</link>
                <guid>abc-123</guid>
                <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
                <description>First post</description>
            </item>
        </channel></rss>"#;

    const ATOM_XML: &str = r#"<?xml version="1.0"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
            <title>Atom Feed</title>
            <link rel="alternate" href="https://example.org/"/>
            <entry>
                <title>No Id Entry</title>
                <link rel="alternate" href="https://example.org/entry-1"/>
                <updated>2024-02-01T12:00:00Z</updated>
                <summary>Short summary</summary>
            </entry>
        </feed>"#;

    #[test]
    fn parses_rss_with_guid() {
        let feed = parse_feed(RSS_XML.as_bytes()).expect("parse rss");
        assert_eq!(feed.title, "Tech Blog");
        assert_eq!(feed.site_url.as_deref(), Some("https://example.com/"));
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0].guid, "id:abc-123");
        assert_eq!(feed.entries[0].title, "Hello RSS");
        assert!(feed.entries[0].published_at.is_some());
    }

    #[test]
    fn parses_atom_and_keeps_guid_stable_across_refreshes() {
        let first = parse_feed(ATOM_XML.as_bytes()).expect("parse atom");
        let second = parse_feed(ATOM_XML.as_bytes()).expect("parse atom again");
        assert_eq!(first.title, "Atom Feed");
        assert_eq!(first.entries[0].published_at, Some(1_706_788_800));
        // feed-rs 会为缺 id 的条目确定性生成 guid(基于 link+title),跨刷新稳定。
        assert_eq!(first.entries[0].guid, second.entries[0].guid);
        assert!(!first.entries[0].guid.is_empty());
    }

    #[test]
    fn guid_falls_back_to_url_then_title_and_time() {
        let entry_with_id = Entry {
            id: "real-id".into(),
            ..Entry::default()
        };
        assert_eq!(entry_guid(&entry_with_id), "id:real-id");

        let entry_with_link = Entry {
            id: String::new(),
            links: vec![Link {
                href: "https://example.com/post-1".to_string(),
                rel: None,
                media_type: None,
                href_lang: None,
                title: None,
                length: None,
            }],
            ..Entry::default()
        };
        assert_eq!(
            entry_guid(&entry_with_link),
            "url:https://example.com/post-1"
        );

        // 无 id 无链接:回退到 title+published 分支(Text 含私有字段,断言前缀即可)。
        let guid = entry_guid(&Entry::default());
        assert!(guid.starts_with("title:"), "unexpected guid: {guid}");
    }

    #[test]
    fn rejects_invalid_xml() {
        assert!(parse_feed(b"this is not xml").is_err());
    }
}
