//! RSS 的 HTTP 接口（网页端）。
//!
//! 与桌面端 Tauri 命令共用 `devtoolbox_application::rss` 用例和同一份
//! `config/dashboard.db`；此前 8 个命令只有桌面桥，网页端只能显示「不支持」。
//!
//! 安全边界：API 自身仍是 loopback-only + Host/Origin guard，局域网访问必须
//! 经过带密码的网关。文章全文抓取额外做公网地址校验，避免把 authenticated
//! 的 HTTP 面变成访问本机/内网服务的 SSRF 代理。

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Path, Query};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{Value, json};

use devtoolbox_application::{
    ApplicationError, RssErrorKind, RssRepositoryPort, commit_new_feed, commit_refresh,
    delete_feed, feed_snapshots, fetch_all_feeds, fetch_new_feed, latest_articles, list_articles,
    list_feeds, mark_article_read, validate_feed_url,
};
use devtoolbox_runtime::composition::FeedFetcherAdapter;

use crate::ai_api::ApiError;

/// 组合根注入的 RSS 依赖（抓取器不是 Clone，单独用 Arc 包一层）。
/// `client` 必须配置 `redirect::Policy::none()`，逐跳校验由 `fetch_article` 负责。
pub struct RssDeps {
    pub repository: Arc<dyn RssRepositoryPort>,
    pub fetcher: Arc<FeedFetcherAdapter>,
    pub client: reqwest::Client,
}

fn rss_error(error: ApplicationError) -> ApiError {
    let code = match &error {
        ApplicationError::InvalidFeedUrl(_) | ApplicationError::DuplicateFeed(_) => "invalid",
        ApplicationError::FeedNotFound(_) => "not_found",
        ApplicationError::Rss { kind, .. } => match kind {
            RssErrorKind::Fetch => "rss_fetch_failed",
            RssErrorKind::Parse => "rss_parse_failed",
            RssErrorKind::Repository => "infrastructure_error",
        },
        _ => "rss_failed",
    };
    ApiError {
        code,
        message: error.to_string(),
    }
}

fn ok() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
pub struct UrlBody {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// `GET /api/v1/rss/feeds`
pub async fn list_feed_list(
    Extension(deps): Extension<Arc<RssDeps>>,
) -> Result<Json<Value>, ApiError> {
    let feeds = list_feeds(deps.repository.as_ref()).map_err(rss_error)?;
    Ok(Json(json!(feeds)))
}

/// `POST /api/v1/rss/feeds`，body `{ url }`
pub async fn add_feed(
    Extension(deps): Extension<Arc<RssDeps>>,
    Json(body): Json<UrlBody>,
) -> Result<Json<Value>, ApiError> {
    let normalized = validate_feed_url(&body.url).map_err(rss_error)?;
    let fetched = fetch_new_feed(&normalized, deps.fetcher.as_ref())
        .await
        .map_err(rss_error)?;
    let feed =
        commit_new_feed(deps.repository.as_ref(), &normalized, fetched).map_err(rss_error)?;
    Ok(Json(json!(feed)))
}

/// `POST /api/v1/rss/refresh`
pub async fn refresh_feeds(
    Extension(deps): Extension<Arc<RssDeps>>,
) -> Result<Json<Value>, ApiError> {
    let snapshots = feed_snapshots(deps.repository.as_ref()).map_err(rss_error)?;
    let results = fetch_all_feeds(&snapshots, deps.fetcher.as_ref()).await;
    let report = commit_refresh(deps.repository.as_ref(), results).map_err(rss_error)?;
    Ok(Json(json!(report)))
}

/// `GET /api/v1/rss/feeds/{feedId}/articles?limit=200`
pub async fn feed_articles(
    Extension(deps): Extension<Arc<RssDeps>>,
    Path(feed_id): Path<i64>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let articles = list_articles(deps.repository.as_ref(), feed_id, limit).map_err(rss_error)?;
    Ok(Json(json!(articles)))
}

/// `GET /api/v1/rss/articles/latest?limit=5`
pub async fn latest_feed_articles(
    Extension(deps): Extension<Arc<RssDeps>>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(5).clamp(1, 200);
    let articles = latest_articles(deps.repository.as_ref(), limit).map_err(rss_error)?;
    Ok(Json(json!(articles)))
}

/// `POST /api/v1/rss/articles/{articleId}/read`
pub async fn mark_read(
    Extension(deps): Extension<Arc<RssDeps>>,
    Path(article_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    mark_article_read(deps.repository.as_ref(), article_id).map_err(rss_error)?;
    Ok(ok())
}

/// `DELETE /api/v1/rss/feeds/{feedId}`
pub async fn remove_feed(
    Extension(deps): Extension<Arc<RssDeps>>,
    Path(feed_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    delete_feed(deps.repository.as_ref(), feed_id).map_err(rss_error)?;
    Ok(ok())
}

fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4() {
                return is_public_ip(&IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || first & 0xfe00 == 0xfc00 // fc00::/7 unique local
                || first & 0xffc0 == 0xfe80) // fe80::/10 link local
        }
    }
}

fn invalid(message: &str) -> ApiError {
    ApiError {
        code: "invalid",
        message: message.to_string(),
    }
}

/// 只做语法、协议与 localhost 名称检查；DNS/IP 检查留给 `validate_public_target`。
fn public_url_shape(raw: &str) -> Result<reqwest::Url, ApiError> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| invalid("invalid article url"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("invalid article url"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| invalid("invalid article url"))?;
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.") {
        return Err(invalid("localhost is not allowed"));
    }
    Ok(url)
}

/// IP 字面量直接校验；域名则要求所有解析结果均为公网地址。
fn validate_public_target(url: &reqwest::Url, resolved: &[IpAddr]) -> Result<(), ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| invalid("invalid article url"))?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(&ip) {
            return Err(invalid("private network address is not allowed"));
        }
        return Ok(());
    }
    if resolved.is_empty() || resolved.iter().any(|ip| !is_public_ip(ip)) {
        return Err(invalid("private network address is not allowed"));
    }
    Ok(())
}

/// 拒绝本机 / 内网 / 链路本地地址；DNS 名称逐个解析结果检查。
async fn public_url(raw: &str) -> Result<reqwest::Url, ApiError> {
    let url = public_url_shape(raw)?;
    let host = url
        .host_str()
        .ok_or_else(|| invalid("invalid article url"))?;
    if host.parse::<IpAddr>().is_ok() {
        validate_public_target(&url, &[])?;
        return Ok(url);
    }
    let port = url
        .port_or_known_default()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
    let resolved: Vec<IpAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| invalid(&format!("cannot resolve article host: {error}")))?
        .map(|address| address.ip())
        .collect();
    validate_public_target(&url, &resolved)?;
    Ok(url)
}

/// `POST /api/v1/articles/fetch`，body `{ url }` → HTML 文本。
///
/// `RssDeps.client` 必须关闭自动重定向：每一跳都重新做公网地址校验，
/// 否则「先公网后私网」的 302 就能把已认证的 HTTP 面变成内网探测代理。
pub async fn fetch_article(
    Extension(deps): Extension<Arc<RssDeps>>,
    Json(body): Json<UrlBody>,
) -> Result<Json<String>, ApiError> {
    let mut url = public_url(&body.url).await?;
    for _ in 0..5 {
        let response = deps
            .client
            .get(url.clone())
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
            .map_err(|error| ApiError {
                code: "rss_fetch_failed",
                message: error.to_string(),
            })?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| ApiError {
                    code: "rss_fetch_failed",
                    message: "redirect without location".to_string(),
                })?
                .to_str()
                .map_err(|error| ApiError {
                    code: "rss_fetch_failed",
                    message: error.to_string(),
                })?
                .to_string();
            url = public_url(
                url.join(&location)
                    .map_err(|error| ApiError {
                        code: "rss_fetch_failed",
                        message: error.to_string(),
                    })?
                    .as_str(),
            )
            .await?;
            continue;
        }
        if !response.status().is_success() {
            return Err(ApiError {
                code: "rss_fetch_failed",
                message: format!("server returned {}", response.status()),
            });
        }
        let bytes = response.bytes().await.map_err(|error| ApiError {
            code: "rss_fetch_failed",
            message: error.to_string(),
        })?;
        let end = bytes.len().min(5 * 1024 * 1024);
        return Ok(Json(String::from_utf8_lossy(&bytes[..end]).into_owned()));
    }
    Err(ApiError {
        code: "rss_fetch_failed",
        message: "too many redirects".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::*;

    fn assert_rejected(raw: &str, resolved: &[IpAddr]) {
        let url = match public_url_shape(raw) {
            Ok(url) => url,
            Err(error) => {
                assert_eq!(error.code, "invalid", "{raw}: {error:?}");
                return;
            }
        };
        let error = validate_public_target(&url, resolved)
            .expect_err("target must be rejected before any request");
        assert_eq!(error.code, "invalid", "{raw}: {error:?}");
    }

    #[test]
    fn rejects_private_loopback_link_local_and_non_http_targets() {
        for raw in [
            "",
            "ftp://example.com/article",
            "javascript:alert(1)",
            "http://localhost/article",
            "http://LOCALHOST./article",
            "http://127.0.0.1/article",
            "http://10.0.0.1/article",
            "http://192.168.1.1/article",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/article",
            "http://[fd00::1]/article",
            "http://[fe80::1]/article",
        ] {
            assert_rejected(raw, &[]);
        }
    }

    #[test]
    fn rejects_dns_names_resolving_to_private_addresses() {
        let url = public_url_shape("http://cdn.example.com/article").expect("public shape");
        let private = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let public = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
        assert!(validate_public_target(&url, &[public]).is_ok());
        assert!(validate_public_target(&url, &[public, private]).is_err());
        assert!(validate_public_target(&url, &[private]).is_err());
        assert!(validate_public_target(&url, &[]).is_err());

        let v6 = public_url_shape("http://v6.example.com/article").expect("v6 shape");
        assert!(
            validate_public_target(
                &v6,
                &[IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))]
            )
            .is_ok()
        );
    }
}
