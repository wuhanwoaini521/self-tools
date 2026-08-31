//! 国内可访问的搜索 Provider 抽象与实现（需求 #三 / #五）。
//!
//! `SearchProvider` 为统一接口；V1 提供三个实现，足够“主 + 备 + 可自托管”：
//! - `BingChinaSearchProvider`：默认主 Provider（`cn.bing.com`，国内可访问、结果 URL 直达）；
//! - `BaiduSearchProvider`：备用 Provider（结果 URL 为百度跳转链接，如实保留）；
//! - `SearXngSearchProvider`：本地 SearXNG（`format=json` 原生 JSON，推荐自托管）。
//!
//! **反爬约定**（需求 #四）：不做验证码 / WAF 绕过 / 指纹伪装；
//! 抓不到完整页面时由上层保留搜索摘要作为低可信度信息源。

use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::InfrastructureError;
use devtoolbox_core::travel::SearchResult;

/// 搜索后端配置（来自应用设置，映射前端 TravelSettings）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelSearchBackend {
    /// 自动：Bing 国内 > 百度
    #[default]
    Auto,
    /// 本地 SearXNG（需配置 `searxng_url`）
    Searxng,
    /// 仅百度
    Baidu,
    /// 仅必应
    Bing,
}

/// 搜索选项。
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchOptions {
    /// 期望的结果数量上限（Provider 尽力而为）。
    pub count: usize,
}

/// 统一搜索接口。`Box<dyn>` 使用，需 `Send + Sync`。
#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, InfrastructureError>;
}

/// 依据配置构建有序 Provider 链（Auto 先 Bing 后百度；上层按序 fallback）。
#[must_use]
pub fn build_providers(
    backend: TravelSearchBackend,
    searxng_url: Option<String>,
    client: &reqwest::Client,
) -> Vec<Box<dyn SearchProvider>> {
    let mut providers: Vec<Box<dyn SearchProvider>> = Vec::new();
    match backend {
        TravelSearchBackend::Auto => {
            providers.push(Box::new(BingChinaSearchProvider::new(client.clone())));
            providers.push(Box::new(BaiduSearchProvider::new(client.clone())));
        }
        TravelSearchBackend::Bing => {
            providers.push(Box::new(BingChinaSearchProvider::new(client.clone())));
        }
        TravelSearchBackend::Baidu => {
            providers.push(Box::new(BaiduSearchProvider::new(client.clone())));
        }
        TravelSearchBackend::Searxng => {
            if let Some(url) = searxng_url.filter(|s| !s.trim().is_empty()) {
                providers.push(Box::new(SearXngSearchProvider::new(client.clone(), url)));
            }
            // SearXNG 未配置 / 不可用时的自动备用
            providers.push(Box::new(BingChinaSearchProvider::new(client.clone())));
        }
    }
    providers
}

const SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
const USER_AGENT: &str = concat!(
    "DevToolbox/",
    env!("CARGO_PKG_VERSION"),
    " (+travel research)"
);

/// Bing 中国（cn.bing.com）。
pub struct BingChinaSearchProvider {
    client: reqwest::Client,
}

impl BingChinaSearchProvider {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SearchProvider for BingChinaSearchProvider {
    fn name(&self) -> &'static str {
        "bing"
    }

    async fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, InfrastructureError> {
        let url = Url::parse_with_params("https://cn.bing.com/search", &[("q", query)])
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header("accept-language", "zh-CN,zh;q=0.9")
            .timeout(SEARCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        if !response.status().is_success() {
            return Err(InfrastructureError::TravelSearch(format!(
                "bing returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        Ok(parse_bing_html(
            &String::from_utf8_lossy(&bytes),
            options.count,
            now_unix(),
        ))
    }
}

/// 百度。
pub struct BaiduSearchProvider {
    client: reqwest::Client,
}

impl BaiduSearchProvider {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SearchProvider for BaiduSearchProvider {
    fn name(&self) -> &'static str {
        "baidu"
    }

    async fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, InfrastructureError> {
        let url = Url::parse_with_params("https://www.baidu.com/s", &[("wd", query)])
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header("accept-language", "zh-CN,zh;q=0.9")
            .timeout(SEARCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        if !response.status().is_success() {
            return Err(InfrastructureError::TravelSearch(format!(
                "baidu returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        Ok(parse_baidu_html(
            &String::from_utf8_lossy(&bytes),
            options.count,
            now_unix(),
        ))
    }
}

/// 本地 SearXNG（JSON API）。
pub struct SearXngSearchProvider {
    client: reqwest::Client,
    base: String,
}

impl SearXngSearchProvider {
    #[must_use]
    pub fn new(client: reqwest::Client, base: String) -> Self {
        Self {
            client,
            base: base.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl SearchProvider for SearXngSearchProvider {
    fn name(&self) -> &'static str {
        "searxng"
    }

    async fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, InfrastructureError> {
        let url = Url::parse_with_params(
            &format!("{}/search", self.base),
            &[("q", query), ("format", "json")],
        )
        .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .timeout(SEARCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        if !response.status().is_success() {
            return Err(InfrastructureError::TravelSearch(format!(
                "searxng returned {} (JSON API 可能被禁用，请检查 SearXNG 设置)",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
        parse_searxng_json(&bytes, options.count, now_unix())
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

// ---------- 纯解析函数（离线可单测） ----------

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex"))
}

/// 提取干净的文本节点（去标签 / 常见实体，压缩空白）。
fn clean_text(raw: &str) -> String {
    let without_tags = tag_re().replace_all(raw, " ");
    let decoded = without_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 结果块基本结构：`<a href="URL">TITLE</a>`（Bing 的 `h2 a` 或百度的 `h3 a`）。
fn result_anchor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<a[^>]+href="(https?://[^"]+)"[^>]*>(.*?)</a>"#).expect("anchor regex")
    })
}

/// 解析 Bing 搜索结果 HTML（按 `<li class="b_algo"` 分块，摘要取第一个 `<p>`）。
#[must_use]
pub fn parse_bing_html(html: &str, count: usize, fetched_at: i64) -> Vec<SearchResult> {
    parse_result_blocks(html, "<li class=\"b_algo", "bing", count, fetched_at)
}

/// 解析百度搜索结果 HTML（按 `<h3` 分块，摘要取 `c-abstract`）。
#[must_use]
pub fn parse_baidu_html(html: &str, count: usize, fetched_at: i64) -> Vec<SearchResult> {
    parse_result_blocks(html, "<h3", "baidu", count, fetched_at)
}

fn parse_result_blocks(
    html: &str,
    block_marker: &str,
    provider: &str,
    count: usize,
    fetched_at: i64,
) -> Vec<SearchResult> {
    let anchor = result_anchor_re();
    let mut results = Vec::new();
    for block in html.split(block_marker).skip(1) {
        if results.len() >= count {
            break;
        }
        let Some(captures) = anchor.captures(block) else {
            continue;
        };
        let title = clean_text(captures.get(2).map_or("", |m| m.as_str()));
        if title.is_empty() {
            continue;
        }
        let url = captures.get(1).map_or("", |m| m.as_str()).to_string();
        if url.is_empty() {
            continue;
        }
        let snippet = if provider == "baidu" {
            block
                .split("c-abstract")
                .nth(1)
                .map(|part| {
                    let capped = &part[..part.len().min(600)];
                    clean_text(capped.split("</div>").next().unwrap_or(capped))
                })
                .unwrap_or_default()
        } else {
            block
                .split("<p")
                .nth(1)
                .map(|part| {
                    let capped = &part[..part.len().min(600)];
                    clean_text(capped.split("</p>").next().unwrap_or(capped))
                })
                .unwrap_or_default()
        };
        results.push(SearchResult {
            title,
            url,
            snippet,
            provider: provider.to_string(),
            published_at: None,
            fetched_at,
        });
    }
    results
}

/// 解析 SearXNG JSON API 返回。
pub fn parse_searxng_json(
    bytes: &[u8],
    count: usize,
    fetched_at: i64,
) -> Result<Vec<SearchResult>, InfrastructureError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| InfrastructureError::TravelSearch(error.to_string()))?;
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            InfrastructureError::TravelSearch("searxng response missing results".to_string())
        })?;
    Ok(results
        .iter()
        .take(count)
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let title = item.get("title")?.as_str()?.to_string();
            let snippet = item
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(SearchResult {
                title,
                url,
                snippet,
                provider: "searxng".to_string(),
                published_at: None,
                fetched_at,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{parse_baidu_html, parse_bing_html, parse_searxng_json};

    const BING_HTML: &str = r#"<html><body><ol id="b_results">
      <li class="b_algo"><h2><a href="https://hz.gov.cn/museum">杭州博物馆官网</a></h2>
        <div class="b_caption"><p>开放时间 门票 预约信息。</p></div></li>
      <li class="b_algo"><h2><a href="https://you.ctrip.com/hangzhou">杭州攻略-携程</a></h2>
        <div class="b_caption"><p>三日游路线 &amp; 美食推荐。</p></div></li>
    </ol></body></html>"#;

    const BAIDU_HTML: &str = r#"<html><body>
      <div class="result"><h3 class="t"><a href="https://www.baidu.com/link?url=abc">杭州 百度百科</a></h3>
        <div class="c-abstract">杭州是浙江省省会，历史名城。</div></div>
      <div class="result"><h3 class="t"><a href="https://example.com/food">杭州小吃</a></h3>
        <div class="c-abstract">本地人推荐的老字号。</div></div>
    </body></html>"#;

    #[test]
    fn bing_html_parse_extracts_results() {
        let results = parse_bing_html(BING_HTML, 10, 1_700_000_000);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "杭州博物馆官网");
        assert_eq!(results[0].url, "https://hz.gov.cn/museum");
        assert!(results[0].snippet.contains("开放时间"));
        assert_eq!(results[1].provider, "bing");
    }

    #[test]
    fn bing_html_respects_count() {
        let results = parse_bing_html(BING_HTML, 1, 1_700_000_000);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn baidu_html_parse_extracts_results_with_redirect_url() {
        let results = parse_baidu_html(BAIDU_HTML, 10, 1_700_000_000);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "杭州 百度百科");
        // 百度跳转链接如实保留，不做绕过
        assert_eq!(results[0].url, "https://www.baidu.com/link?url=abc");
        assert!(results[0].snippet.contains("浙江省省会"));
    }

    #[test]
    fn empty_html_yields_no_results() {
        assert!(parse_bing_html("<html></html>", 10, 1).is_empty());
        assert!(parse_baidu_html("<html></html>", 10, 1).is_empty());
    }

    #[test]
    fn searxng_json_parses_results() {
        let body = r#"{"results": [
            {"url": "https://sz.gov.example/a", "title": "深圳 景点 A", "content": "介绍 A"},
            {"url": "https://b.example/b", "title": "深圳 攻略", "content": ""}
        ]}"#;
        let results = parse_searxng_json(body.as_bytes(), 10, 1_700_000_000).expect("parse");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].provider, "searxng");
        assert_eq!(results[0].snippet, "介绍 A");
    }

    #[test]
    fn searxng_invalid_json_is_an_error() {
        assert!(parse_searxng_json(b"not json", 10, 1).is_err());
    }
}
