//! RSS 抓取与解析适配器:reqwest 拉取 + feed-rs 统一解析 RSS 2.0 / Atom / JSON Feed。
//!
//! 处理"野生"Feed 的惯例(参考 Miniflux / Universal Feed Parser 的做法):
//! - 相对 URL 以 Feed 地址为 base 解析(links / 站点链接;HTML 正文由前端净化时补全);
//! - 展示正文优先完整 `content`(RSS content:encoded / Atom content),缺失才回退摘要;
//! - 请求传原始 bytes,由 feed-rs 按 XML prolog 解码(兼容 GBK 等非 UTF-8 源);
//! - 把网站首页误当 Feed 添加时,通过 HTML 嗅探给出明确错误;
//! - guid 去重键按 `entry.id → 链接 → 标题+时间` 三级回退,保证稳定。

use std::time::Duration;

use feed_rs::model::{Feed as ParsedFeed, Text};
use feed_rs::parser;
use serde::{Deserialize, Serialize};

use crate::error::InfrastructureError;

const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("DevToolbox/", env!("CARGO_PKG_VERSION"), " (+rss reader)");

/// 归一化后的一篇文章(来自 RSS item 或 Atom entry)。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FetchedEntry {
    /// 去重主键:id → url → title+published 三级回退
    pub guid: String,
    pub url: String,
    pub title: String,
    /// Unix 秒
    pub published_at: Option<i64>,
    /// 展示正文:content 优先、summary 兜底(可能含 HTML,由前端净化后渲染)
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

/// 展示正文:优先完整 `content`(Miniflux 惯例),缺失时回退 `summary`。
/// 注意 `<content src="..."/>` 表示外链正文(body 为空),此时仍视为无内容。
fn entry_body(entry: &feed_rs::model::Entry) -> Option<String> {
    entry
        .content
        .as_ref()
        .and_then(|content| content.body.as_ref())
        .map(|body| body.trim().to_string())
        .filter(|body| !body.is_empty())
        .or_else(|| text_content(entry.summary.clone()))
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

/// 去重键三级回退:entry.id → 链接 → 标题+发布时间。
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

/// 相对引用解析为绝对地址(feedparser/Miniflux 的标准行为);已是绝对地址则原样返回。
fn absolutize(candidate: &str, base_uri: Option<&str>) -> String {
    if candidate.is_empty() {
        return candidate.to_string();
    }
    if url::Url::parse(candidate).is_ok() {
        return candidate.to_string();
    }
    base_uri
        .and_then(|base| url::Url::parse(base).ok())
        .and_then(|base| base.join(candidate).ok())
        .map(|url| url.to_string())
        .unwrap_or_else(|| candidate.to_string())
}

/// Feed 标题兜底:从 Feed 地址提取域名(Miniflux 惯例)。
fn hostname(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_string)
}

fn normalize(parsed: ParsedFeed, base_uri: Option<&str>) -> FetchedFeed {
    let site_url = parsed
        .links
        .iter()
        .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
        .or_else(|| parsed.links.first())
        .map(|link| absolutize(&link.href, base_uri))
        .filter(|href| !href.is_empty());
    FetchedFeed {
        title: text_content(parsed.title)
            .or_else(|| base_uri.and_then(hostname))
            .unwrap_or_else(|| "Untitled Feed".to_string()),
        site_url,
        entries: parsed
            .entries
            .into_iter()
            .map(|entry| {
                let guid = entry_guid(&entry);
                let url = absolutize(&entry_link(&entry), base_uri);
                let published_at = entry_published_at(&entry);
                let summary = entry_body(&entry);
                let title = text_content(entry.title).unwrap_or_else(|| "(untitled)".to_string());
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

/// 解析字节流(无 base)。与网络解耦,便于离线测试。
pub fn parse_feed(bytes: &[u8]) -> Result<FetchedFeed, InfrastructureError> {
    parse_feed_with_base(bytes, None)
}

/// 解析字节流,并以 Feed 地址为 base 解析相对引用。
pub fn parse_feed_with_base(
    bytes: &[u8],
    base_uri: Option<&str>,
) -> Result<FetchedFeed, InfrastructureError> {
    let parsed = parser::Builder::new()
        .base_uri(base_uri)
        .build()
        .parse(bytes)
        .map_err(|source| InfrastructureError::FeedParse(source.to_string()))?;
    Ok(normalize(parsed, base_uri))
}

/// 粗判「返回的是网页而不是 Feed」:前 1KB 里没有任何 Feed 痕迹,却是 HTML 文档。
/// 仅看正文特征,不看 content-type——不少正规博客的 Feed 就以 text/html 提供。
fn looks_like_html(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    if head.contains("<?xml")
        || head.contains("<feed")
        || head.contains("<rss")
        || head.contains("<rdf")
    {
        return false;
    }
    let compact = head.trim_start();
    compact.contains("<!doctype html") || compact.contains("<html")
}

/// 抓取并解析一个 Feed URL。超时、非 2xx、HTML 页面、XML 异常均返回错误。
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
    // 传原始 bytes:feed-rs 依据 XML prolog 解码(支持非 UTF-8 源);text() 会破坏编码。
    let bytes = response
        .bytes()
        .await
        .map_err(|source| InfrastructureError::FeedFetch(source.to_string()))?;
    if looks_like_html(&bytes) {
        return Err(InfrastructureError::FeedFetch(format!(
            "the URL returned a web page instead of a feed: {url}"
        )));
    }
    parse_feed_with_base(&bytes, Some(url))
}

/// 全局共享的 HTTP 客户端(连接池 + 统一 UA)。
pub fn feed_client() -> Result<reqwest::Client, InfrastructureError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|source| InfrastructureError::FeedFetch(source.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{entry_guid, looks_like_html, parse_feed, parse_feed_with_base};
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

    #[test]
    fn uses_content_when_summary_missing() {
        // V2EX 等 Atom 源:无 <summary>,正文在 <content type="html">。
        let atom_content_only = r#"<?xml version="1.0"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Content Only</title>
                <entry>
                    <title>With Content</title>
                    <link rel="alternate" href="https://example.org/c1"/>
                    <id>tag:example.org,2024:/c1</id>
                    <content type="html">&lt;p&gt;Full body here&lt;/p&gt;</content>
                </entry>
            </feed>"#;
        let feed = parse_feed(atom_content_only.as_bytes()).expect("parse atom");
        assert_eq!(
            feed.entries[0].summary.as_deref(),
            Some("<p>Full body here</p>")
        );
    }

    #[test]
    fn external_content_src_is_not_treated_as_body() {
        let atom_linked_content = r#"<?xml version="1.0"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <title>Linked Content</title>
                <entry>
                    <title>Outbound</title>
                    <link rel="alternate" href="https://example.org/c2"/>
                    <id>tag:example.org,2024:/c2</id>
                    <content src="https://cdn.example.org/article.html"/>
                </entry>
            </feed>"#;
        let feed = parse_feed(atom_linked_content.as_bytes()).expect("parse atom");
        assert_eq!(feed.entries[0].summary, None);
    }

    #[test]
    fn prefers_full_content_over_excerpt_summary() {
        // RSS 2.0: description=摘要、content:encoded=全文 → 展示取全文(Miniflux 惯例)。
        let rss_both = r#"<?xml version="1.0"?>
            <rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel>
                <title>Both</title>
                <item>
                    <title>Post</title>
                    <guid>both-1</guid>
                    <description>short excerpt</description>
                    <content:encoded><![CDATA[<p>FULL ARTICLE</p>]]></content:encoded>
                </item>
            </channel></rss>"#;
        let feed = parse_feed(rss_both.as_bytes()).expect("parse rss");
        assert_eq!(
            feed.entries[0].summary.as_deref(),
            Some("<p>FULL ARTICLE</p>")
        );
    }

    #[test]
    fn relative_links_resolved_against_feed_uri() {
        // feedparser/Miniflux 惯例:相对链接以 Feed 地址为 base 解析。
        let rss_relative = r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
                <title>Relative</title>
                <item>
                    <title>Post</title>
                    <link>/posts/hello</link>
                    <guid>rel-1</guid>
                </item>
            </channel></rss>"#;
        let feed = parse_feed_with_base(
            rss_relative.as_bytes(),
            Some("https://blog.example.com/feed.xml"),
        )
        .expect("parse rss");
        assert_eq!(feed.entries[0].url, "https://blog.example.com/posts/hello");
    }

    #[test]
    fn feed_title_falls_back_to_hostname() {
        let rss_no_title = r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
                <item><title>Only Post</title><guid>t-1</guid></item>
            </channel></rss>"#;
        let feed = parse_feed_with_base(
            rss_no_title.as_bytes(),
            Some("https://news.example.org/feed"),
        )
        .expect("parse rss");
        assert_eq!(feed.title, "news.example.org");
    }

    #[test]
    fn html_pages_are_detected_but_feeds_are_not() {
        assert!(looks_like_html(
            b"<!DOCTYPE html><html><body>homepage</body></html>"
        ));
        assert!(looks_like_html(b"\n\n<html lang=\"zh\"></html>"));
        assert!(!looks_like_html(
            b"<?xml version=\"1.0\"?><rss version=\"2.0\"></rss>"
        ));
        assert!(!looks_like_html(
            b"<!-- generator -->\n<feed xmlns=\"http://www.w3.org/2005/Atom\">"
        ));
        assert!(!looks_like_html(b"<?xml version=\"1.0\" encoding=\"gb2312\"?><rss version=\"2.0\"><channel><title>GB</title></channel></rss>"));
    }
}
