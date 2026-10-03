//! Geography 的 HTTP 接口（网页端）。
//!
//! 此前网页端的 Geography 搜索走前端硬编码的 12 条示例数据
//! （`FALLBACK_ENTITIES`），**不查真实库**——于是搜「大连」「西雅图」这类
//! 真实存在的地点一律无结果。接入后两端共用 `GeographyStore`。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use devtoolbox_core::geography::GeoEntityType;

use crate::ai_api::ApiError;

/// 前端的实体类型 slug → core 枚举。未知值直接 400，不要静默忽略——
/// 否则「按类型筛选没生效」会表现成「搜不到」，很难排查。
fn parse_entity_type(slug: &str) -> Result<GeoEntityType, ApiError> {
    use GeoEntityType::{
        Archipelago, Basin, City, Country, Desert, Island, Lake, Mountain, MountainRange, Ocean,
        Plain, Plateau, Province, Region, River, Sea, World,
    };
    Ok(
        match slug.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "world" => World,
            "country" => Country,
            "region" => Region,
            "province" => Province,
            "city" => City,
            "river" => River,
            "mountain" => Mountain,
            "mountain_range" | "range" => MountainRange,
            "plateau" => Plateau,
            "plain" => Plain,
            "basin" => Basin,
            "desert" => Desert,
            "lake" => Lake,
            "ocean" => Ocean,
            "sea" => Sea,
            "island" => Island,
            "archipelago" => Archipelago,
            other => {
                return Err(ApiError {
                    code: "invalid",
                    message: format!("未知的地理实体类型：{other}"),
                });
            }
        },
    )
}

type Shared = Arc<devtoolbox_application::geography::GeographyService>;

fn ok<T: Serialize>(value: T) -> Result<axum::Json<serde_json::Value>, ApiError> {
    serde_json::to_value(value)
        .map(axum::Json)
        .map_err(|error| ApiError {
            code: "encode_failed",
            message: error.to_string(),
        })
}

fn backend(error: devtoolbox_application::ApplicationError) -> ApiError {
    ApiError {
        code: "geography_failed",
        message: error.to_string(),
    }
}

/// `GET /api/v1/geography/home?fresh=0`
#[axum::debug_handler]
pub async fn home(
    axum::extract::Query(params): axum::extract::Query<FreshQuery>,
    axum::Extension(service): axum::Extension<Shared>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    ok(service.home(params.cursor.unwrap_or(0)).map_err(backend)?)
}

/// 注意：GET 查询串按字面字段名解析。前端 `geographyClient.home` 传的是
/// `{ cursor }`，所以字段名必须叫 `cursor`。
#[derive(Debug, Deserialize)]
pub struct FreshQuery {
    pub cursor: Option<u64>,
}

/// `GET /api/v1/geography/search?q=…&limit=30`
pub async fn search(
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
    axum::Extension(service): axum::Extension<Shared>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let query = params.query.trim().to_string();
    if query.is_empty() {
        return Ok(axum::Json(serde_json::json!(
            Vec::<serde_json::Value>::new()
        )));
    }
    let entity_type = params
        .entity_type
        .as_deref()
        .map(parse_entity_type)
        .transpose()?;
    let hits = service
        .search(
            &query,
            entity_type,
            params.limit.unwrap_or(30).clamp(1, 200),
        )
        .map_err(backend)?;
    ok(hits)
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// 前端 transport 传的是 `query=`（与 Tauri 命令参数同名）。
    #[serde(alias = "q")]
    pub query: String,
    /// 可选的实体类型过滤：`country` / `province` / `city` / `river` / `range` / `terrain`。
    pub entity_type: Option<String>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/geography/entities/{id}`
pub async fn detail(
    axum::Extension(service): axum::Extension<Shared>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    match service.detail(&id).map_err(backend)? {
        Some(detail) => ok(detail),
        None => Err(ApiError {
            code: "not_found",
            message: format!("地理实体不存在：{id}"),
        }),
    }
}

/// `POST /api/v1/geography/favorite` —— 收藏 / 取消。
/// 与其它 POST 端点一致，实体 id 走请求体 `{ "id": ... }`。
pub async fn toggle_favorite(
    axum::Extension(service): axum::Extension<Shared>,
    axum::Json(body): axum::Json<FavoriteRequest>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let id = body.id.trim();
    if id.is_empty() {
        return Err(ApiError {
            code: "invalid",
            message: "缺少地理实体 id".to_string(),
        });
    }
    let favorite = service.toggle_favorite(id).map_err(backend)?;
    // 返回裸 bool：前端 `geographyClient.toggleFavorite` 的返回类型是 Promise<boolean>。
    ok(favorite)
}

#[derive(Debug, Deserialize)]
pub struct FavoriteRequest {
    pub id: String,
}
