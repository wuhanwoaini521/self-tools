//! Travel 领域的基础模型与纯数据结构。
//!
//! 本模块只定义数据与枚举，不做 I/O、不依赖网络 / 存储 / UI。
//! 所有结构体均为 serde 类型，便于跨层(DTO / 缓存 / Tauri 序列化)传输。

use serde::{Deserialize, Serialize};

/// 网页内容的获取状态：全文 / 仅搜索摘要 / 不可用。
/// 需求：#四 支持 Full Content、Snippet Only、Unavailable 三种 Source 状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentState {
    /// 成功抓到完整正文
    Full,
    /// 网页抓取失败，仅保留搜索引擎摘要（低可信度信息源）
    SnippetOnly,
    /// 完全无法获得内容（既不展示也不参与事实）
    Unavailable,
}

impl ContentState {
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Full | Self::SnippetOnly)
    }
}

/// 一次搜索结果（来自任意 SearchProvider，统一归一化）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    /// 来源 Provider 名称（如 "bing" / "baidu" / "searxng"），仅作展示与审计。
    pub provider: String,
    /// 页面发布日期（Unix 秒）；搜索引擎一般不提供，通常为 `None`。
    pub published_at: Option<i64>,
    /// 抓取时间（Unix 秒）。
    pub fetched_at: i64,
}

/// 归一化后的旅行文档（网页抓取结果）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelDocument {
    pub url: String,
    pub title: String,
    pub state: ContentState,
    /// 完整正文（`state == Full` 时）。
    pub content: Option<String>,
    /// 搜索摘要（`state == SnippetOnly` 时代替正文；`Full` 时也可能保留便于参考）。
    pub snippet: Option<String>,
    pub published_at: Option<i64>,
    pub fetched_at: i64,
    /// 抓到正文的 Provider / 抓取器名称。
    pub provider: Option<String>,
}

impl TravelDocument {
    /// 参与事实提取 / 展示的文本：优先正文，回退摘要。
    #[must_use]
    pub fn readable_text(&self) -> String {
        self.content
            .clone()
            .or_else(|| self.snippet.clone())
            .unwrap_or_default()
    }
}

/// 事实类别。需求：#九 `FactCategory`。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactCategory {
    /// 城市概览
    CityOverview,
    /// 景点
    Attraction,
    /// 美食
    Food,
    /// 交通
    Transport,
    /// 住宿
    Accommodation,
    /// 天气
    Weather,
    /// 开放时间
    OpeningHours,
    /// 门票
    Ticket,
    /// 预约政策
    Reservation,
    /// 活动
    Activity,
    /// 旅行贴士
    TravelTip,
    /// 注意事项 / 警告
    Warning,
}

impl FactCategory {
    /// 解析 LLM/设置返回的类别字符串；未知类别返回 `None`（解析时跳过该项）。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        Some(match normalized.as_str() {
            "city_overview" | "cityoverview" | "城市概览" | "城市介绍" => {
                Self::CityOverview
            }
            "attraction" | "景点" => Self::Attraction,
            "food" | "美食" => Self::Food,
            "transport" | "交通" => Self::Transport,
            "accommodation" | "住宿" => Self::Accommodation,
            "weather" | "天气" => Self::Weather,
            "opening_hours" | "openinghours" | "开放时间" => Self::OpeningHours,
            "ticket" | "门票" | "票价" => Self::Ticket,
            "reservation" | "预约" => Self::Reservation,
            "activity" | "活动" => Self::Activity,
            "travel_tip" | "traveltip" | "贴士" | "tips" => Self::TravelTip,
            "warning" | "注意事项" | "避坑" => Self::Warning,
            _ => return None,
        })
    }
}

/// 一条提取到的事实（需求 #九 建议模型）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TravelFact {
    pub category: FactCategory,
    pub subject: String,
    pub value: String,
    /// 来源标识（网页 URL 或搜索摘要标识）。
    pub source_id: String,
    /// 提取置信度 0.0 ~ 1.0（LLM 自评或启发式）。
    pub confidence: f32,
    /// 事实归属的抓取时间（Unix 秒）。
    pub fetched_at: i64,
}

/// 来源可信度等级（需求 #七）。S > A > B > C。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SourceLevel {
    /// 官方信息：政府 / 文旅局 / 景区官网 / 博物馆官网 / 机场地铁官网等
    S,
    /// 大型稳定信息源：百度百科、地图 POI、大型旅游平台官方页
    A,
    /// 旅游内容平台：携程 / 去哪儿 / 马蜂窝 / 同程 / 飞猪
    B,
    /// UGC / 社区 / 个人博客 / 游记 / 未知来源
    C,
}

impl SourceLevel {
    /// 权威权重（需求 #七：S 0.95~1.0、A 0.80~0.90、B 0.65~0.80、C 0.40~0.65）。
    #[must_use]
    pub const fn weight(self) -> f32 {
        match self {
            Self::S => 1.0,
            Self::A => 0.85,
            Self::B => 0.72,
            Self::C => 0.5,
        }
    }

    /// 展示用星级（1 ~ 5）。
    #[must_use]
    pub const fn stars(self) -> u8 {
        match self {
            Self::S => 5,
            Self::A => 4,
            Self::B => 3,
            Self::C => 2,
        }
    }

    /// 主来源类型标签（用于「primary_source」展示，如 official / travel_platform / ugc）。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::S => "official",
            Self::A => "authoritative",
            Self::B => "travel_platform",
            Self::C => "ugc",
        }
    }
}

impl std::fmt::Display for SourceLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let letter = match self {
            Self::S => "S",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        };
        formatter.write_str(letter)
    }
}

impl std::fmt::Display for ContentState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Full => "full",
            Self::SnippetOnly => "snippet",
            Self::Unavailable => "unavailable",
        };
        formatter.write_str(label)
    }
}

/// 已评级的信息来源（Guide 的 Sources 列表项）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TravelSource {
    pub url: String,
    pub title: String,
    pub host: String,
    pub level: SourceLevel,
    pub state: ContentState,
    pub published_at: Option<i64>,
    pub fetched_at: i64,
    /// 综合评分 authority × freshness × relevance。
    pub score: f32,
}

// ---------- Research 进度事件 ----------

/// 研究阶段（Research Progress 的步骤）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchPhase {
    /// 识别城市
    IdentifyCity,
    /// 规划搜索任务
    PlanQueries,
    /// 搜索
    Search,
    /// 抓取网页
    FetchDocuments,
    /// 提取事实
    ExtractFacts,
    /// 结构化数据源（高德 POI / 和风天气等；未配置 Key 时跳过）
    DataSources,
    /// 来源排序
    RankSources,
    /// 事实验证
    ValidateFacts,
    /// 生成攻略
    GenerateGuide,
    /// 保存攻略
    SaveGuide,
}

/// 步骤状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    Done,
    Failed,
    Skipped,
}

/// 研究进度事件（需求 #十六）。纯数据：未来可直接转成 Tauri Event 推送给前端。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelResearchEvent {
    pub phase: ResearchPhase,
    pub status: StepStatus,
    /// 人类可读消息（如「搜索景点：杭州 必去景点」）。
    pub message: String,
    /// 事件序号，前端按序展示。
    pub seq: u64,
}
