//! 结构化国内数据 Provider（需求 #十二）：高德 POI + 和风天气。
//!
//! - `AmapPoiProvider`：高德「地点搜索」API（restapi.amap.com/v3/place/text），按城市查
//!   景点 / 美食 / 住宿 POI，产出 `FactCategory::Attraction / Food / Accommodation` 事实；
//! - `QWeatherProvider`：和风天气（geoapi 城市定位 + devapi 3 天预报），产出
//!   `FactCategory::Weather` 事实；
//! - **Key 全部可选**：未配置时 `providers_for` 返回空列表，Travel 核心流程照常运行
//!   （这就是「Key 未配置时的增强路径」：有 Key 多出 POI/天气事实，没有 Key 也不报错）。

use async_trait::async_trait;

use devtoolbox_core::travel::{
    AMAP_SOURCE_URL, FactCategory, MapCoordinates, OPEN_METEO_SOURCE_URL, ProviderError,
    QWEATHER_SOURCE_URL, TravelDataProvider, TravelDataRequest, TravelFact, TravelRoute,
    TravelRouteRequest, normalize_city_query,
};

const DATA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const USER_AGENT: &str = concat!("DevToolbox/", env!("CARGO_PKG_VERSION"), " (+travel data)");

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
    let mut qweather_configured = false;
    if let (Some(key), Some(api_host)) = (
        qweather_key.filter(|key| !key.trim().is_empty()),
        qweather_api_host.filter(|host| !host.trim().is_empty()),
    ) {
        providers.push(Box::new(QWeatherProvider::new(
            client.clone(),
            key,
            api_host,
        )));
        qweather_configured = true;
    }
    // 未配置和风天气 Key 时用 Open-Meteo 兜底：免费、无需 Key。
    // 否则「天气获取」这一步永远为空，页面只剩一句「未配置数据源 Key」。
    if !qweather_configured {
        providers.push(Box::new(OpenMeteoWeatherProvider::new(client.clone())));
    }
    // OSM 系（Nominatim 补坐标 + OSRM 算驾车时长）同样不需要 Key，
    // 让没有高德 Key 时行程仍能排出顺序与交通时间。
    providers.push(Box::new(NominatimGeocoder::new(client.clone())));
    providers.push(Box::new(OsrmRouteProvider::new(client.clone())));
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
) -> Result<serde_json::Value, ProviderError> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(DATA_TIMEOUT)
        .send()
        .await
        // reqwest errors may contain the full URL; AMap keys are query params.
        // Return a neutral message rather than leaking the credential to UI/errors.
        .map_err(|_| ProviderError::data(format!("{provider}: request failed")))?;
    if !response.status().is_success() {
        return Err(ProviderError::data(format!(
            "{provider}: server returned {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ProviderError::data(format!("{provider}: response body unavailable")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| ProviderError::data(format!("{provider}: bad json: {error}")))
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

    async fn fetch(&self, request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError> {
        if request.kind != "poi" {
            return Ok(Vec::new());
        }
        let mut facts = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let city = normalize_city_query(&request.city);
        for (keywords, category) in AMAP_QUERIES {
            // 景点需要作为用户可挑选的城市目录，保留更多候选；餐饮和住宿仍控制请求量。
            let offset = if *category == FactCategory::Attraction {
                20
            } else {
                8
            };
            let url = format!(
                "https://restapi.amap.com/v3/place/text?key={key}&keywords={keywords}&city={city}&citylimit=true&offset={offset}&page=1&extensions=base",
                key = percent_encode(&self.key),
                keywords = percent_encode(keywords),
                city = percent_encode(&city),
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
            return Err(ProviderError::data(failures.join("；")));
        }
        Ok(facts)
    }

    async fn driving_route(
        &self,
        request: TravelRouteRequest,
    ) -> Result<Option<TravelRoute>, ProviderError> {
        let url = format!(
            "https://restapi.amap.com/v3/direction/driving?origin={origin_lon},{origin_lat}&destination={destination_lon},{destination_lat}&extensions=base&key={key}",
            origin_lon = request.origin.longitude,
            origin_lat = request.origin.latitude,
            destination_lon = request.destination.longitude,
            destination_lat = request.destination.latitude,
            key = percent_encode(&self.key),
        );
        let body = get_json(&self.client, &url, "amap driving").await?;
        parse_amap_driving_route(&body)
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
) -> Result<Vec<TravelFact>, ProviderError> {
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
        return Err(ProviderError::data(format!(
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
        let coordinates = poi
            .get("location")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_amap_coordinates);
        let poi_id = poi
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
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
            coordinates,
            poi_id,
            area: (!district.is_empty()).then(|| district.to_string()),
            address: (!address.is_empty()).then(|| address.to_string()),
        });
    }
    Ok(facts)
}

fn parse_amap_coordinates(value: &str) -> Option<MapCoordinates> {
    let (longitude, latitude) = value.split_once(',')?;
    Some(MapCoordinates {
        longitude: longitude.trim().parse().ok()?,
        latitude: latitude.trim().parse().ok()?,
    })
}

/// 解析高德驾车路径第一条方案（离线可单测）。
pub fn parse_amap_driving_route(
    body: &serde_json::Value,
) -> Result<Option<TravelRoute>, ProviderError> {
    let status = body
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if status != "1" {
        let info = body
            .get("info")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        return Err(ProviderError::data(format!(
            "amap driving: status={status} info={info}"
        )));
    }
    let Some(path) = body
        .get("route")
        .and_then(|route| route.get("paths"))
        .and_then(serde_json::Value::as_array)
        .and_then(|paths| paths.first())
    else {
        return Ok(None);
    };
    let distance_m = path
        .get("distance")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<f32>().ok())
        .or_else(|| {
            path.get("distance")
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as f32)
        });
    let duration_s = path
        .get("duration")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| {
            path.get("duration")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32)
        });
    match (distance_m, duration_s) {
        (Some(distance_m), Some(duration_s)) if distance_m >= 0.0 => Ok(Some(TravelRoute {
            distance_km: distance_m / 1000.0,
            duration_minutes: ((duration_s as f32) / 60.0).ceil() as u32,
        })),
        _ => Ok(None),
    }
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

    async fn fetch(&self, request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError> {
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
            ProviderError::data(format!("qweather: city not found for {}", request.city))
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
) -> Result<Vec<TravelFact>, ProviderError> {
    if body.get("code").and_then(serde_json::Value::as_str) != Some("200") {
        return Err(ProviderError::data(
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
        coordinates: None,
        poi_id: None,
        area: None,
        address: None,
    }])
}

// ---------- Open-Meteo（无 Key 天气） ----------

/// Open-Meteo 天气数据源：地理编码 + 3 天预报，全程无需 API Key。
pub struct OpenMeteoWeatherProvider {
    client: reqwest::Client,
}

impl OpenMeteoWeatherProvider {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TravelDataProvider for OpenMeteoWeatherProvider {
    fn name(&self) -> &'static str {
        "open-meteo"
    }

    async fn fetch(&self, request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError> {
        if request.kind != "weather" {
            return Ok(Vec::new());
        }
        let city = normalize_city_query(&request.city);
        let point = open_meteo_geocode(&self.client, &city)
            .await?
            .ok_or_else(|| ProviderError::data(format!("open-meteo: city not found for {city}")))?;
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=Asia%2FShanghai&forecast_days=3",
            latitude = point.latitude,
            longitude = point.longitude,
        );
        let body = get_json(&self.client, &url, "open-meteo").await?;
        Ok(parse_open_meteo_daily(&body, &city, now_unix()))
    }
}

async fn open_meteo_geocode(
    client: &reqwest::Client,
    city: &str,
) -> Result<Option<MapCoordinates>, ProviderError> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={name}&count=1&language=zh&format=json",
        name = percent_encode(city),
    );
    let body = get_json(client, &url, "open-meteo").await?;
    Ok(parse_open_meteo_location(&body))
}

/// 取地理编码结果里的第一个精确点。
#[must_use]
pub fn parse_open_meteo_location(body: &serde_json::Value) -> Option<MapCoordinates> {
    let first = body.get("results")?.as_array()?.first()?;
    Some(MapCoordinates {
        latitude: first.get("latitude")?.as_f64()?,
        longitude: first.get("longitude")?.as_f64()?,
    })
}

/// WMO 天气代码 → 中文描述（Open-Meteo 返回的是 WMO code，不是文字）。
#[must_use]
pub fn weather_text_for_code(code: i64) -> &'static str {
    match code {
        0 => "晴",
        1 => "晴间多云",
        2 => "多云",
        3 => "阴",
        45 | 48 => "雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 => "冻雨",
        61 => "小雨",
        63 => "中雨",
        65 => "大雨",
        66 | 67 => "冻雨",
        71 => "小雪",
        73 => "中雪",
        75 => "大雪",
        77 => "米雪",
        80 | 81 => "阵雨",
        82 => "强阵雨",
        85 | 86 => "阵雪",
        95 => "雷阵雨",
        96 | 99 => "雷雨伴冰雹",
        _ => "未知",
    }
}

/// 解析 Open-Meteo 日预报：合并成一条 Weather 事实（与和风同格式）。
#[must_use]
pub fn parse_open_meteo_daily(
    body: &serde_json::Value,
    city: &str,
    fetched_at: i64,
) -> Vec<TravelFact> {
    let daily = body.get("daily");
    let times = daily
        .and_then(|value| value.get("time"))
        .and_then(serde_json::Value::as_array);
    let codes = daily
        .and_then(|value| value.get("weather_code"))
        .and_then(serde_json::Value::as_array);
    let maxima = daily
        .and_then(|value| value.get("temperature_2m_max"))
        .and_then(serde_json::Value::as_array);
    let minima = daily
        .and_then(|value| value.get("temperature_2m_min"))
        .and_then(serde_json::Value::as_array);
    let (Some(times), Some(codes), Some(maxima), Some(minima)) = (times, codes, maxima, minima)
    else {
        return Vec::new();
    };

    let mut parts: Vec<String> = Vec::new();
    for (index, date) in times.iter().take(3).enumerate() {
        let Some(date) = date.as_str() else { continue };
        let code = codes
            .get(index)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        let max = maxima.get(index).and_then(serde_json::Value::as_f64);
        let min = minima.get(index).and_then(serde_json::Value::as_f64);
        let (Some(max), Some(min)) = (max, min) else {
            continue;
        };
        parts.push(format!(
            "{date} {text} {min}~{max}°C",
            text = weather_text_for_code(code),
            min = min.round() as i64,
            max = max.round() as i64,
        ));
    }
    if parts.is_empty() {
        return Vec::new();
    }
    vec![TravelFact {
        category: FactCategory::Weather,
        subject: format!("{city}天气"),
        value: parts.join("；"),
        source_id: OPEN_METEO_SOURCE_URL.to_string(),
        confidence: 0.75,
        fetched_at,
        coordinates: None,
        poi_id: None,
        area: None,
        address: None,
    }]
}

// ---------- OSM：Nominatim 坐标 + OSRM 驾车路线（均无需 Key） ----------

/// Nominatim 地理编码：给景点补坐标，用于行程聚类与驾车时长。
pub struct NominatimGeocoder {
    client: reqwest::Client,
}

impl NominatimGeocoder {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TravelDataProvider for NominatimGeocoder {
    fn name(&self) -> &'static str {
        "nominatim"
    }

    async fn fetch(&self, _request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError> {
        Ok(Vec::new())
    }

    async fn geocode(
        &self,
        city: &str,
        name: &str,
    ) -> Result<Option<MapCoordinates>, ProviderError> {
        let query = format!("{name} {city}");
        let url = format!(
            "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=1&accept-language=zh-CN&q={query}",
            query = percent_encode(&query),
        );
        let body = get_json(&self.client, &url, "nominatim").await?;
        Ok(parse_nominatim_first(&body))
    }
}

/// Nominatim 返回的 `lat` / `lon` 是字符串。
#[must_use]
pub fn parse_nominatim_first(body: &serde_json::Value) -> Option<MapCoordinates> {
    let first = body.as_array()?.first()?;
    Some(MapCoordinates {
        latitude: first.get("lat")?.as_str()?.parse().ok()?,
        longitude: first.get("lon")?.as_str()?.parse().ok()?,
    })
}

/// OSRM 公共路由服务：提供驾车距离与时长（无需 Key）。
pub struct OsrmRouteProvider {
    client: reqwest::Client,
}

impl OsrmRouteProvider {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TravelDataProvider for OsrmRouteProvider {
    fn name(&self) -> &'static str {
        "osrm"
    }

    async fn fetch(&self, _request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError> {
        Ok(Vec::new())
    }

    async fn driving_route(
        &self,
        request: TravelRouteRequest,
    ) -> Result<Option<TravelRoute>, ProviderError> {
        let url = format!(
            "https://router.project-osrm.org/route/v1/driving/{origin_lon},{origin_lat};{target_lon},{target_lat}?overview=false&alternatives=false",
            origin_lon = request.origin.longitude,
            origin_lat = request.origin.latitude,
            target_lon = request.destination.longitude,
            target_lat = request.destination.latitude,
        );
        let body = get_json(&self.client, &url, "osrm").await?;
        Ok(parse_osrm_route(&body))
    }
}

/// OSRM `routes[0].duration` 是秒、`distance` 是米。
#[must_use]
pub fn parse_osrm_route(body: &serde_json::Value) -> Option<TravelRoute> {
    let route = body.get("routes")?.as_array()?.first()?;
    let duration = route.get("duration")?.as_f64()?;
    let distance = route.get("distance")?.as_f64()?;
    Some(TravelRoute {
        distance_km: (distance / 1000.0) as f32,
        duration_minutes: (duration / 60.0).round() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AMAP_SOURCE_URL, OPEN_METEO_SOURCE_URL, QWEATHER_SOURCE_URL, parse_amap_driving_route,
        parse_amap_pois, parse_nominatim_first, parse_open_meteo_daily, parse_open_meteo_location,
        parse_osrm_route, parse_qweather_daily, parse_qweather_location, providers_for,
        weather_text_for_code,
    };
    use devtoolbox_core::travel::FactCategory;

    #[test]
    fn providers_for_uses_keyless_sources_when_keys_missing() {
        let client = reqwest::Client::new();
        // 没有高德/和风 Key 时：天气由 Open-Meteo 兜底，坐标/路线由 OSM 提供。
        let providers = providers_for(None, None, None, None, &client);
        // open-meteo（天气）+ nominatim（坐标）+ osrm（路线）
        assert_eq!(providers.len(), 3);
        assert!(providers.iter().any(|p| p.name() == "open-meteo"));
        assert!(providers.iter().any(|p| p.name() == "nominatim"));
        assert!(providers.iter().any(|p| p.name() == "osrm"));
        assert!(!providers.iter().any(|p| p.name() == "qweather"));

        let providers = providers_for(
            Some("amap-key".to_string()),
            Some("qweather-key".to_string()),
            Some("example.qweatherapi.com".to_string()),
            None,
            &client,
        );
        // amap + qweather + nominatim + osrm（有高德/和风时不再叠加 open-meteo）
        assert_eq!(providers.len(), 4);
        assert!(!providers.iter().any(|p| p.name() == "open-meteo"));
        assert!(providers.iter().any(|p| p.name() == "amap-poi"));
        assert!(providers.iter().any(|p| p.name() == "qweather"));
        // 和风已配置时不再叠加 Open-Meteo（同主体会产生冲突版本）。
        assert!(!providers.iter().any(|p| p.name() == "open-meteo"));
    }

    #[test]
    fn open_meteo_daily_becomes_a_weather_fact() {
        let body = serde_json::json!({
            "daily": {
                "time": ["2026-11-19", "2026-11-20", "2026-11-21", "2026-11-22"],
                "weather_code": [0, 61, 3, 80],
                "temperature_2m_max": [21.4, 18.6, 17.2, 19.0],
                "temperature_2m_min": [11.6, 10.2, 9.8, 10.5]
            }
        });
        let facts = parse_open_meteo_daily(&body, "杭州", 1_700_000_000);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, FactCategory::Weather);
        assert_eq!(facts[0].subject, "杭州天气");
        assert_eq!(
            facts[0].value,
            "2026-11-19 晴 12~21°C；2026-11-20 小雨 10~19°C；2026-11-21 阴 10~17°C"
        );
        assert_eq!(facts[0].source_id, OPEN_METEO_SOURCE_URL);
    }

    #[test]
    fn open_meteo_location_and_wmo_text() {
        let body = serde_json::json!({
            "results": [{"name": "杭州", "latitude": 30.29365, "longitude": 120.16142}]
        });
        let point = parse_open_meteo_location(&body).expect("coords");
        assert!((point.latitude - 30.29365).abs() < 1e-6);
        assert_eq!(weather_text_for_code(0), "晴");
        assert_eq!(weather_text_for_code(95), "雷阵雨");
        assert_eq!(weather_text_for_code(42), "未知");
        assert!(parse_open_meteo_location(&serde_json::json!({"results": []})).is_none());
    }

    #[test]
    fn nominatim_and_osrm_parse() {
        let nominatim = serde_json::json!([
            {"lat": "30.2489", "lon": "120.1494", "display_name": "西湖, 杭州"}
        ]);
        let point = parse_nominatim_first(&nominatim).expect("coords");
        assert!((point.latitude - 30.2489).abs() < 1e-6);
        assert!((point.longitude - 120.1494).abs() < 1e-6);

        let osrm = serde_json::json!({
            "code": "Ok",
            "routes": [{"distance": 8400.0, "duration": 1140.0}]
        });
        let route = parse_osrm_route(&osrm).expect("route");
        assert!((route.distance_km - 8.4).abs() < 1e-3);
        assert_eq!(route.duration_minutes, 19);
        assert!(parse_osrm_route(&serde_json::json!({"code": "NoRoute"})).is_none());
    }

    #[test]
    fn amap_pois_parse_into_facts_with_source_url() {
        let body = serde_json::json!({
            "status": "1",
            "pois": [
                {"name": "西湖风景名胜区", "adname": "西湖区", "address": "龙井路1号", "location": "120.129,30.236"},
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
        assert_eq!(
            facts[0].coordinates.as_ref().map(|point| point.longitude),
            Some(120.129)
        );
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
    fn amap_driving_route_parses_distance_and_duration() {
        let body = serde_json::json!({
            "status": "1",
            "route": {"paths": [{"distance": "12500", "duration": "1860"}]}
        });
        let route = parse_amap_driving_route(&body)
            .expect("parse route")
            .expect("first path");
        assert!((route.distance_km - 12.5).abs() < f32::EPSILON);
        assert_eq!(route.duration_minutes, 31);
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
