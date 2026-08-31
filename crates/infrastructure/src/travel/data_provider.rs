//! 结构化国内数据 Provider（需求 #十二）：高德 POI + 和风天气。
//!
//! - `AmapPoiProvider`：高德「地点搜索」API（restapi.amap.com/v3/place/text），按城市查
//!   景点 / 美食 / 住宿 POI，产出 `FactCategory::Attraction / Food / Accommodation` 事实；
//! - `QWeatherProvider`：和风天气（geoapi 城市定位 + devapi 3 天预报），产出
//!   `FactCategory::Weather` 事实；
//! - **Key 全部可选**：未配置时 `providers_for` 返回空列表，Travel 核心流程照常运行
//!   （这就是「Key 未配置时的增强路径」：有 Key 多出 POI/天气事实，没有 Key 也不报错）。

use async_trait::async_trait;

use crate::error::InfrastructureError;
use devtoolbox_core::travel::{FactCategory, TravelFact};

const DATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const USER_AGENT: &str = concat!("DevToolbox/", env!("CARGO_PKG_VERSION"), " (+travel data)");

/// 高德 POI 在 Sources 中的展示地址（也是事实 source_id，供权威权重匹配）。
pub const AMAP_SOURCE_URL: &str = "https://restapi.amap.com";
/// 和风天气在 Sources 中的展示地址。
pub const QWEATHER_SOURCE_URL: &str = "https://devapi.qweather.com";

/// 结构化数据请求。
#[derive(Clone, Debug)]
pub struct TravelDataRequest {
    pub city: String,
    /// 查询类别（"poi" / "weather"，V2 可扩展）。
    pub kind: &'static str,
}

/// 数据 Provider 接口。
#[async_trait]
pub trait TravelDataProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn fetch(
        &self,
        request: TravelDataRequest,
    ) -> Result<Vec<TravelFact>, InfrastructureError>;
}

/// 依据设置装配数据 Provider。Key 未配置 → 不生成对应 Provider，核心流程不受影响。
#[must_use]
pub fn providers_for(
    amap_key: Option<String>,
    qweather_key: Option<String>,
    qweather_api_host: Option<String>,
    _baidu_map_key: Option<String>,
    client: &reqwest::Client,
) -> Vec<Box<dyn TravelDataProvider>> {
    let mut providers: Vec<Box<dyn TravelDataProvider>> = Vec::new();
    if let Some(key) = amap_key.filter(|k| !k.trim().is_empty()) {
        providers.push(Box::new(AmapPoiProvider::new(client.clone(), key)));
    }
    if let (Some(key), Some(api_host)) = (
        qweather_key.filter(|key| !key.trim().is_empty()),
        qweather_api_host.filter(|host| !host.trim().is_empty()),
    ) {
        providers.push(Box::new(QWeatherProvider::new(
            client.clone(),
            key,
            api_host,
        )));
    }
    // 百度地图开放平台（V2 TODO：baidu_map_key 接入「地点检索」）
    providers
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    provider: &str,
) -> Result<serde_json::Value, InfrastructureError> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(DATA_TIMEOUT)
        .send()
        .await
        .map_err(|error| InfrastructureError::TravelData(format!("{provider}: {error}")))?;
    if !response.status().is_success() {
        return Err(InfrastructureError::TravelData(format!(
            "{provider}: server returned {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| InfrastructureError::TravelData(format!("{provider}: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| InfrastructureError::TravelData(format!("{provider}: bad json: {error}")))
}

// ---------- 高德 POI ----------

/// 高德 POI 数据源（服务端 API，不需要浏览器）。
pub struct AmapPoiProvider {
    client: reqwest::Client,
    key: String,
}

impl AmapPoiProvider {
    #[must_use]
    pub fn new(client: reqwest::Client, key: String) -> Self {
        Self { client, key }
    }
}

/// 关键字组 → 事实类别（每组一个搜索请求，避免单次请求量过大被截断）。
const AMAP_QUERIES: &[(&str, FactCategory)] = &[
    ("旅游景点,风景名胜,公园,博物馆", FactCategory::Attraction),
    ("美食,餐厅,小吃", FactCategory::Food),
    ("酒店,民宿", FactCategory::Accommodation),
];

#[async_trait]
impl TravelDataProvider for AmapPoiProvider {
    fn name(&self) -> &'static str {
        "amap-poi"
    }

    async fn fetch(
        &self,
        request: TravelDataRequest,
    ) -> Result<Vec<TravelFact>, InfrastructureError> {
        if request.kind != "poi" {
            return Ok(Vec::new());
        }
        let mut facts = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for (keywords, category) in AMAP_QUERIES {
            let url = format!(
                "https://restapi.amap.com/v3/place/text?key={key}&keywords={keywords}&city={city}&offset=8&page=1&extensions=base",
                key = percent_encode(&self.key),
                keywords = percent_encode(keywords),
                city = percent_encode(&request.city),
            );
            let body = match get_json(&self.client, &url, "amap").await {
                Ok(body) => body,
                Err(error) => {
                    failures.push(error.to_string());
                    continue;
                }
            };
            match parse_amap_pois(&body, *category, now_unix()) {
                Ok(parsed) => facts.extend(parsed),
                Err(error) => failures.push(error.to_string()),
            }
        }
        if facts.is_empty() && !failures.is_empty() {
            return Err(InfrastructureError::TravelData(failures.join("；")));
        }
        Ok(facts)
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// 解析高德 place/text 响应（离线可单测）。
/// `status == "1"` 成功；`status == "0"` 视为业务错误（Key 无效等）。
pub fn parse_amap_pois(
    body: &serde_json::Value,
    category: FactCategory,
    fetched_at: i64,
) -> Result<Vec<TravelFact>, InfrastructureError> {
    let status = body
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if status != "1" {
        let infocode = body
            .get("infocode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let info = body
            .get("info")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        return Err(InfrastructureError::TravelData(format!(
            "amap: status={status} infocode={infocode} info={info}"
        )));
    }
    let Some(pois) = body.get("pois").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut facts = Vec::new();
    for poi in pois {
        let Some(name) = poi.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let address = poi
            .get("address")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let district = poi
            .get("adname")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let value = if address.is_empty() && district.is_empty() {
            "高德地图 POI".to_string()
        } else {
            vec![district, address]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        facts.push(TravelFact {
            category,
            subject: name.to_string(),
            value,
            source_id: AMAP_SOURCE_URL.to_string(),
            confidence: 0.8,
            fetched_at,
        });
    }
    Ok(facts)
}

// ---------- 和风天气 ----------

/// 和风天气数据源（城市定位 + 3 天预报）。
pub struct QWeatherProvider {
    client: reqwest::Client,
    key: String,
    api_host: String,
}

impl QWeatherProvider {
    #[must_use]
    pub fn new(client: reqwest::Client, key: String, api_host: String) -> Self {
        Self {
            client,
            key,
            api_host: normalize_api_host(&api_host),
        }
    }
}

#[async_trait]
impl TravelDataProvider for QWeatherProvider {
    fn name(&self) -> &'static str {
        "qweather"
    }

    async fn fetch(
        &self,
        request: TravelDataRequest,
    ) -> Result<Vec<TravelFact>, InfrastructureError> {
        if request.kind != "weather" {
            return Ok(Vec::new());
        }
        // 1) 城市定位（取第一个匹配）
        let lookup_url = format!(
            "{host}/geo/v2/city/lookup?location={location}&key={key}",
            host = self.api_host,
            location = percent_encode(&request.city),
            key = percent_encode(&self.key),
        );
        let lookup = get_json(&self.client, &lookup_url, "qweather").await?;
        let location_id = parse_qweather_location(&lookup).ok_or_else(|| {
            InfrastructureError::TravelData(format!(
                "qweather: city not found for {}",
                request.city
            ))
        })?;
        // 2) 3 天预报
        let forecast_url = format!(
            "{host}/v7/weather/3d?location={location}&key={key}",
            host = self.api_host,
            location = location_id,
            key = percent_encode(&self.key),
        );
        let forecast = get_json(&self.client, &forecast_url, "qweather").await?;
        parse_qweather_daily(&forecast, &request.city, now_unix())
    }
}

fn normalize_api_host(value: &str) -> String {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("https://") {
        value.to_string()
    } else {
        format!("https://{value}")
    }
}

/// 从城市定位响应中提取首个 location id。
#[must_use]
pub fn parse_qweather_location(body: &serde_json::Value) -> Option<String> {
    if body.get("code").and_then(serde_json::Value::as_str) != Some("200") {
        return None;
    }
    body.get("location")?
        .as_array()?
        .iter()
        .find_map(|item| item.get("id")?.as_str().map(str::to_string))
}

/// 解析 3 天预报：合并成一条 Weather 事实（避免按天拆分造成同主体冲突）。
pub fn parse_qweather_daily(
    body: &serde_json::Value,
    city: &str,
    fetched_at: i64,
) -> Result<Vec<TravelFact>, InfrastructureError> {
    if body.get("code").and_then(serde_json::Value::as_str) != Some("200") {
        return Err(InfrastructureError::TravelData(
            "qweather: daily code != 200".to_string(),
        ));
    }
    let Some(daily) = body.get("daily").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut parts: Vec<String> = Vec::new();
    for day in daily.iter().take(3) {
        let Some(fx_date) = day.get("fxDate").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let text_day = day
            .get("textDay")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未知");
        let temp_min = day
            .get("tempMin")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let temp_max = day
            .get("tempMax")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        parts.push(format!("{fx_date} {text_day} {temp_min}~{temp_max}°C"));
    }
    if parts.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![TravelFact {
        category: FactCategory::Weather,
        subject: format!("{city}天气"),
        value: parts.join("；"),
        source_id: QWEATHER_SOURCE_URL.to_string(),
        confidence: 0.9,
        fetched_at,
    }])
}

#[cfg(test)]
mod tests {
    use super::{
        AMAP_SOURCE_URL, QWEATHER_SOURCE_URL, parse_amap_pois, parse_qweather_daily,
        parse_qweather_location, providers_for,
    };
    use devtoolbox_core::travel::FactCategory;

    #[test]
    fn providers_for_skips_unconfigured_keys() {
        let client = reqwest::Client::new();
        assert!(providers_for(None, None, None, None, &client).is_empty());
        let providers = providers_for(
            Some("amap-key".to_string()),
            Some("qweather-key".to_string()),
            Some("example.qweatherapi.com".to_string()),
            None,
            &client,
        );
        assert_eq!(providers.len(), 2);
        assert!(providers.iter().any(|p| p.name() == "amap-poi"));
        assert!(providers.iter().any(|p| p.name() == "qweather"));
    }

    #[test]
    fn amap_pois_parse_into_facts_with_source_url() {
        let body = serde_json::json!({
            "status": "1",
            "pois": [
                {"name": "西湖风景名胜区", "adname": "西湖区", "address": "龙井路1号"},
                {"name": "楼外楼", "adname": "西湖区", "address": "孤山路30号"}
            ]
        });
        let facts =
            parse_amap_pois(&body, FactCategory::Attraction, 1_700_000_000).expect("parse amap");
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].subject, "西湖风景名胜区");
        assert!(facts[0].value.contains("西湖区") && facts[0].value.contains("龙井路1号"));
        assert_eq!(facts[0].source_id, AMAP_SOURCE_URL);
        assert_eq!(facts[0].confidence, 0.8);
    }

    #[test]
    fn amap_error_status_is_reported() {
        let body = serde_json::json!({"status": "0", "infocode": "10001", "info": "invalid key"});
        assert!(parse_amap_pois(&body, FactCategory::Attraction, 1).is_err());
    }

    #[test]
    fn amap_missing_pois_yields_empty() {
        let body = serde_json::json!({"status": "1"});
        let facts = parse_amap_pois(&body, FactCategory::Food, 1).expect("empty ok");
        assert!(facts.is_empty());
    }

    #[test]
    fn qweather_lookup_finds_first_location_id() {
        let body = serde_json::json!({
            "code": "200",
            "location": [
                {"name": "杭州", "id": "101210101", "adm1": "浙江省"},
                {"name": "杭州", "id": "101210102"}
            ]
        });
        assert_eq!(parse_qweather_location(&body).as_deref(), Some("101210101"));
        assert_eq!(
            parse_qweather_location(&serde_json::json!({"code": "401"})),
            None
        );
    }

    #[test]
    fn qweather_daily_parses_into_single_weather_fact() {
        let body = serde_json::json!({
            "code": "200",
            "daily": [
                {"fxDate": "2026-09-01", "textDay": "晴", "tempMin": "24", "tempMax": "33"},
                {"fxDate": "2026-09-02", "textDay": "多云", "tempMin": "25", "tempMax": "32"}
            ]
        });
        let facts = parse_qweather_daily(&body, "杭州", 1_700_000_000).expect("parse weather");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, FactCategory::Weather);
        assert_eq!(facts[0].subject, "杭州天气");
        assert!(facts[0].value.contains("2026-09-01 晴 24~33°C"));
        assert!(facts[0].value.contains("2026-09-02 多云 25~32°C"));
        assert_eq!(facts[0].source_id, QWEATHER_SOURCE_URL);
    }
}
