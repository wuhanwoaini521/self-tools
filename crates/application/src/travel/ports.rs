//! Travel 用例的存储端口（Gate 8）。
//!
//! 沿用 Gate 7.6 RSS 的模式：端口在 application，实现由组合根
//! （desktop `travel_providers.rs` 里的适配器）装配，application 不接触 SQLite。
//! 方法全部同步（短锁语义，不跨 await），错误以文本返回。

use devtoolbox_core::travel::{CityGuide, SearchResult, TravelDocument};

/// Travel 缓存存储（攻略 / 搜索结果 / 页面文档）。
pub trait TravelStorePort: Send + Sync {
    /// 读取攻略（未命中 / 过期返回 `None`）。
    fn get_guide(&self, city: &str, days: u8, now: i64) -> Result<Option<CityGuide>, String>;

    /// 写入 / 更新攻略，返回落库后的副本（`updated_at` 刷新）。
    fn upsert_guide(&self, guide: &CityGuide, now: i64) -> Result<CityGuide, String>;

    /// 读取搜索结果缓存（未命中 / 过期返回 `None`）。
    fn get_search_results(
        &self,
        query: &str,
        now: i64,
    ) -> Result<Option<Vec<SearchResult>>, String>;

    /// 写入搜索结果缓存。
    fn put_search_results(
        &self,
        query: &str,
        results: &[SearchResult],
        now: i64,
    ) -> Result<(), String>;

    /// 读取页面文档缓存（未命中 / 过期返回 `None`）。
    fn get_document(&self, url: &str, now: i64) -> Result<Option<TravelDocument>, String>;

    /// 写入页面文档缓存。
    fn put_document(&self, document: &TravelDocument, now: i64) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// V5 Personal AI 接入端口（Travel 模块消费；desktop 组合根实现）
// ---------------------------------------------------------------------------

/// 精简目的地搜索结果（工具返回给 AI 的数据，不携带巨型文档）。
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TravelSearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub domain: String,
}

/// 已缓存行程的紧凑摘要（工具/上下文提供方使用；不触发生成与写入）。
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TripContext {
    pub city: String,
    pub days: u8,
    pub summary: Option<String>,
    pub highlights: Vec<String>,
    pub sources_count: usize,
    pub from_cache: bool,
}

/// Travel AI 接入端口：只读/查询面（搜索走既有 Provider 生态；行程读缓存）。
///
/// V5 §43：规划结果默认 preview，不自动写永久数据 —— 端口只提供读取方法。
#[async_trait::async_trait]
pub trait TravelAiPort: Send + Sync {
    fn configured(&self) -> bool;

    /// 目的地搜索（复用组合根装配的 SearchProvider）。
    async fn search_destination(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TravelSearchHit>, String>;

    /// 读取已缓存城市归档（不触发生成/不写数据）。
    fn trip_context(&self, city: &str) -> Result<Option<TripContext>, String>;

    /// 规划预览：优先缓存速览；无缓存时返回确定性骨架（不自动写永久数据）。
    fn plan_preview(&self, city: &str, days: u8) -> Result<Option<TripContext>, String>;
}
