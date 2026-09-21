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
