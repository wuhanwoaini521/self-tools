//! 旅行研究的外部能力契约：搜索 / 抓取 / LLM / 结构化数据 Provider。
//!
//! Gate 8：这些接口原本定义在 `devtoolbox_infrastructure::travel`，而
//! `TravelResearchService`（application）为了编排它们必须反向依赖
//! infrastructure。现在契约上移到 core —— infrastructure 实现、
//! application 消费，两者都只依赖 core，消除反向依赖。
//!
//! `ProviderError` 只携带分类与文本（不携带任何基础设施类型）；其 Display
//! 文本与既有 `InfrastructureError::Travel*` 保持一致，用户可见文案不变。

use std::fmt;

use async_trait::async_trait;

use super::{MapCoordinates, SearchResult, TravelDocument, TravelFact};

/// 搜索选项。
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchOptions {
    /// 期望的结果数量上限（Provider 尽力而为）。
    pub count: usize,
}

/// 结构化数据请求。
#[derive(Clone, Debug)]
pub struct TravelDataRequest {
    pub city: String,
    /// 查询类别（"poi" / "weather"，V2 可扩展）。
    pub kind: &'static str,
}

/// 两个 POI 之间的驾车路线请求。
#[derive(Clone, Debug)]
pub struct TravelRouteRequest {
    pub origin: MapCoordinates,
    pub destination: MapCoordinates,
}

/// 路线服务返回的可读结果。距离来自道路路径，不是坐标直线距离。
#[derive(Clone, Debug, Default)]
pub struct TravelRoute {
    pub distance_km: f32,
    pub duration_minutes: u32,
}

/// 高德 POI 在 Sources 中的展示地址（也是事实 source_id，供权威权重匹配）。
pub const AMAP_SOURCE_URL: &str = "https://restapi.amap.com";
/// 和风天气在 Sources 中的展示地址。
pub const QWEATHER_SOURCE_URL: &str = "https://devapi.qweather.com";

/// Provider 失败类别（与 `TravelFailure` 的 `TravelErrorKind` 对齐）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderErrorKind {
    Search,
    Fetch,
    Llm,
    Data,
}

/// 统一 Provider 失败：分类 + 基础文本，不携带任何基础设施错误类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    /// 基础设施层失败文本（如 "server returned 503"）。
    pub message: String,
}

impl ProviderError {
    #[must_use]
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn search(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Search, message)
    }

    #[must_use]
    pub fn fetch(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Fetch, message)
    }

    #[must_use]
    pub fn llm(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Llm, message)
    }

    #[must_use]
    pub fn data(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Data, message)
    }
}

/// Display 前缀与既有 `InfrastructureError::Travel*` 文本完全一致，
/// 上层对消息的匹配（如 LLM 传输错误判定 `travel llm request failed …`）不改变。
impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.kind {
            ProviderErrorKind::Search => "travel search failed",
            ProviderErrorKind::Fetch => "travel page fetch failed",
            ProviderErrorKind::Llm => "travel llm request failed",
            ProviderErrorKind::Data => "travel data provider failed",
        };
        write!(f, "{prefix}: {}", self.message)
    }
}

impl std::error::Error for ProviderError {}

/// 统一搜索接口。`Box<dyn>` 使用，需 `Send + Sync`。
#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn search(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchResult>, ProviderError>;
}

/// 网页抓取接口（可 mock）。
#[async_trait]
pub trait WebFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<TravelDocument, ProviderError>;
}

/// LLM 调用接口（可 mock）。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 单轮对话，返回模型原始输出文本。
    async fn complete(&self, system: &str, user: &str) -> Result<String, ProviderError>;
}

/// 数据 Provider 接口（高德 / 和风等结构化数据源）。
#[async_trait]
pub trait TravelDataProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn fetch(&self, request: TravelDataRequest) -> Result<Vec<TravelFact>, ProviderError>;

    async fn driving_route(
        &self,
        _request: TravelRouteRequest,
    ) -> Result<Option<TravelRoute>, ProviderError> {
        Ok(None)
    }
}