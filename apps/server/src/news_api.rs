//! News 的 HTTP 接口（网页端）。
//!
//! 重点：**推荐订阅源目录是 `core::news::recommended_sources()` 里的纯函数**
//! （22 条，跨五个分类），不依赖数据库也不依赖网络。网页端看不到推荐源，
//! 纯粹是因为这个端点没接上——不是数据缺失。
//!
//! 桌面端 `news_recommended` 会先滤掉「用户已订阅」的 url，这里保持同样语义：
//! 列表来源唯一（core 目录），不重复订阅。

use std::sync::Arc;

use devtoolbox_application::news::{NewsCategory, NewsIngestPort, NewsPort, NewsService};
use serde::Deserialize;

use crate::ai_api::ApiError;

/// `GET /api/v1/news/recommended`
pub async fn recommended(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let view = news.sources().map_err(|error| ApiError {
        code: "news_failed",
        message: error.to_string(),
    })?;
    let existing: Vec<String> = view
        .sources
        .iter()
        .map(|source| source.url.clone())
        .collect();

    let list: Vec<serde_json::Value> = devtoolbox_application::news::recommended_sources()
        .into_iter()
        .filter(|candidate| !existing.contains(&candidate.url))
        .map(|candidate| {
            serde_json::json!({
                "name": candidate.name,
                "url": candidate.url,
                "site_url": candidate.site_url,
                "category": candidate.category,
                "note": candidate.note,
                "verified_on": candidate.verified_on,
            })
        })
        .collect();
    // 前端契约：`news_recommended` 返回**裸数组**（与桌面端一致）。
    Ok(axum::Json(serde_json::json!(list)))
}

/// `GET /api/v1/news/sources` —— 已订阅源 + 概览。
pub async fn sources(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let view = news.sources().map_err(|error| ApiError {
        code: "news_failed",
        message: error.to_string(),
    })?;
    let items: Vec<serde_json::Value> = view.sources.iter().map(source_json).collect();
    Ok(axum::Json(serde_json::json!({
        "sources": items,
        "health": match view.health {
            devtoolbox_application::news::NewsSourceHealth::Degraded => "degraded",
            devtoolbox_application::news::NewsSourceHealth::Healthy => "healthy",
        },
    })))
}

/// `GET /api/v1/news/headlines?scope=all|starred|by_category&limit=30`
///
/// scope 语义与桌面端 `news_headlines` 一致；默认 `all`。
pub async fn headlines(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Query(params): axum::extract::Query<HeadlinesQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let limit = params.limit.unwrap_or(30).clamp(1, 200) as i64;
    let articles = match params.scope.as_deref().unwrap_or("all") {
        "starred" => news.starred(limit),
        "by_category" => {
            let Some(category) = params.category.as_deref().and_then(parse_category) else {
                return Err(ApiError {
                    code: "invalid",
                    message: "scope=by_category 需要合法的 category".to_string(),
                });
            };
            news.by_category(category, limit)
        }
        "by_source" => {
            let Some(source_id) = params.source_id else {
                return Err(ApiError {
                    code: "invalid",
                    message: "scope=by_source 需要 source_id".to_string(),
                });
            };
            news.by_source(source_id, limit)
        }
        _ => news.latest(limit),
    }
    .map_err(|error| ApiError {
        code: "news_failed",
        message: error.to_string(),
    })?;
    Ok(axum::Json(serde_json::json!(
        articles.iter().map(article_json).collect::<Vec<_>>()
    )))
}

/// `GET /api/v1/news/search?q=…&limit=20`
pub async fn search(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let query = params.q.trim().to_string();
    if query.is_empty() {
        return Ok(axum::Json(serde_json::json!(
            Vec::<serde_json::Value>::new()
        )));
    }
    let articles = news
        .search(&query, params.limit.unwrap_or(20).clamp(1, 200) as i64)
        .map_err(|error| ApiError {
            code: "news_failed",
            message: error.to_string(),
        })?;
    Ok(axum::Json(serde_json::json!(
        articles.iter().map(article_json).collect::<Vec<_>>()
    )))
}

/// `GET /api/v1/news/starred?limit=20`
pub async fn starred(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Query(params): axum::extract::Query<LimitQuery>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let articles = news
        .starred(params.limit.unwrap_or(20).clamp(1, 200) as i64)
        .map_err(|error| ApiError {
            code: "news_failed",
            message: error.to_string(),
        })?;
    Ok(axum::Json(serde_json::json!(
        articles.iter().map(article_json).collect::<Vec<_>>()
    )))
}

/// `POST /api/v1/news/sources` —— 订阅一个源。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSourceRequest {
    pub url: String,
    #[serde(default)]
    pub category: Option<String>,
}

/// `POST /api/v1/news/refresh`
///
/// **网页端此前完全没有刷新入口**：`news_refresh_now` 只是 Tauri 命令，
/// transport 里没有对应映射，于是浏览器打开 News 页点刷新只会得到
/// 「尚无网页端接口」—— 这台机器明明就是服务器，抓取链路也全在本地。
///
/// 现在补上：与桌面端调用**同一个** `NewsIngestPort::refresh`。
pub async fn refresh(
    axum::Extension(ingest): axum::Extension<Arc<dyn NewsIngestPort>>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let report = ingest.refresh().await.map_err(|error| ApiError {
        code: "news_refresh_failed",
        message: error.to_string(),
    })?;
    Ok(axum::Json(serde_json::json!({
        "new_articles": report.new_articles,
        "failures": report
            .failures
            .iter()
            .map(|failure| serde_json::json!({
                "source": failure.source,
                "message": failure.message,
            }))
            .collect::<Vec<_>>(),
    })))
}

pub async fn add_source(
    axum::Extension(ingest): axum::Extension<Arc<dyn NewsIngestPort>>,
    axum::Json(request): axum::Json<AddSourceRequest>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let category = request
        .category
        .as_deref()
        .and_then(parse_category)
        .unwrap_or(NewsCategory::World);
    let source = ingest
        .add_source(&request.url, category)
        .await
        .map_err(|error| ApiError {
            code: "news_add_source_failed",
            message: error.to_string(),
        })?;
    Ok(axum::Json(
        serde_json::json!({ "source": source_json(&source) }),
    ))
}

/// `POST /api/v1/news/sources/{id}/remove` —— 取消订阅。
pub async fn remove_source(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Path(source_id): axum::extract::Path<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    news.remove_source(source_id).map_err(|error| ApiError {
        code: "news_remove_source_failed",
        message: error.to_string(),
    })?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/v1/news/articles/{id}/star` —— 收藏 / 取消。
pub async fn toggle_star(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Path(article_id): axum::extract::Path<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let starred = news.toggle_star(article_id).map_err(|error| ApiError {
        code: "news_failed",
        message: error.to_string(),
    })?;
    Ok(axum::Json(serde_json::json!({ "starred": starred })))
}

/// `POST /api/v1/news/articles/{id}/read` —— 标记已读。
pub async fn mark_read(
    axum::Extension(news): axum::Extension<Arc<NewsService>>,
    axum::extract::Path(article_id): axum::extract::Path<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    news.mark_read(article_id).map_err(|error| ApiError {
        code: "news_failed",
        message: error.to_string(),
    })?;
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct HeadlinesQuery {
    pub scope: Option<String>,
    pub source_id: Option<i64>,
    pub category: Option<String>,
    pub limit: Option<usize>,
}

/// 分类字符串 → 枚举（与桌面端 `parse_news_category` 同语义）。
fn parse_category(raw: &str) -> Option<NewsCategory> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "general" | "综合" => Some(NewsCategory::General),
        "tech" | "科技" => Some(NewsCategory::Tech),
        "finance" | "财经" => Some(NewsCategory::Finance),
        "world" | "国际" => Some(NewsCategory::World),
        "china" | "中国" => Some(NewsCategory::China),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

/// 新闻源 JSON —— **必须与桌面端 `news_source_json` 逐字段一致**，
/// 否则同一个前端类型在两端会解析出不同形状。
fn source_json(source: &devtoolbox_application::news::NewsSource) -> serde_json::Value {
    serde_json::json!({
        "id": source.id,
        "name": source.name,
        "url": source.url,
        "source_type": source.source_type.id(),
        "category": source.category.id(),
        "category_label": source.category.label(),
        "site_url": source.site_url,
        "last_updated": source.last_updated,
        "last_error": source.last_error,
        "unread_count": source.unread_count,
        "health": source.health.id(),
        "latest_article_at": source.latest_article_at,
        "disabled_reason": source.disabled_reason,
    })
}

/// 文章 JSON —— 与桌面端 `news_article_json` 一致。
fn article_json(article: &devtoolbox_application::news::NewsArticle) -> serde_json::Value {
    serde_json::json!({
        "id": article.id,
        "source_id": article.source_id,
        "source": article.source_name,
        "title": article.title,
        "url": article.url,
        "author": article.author,
        "image_url": article.image_url,
        "published_at": article.published_at,
        "summary": article.summary,
        "is_read": article.is_read,
        "starred": article.starred,
    })
}
