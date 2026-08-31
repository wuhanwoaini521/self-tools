//! `CityGuide` —— 最终呈现给用户的结构化攻略（需求 #十三）。
//!
//! 字段与前端 `CityGuide` 接口一一对应（serde snake_case）。
//! 所有集合字段均可缺失/为空，缺失时前端显示「暂无可靠数据」，绝不编造。

use serde::{Deserialize, Serialize};

use crate::travel::model::{FactCategory, MapCoordinates, TravelSource};
use crate::travel::query_planner::TravelDateRange;

/// 顶层攻略。`serde(default)` 保证 LLM 输出缺字段时也能解析。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct CityGuide {
    pub city: CityInfo,
    pub summary: String,
    pub highlights: Vec<String>,
    pub best_time: Option<String>,
    /// 来自天气数据源的近期逐日预报；不由 LLM 编造。
    pub weather: Option<WeatherForecast>,
    pub districts: Vec<DistrictInfo>,
    pub attractions: Vec<Attraction>,
    pub foods: Vec<Food>,
    pub restaurants: Vec<Place>,
    pub transport: TransportGuide,
    pub accommodation_areas: Vec<AccommodationArea>,
    pub itineraries: Itineraries,
    pub local_tips: Vec<TravelTip>,
    pub warnings: Vec<TravelWarning>,
    pub sources: Vec<TravelSource>,
    pub meta: GuideMeta,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct CityInfo {
    /// 用户输入的城市名（中文），必填。
    pub name: String,
    /// 英文名（如 "Hangzhou"）；无数据时为 `None`。
    pub name_en: Option<String>,
    /// 省级（如 "浙江"）。
    pub province: Option<String>,
    /// 国家（如 "中国"）。
    pub country: Option<String>,
}

/// 近期天气预报（当前数据源为和风天气）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct WeatherForecast {
    pub city: String,
    pub days: Vec<WeatherDay>,
}

/// 单日预报。温度保留 API 原始文本，避免将未知值伪装成数字。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct WeatherDay {
    pub date: String,
    pub text_day: String,
    pub temp_min: String,
    pub temp_max: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DistrictInfo {
    pub name: String,
    /// 该区域的亮点 / 说明。
    pub note: Option<String>,
    /// 主要景点名列表。
    pub landmarks: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Attraction {
    pub name: String,
    /// 一句话介绍。
    pub intro: Option<String>,
    /// 所在区域（如 "西湖区"）。
    pub area: Option<String>,
    /// 建议游玩时长（如 "半天"）。
    pub suggested_duration: Option<String>,
    /// 开放时间（经过多源验证的字段，见 `VerifiedValue`）。
    pub opening_hours: Option<VerifiedValue>,
    /// 门票信息（多源验证字段）。
    pub ticket: Option<VerifiedValue>,
    /// 预约政策（多源验证字段）。
    pub reservation: Option<VerifiedValue>,
    /// 本地游览贴士。
    pub tips: Vec<String>,
    /// 支撑该条目的事实来源 URL。
    pub source_ids: Vec<String>,
    /// 仅来自地图 POI 的坐标，用于攻略地图展示。
    pub coordinates: Option<MapCoordinates>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Food {
    pub name: String,
    /// 菜系 / 类别（如 "杭帮菜"）。
    pub dish_type: Option<String>,
    /// 介绍 / 推荐理由。
    pub intro: Option<String>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Place {
    pub name: String,
    pub area: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct TransportGuide {
    /// 交通总览。
    pub overview: Option<String>,
    /// 机场抵达方式。
    pub airport: Option<String>,
    /// 火车站 / 高铁抵达方式。
    pub train_station: Option<String>,
    /// 市内地铁。
    pub metro: Option<String>,
    /// 公交 / 打车贴士。
    pub bus_taxi: Option<String>,
    pub tips: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct AccommodationArea {
    pub name: String,
    pub area: Option<String>,
    /// 适合人群 / 理由。
    pub note: Option<String>,
    /// 大致价位档（如 "中高端"）。
    pub budget: Option<String>,
}

/// 多日路线（需求 #十三：oneDay / twoDays / threeDays）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Itineraries {
    pub one_day: Option<Itinerary>,
    pub two_days: Option<Itinerary>,
    pub three_days: Option<Itinerary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Itinerary {
    /// 天数（1 ~ 3）。
    pub day: u8,
    /// 主题（如 "经典西湖一日游"）。
    pub title: Option<String>,
    pub stops: Vec<ItineraryStop>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItineraryStop {
    pub name: String,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelTip {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelWarning {
    pub title: String,
    pub text: String,
}

/// 多源验证后的「关键事实值」（需求 #十）。
/// `has_conflict = true` 时前端应显示「⚠ 信息存在多个版本」。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct VerifiedValue {
    pub value: String,
    /// "high" | "medium" | "low"
    pub confidence: String,
    /// 支撑该取值的一致来源数量。
    pub verified_sources: usize,
    /// 主来源类型（official / authoritative / travel_platform / ugc / unknown）。
    pub primary_source: String,
    /// 是否存在冲突版本。
    pub has_conflict: bool,
}

/// 攻略元信息（生成 / 更新时间由存储层维护）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuideMeta {
    /// 首次生成时间（Unix 秒）。
    pub generated_at: i64,
    /// 最近更新时间（Unix 秒）。
    pub updated_at: i64,
    /// 行程天数（用户输入）。
    pub days: u8,
    /// 用户选择的行程日期范围；缺省时表示仅按天数研究。
    pub date_range: Option<TravelDateRange>,
    /// 是否使用了 LLM（未配置 / 失败时为 false，攻略为降级版）。
    pub llm_used: bool,
    /// 研究过程提示（如「天气信息获取失败」「某来源仅摘要」）。
    pub notes: Vec<String>,
}

/// 供前端「最近攻略」列表使用的最小元信息。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuideSummary {
    pub city: String,
    pub days: u8,
    pub updated_at: i64,
}

/// 已验证事实（`verify_facts` 的输出），供 Guide 组装时查询。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerifiedFact {
    pub category: FactCategory,
    pub subject: String,
    pub value: String,
    pub confidence: String,
    pub verified_sources: usize,
    pub primary_source: String,
    pub has_conflict: bool,
    /// 冲突时存在的其他版本（供 UI 展示「多个版本」）。
    pub candidates: Vec<String>,
}
