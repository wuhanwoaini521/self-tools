//! Travel 存储适配器（桌面与网页共用）。
//!
//! 原先定义在桌面组合根里，server 取不到 —— Travel 的 provider 装配因此无法复用，
//! 网页端整页 Travel 不可用。下沉后两端读写同一份 travel.db。

use std::sync::Arc;

use parking_lot::Mutex;

use crate::travel::TravelStore;
use devtoolbox_application::travel::TravelStorePort;
use devtoolbox_core::travel::{CityGuide, SearchResult, TravelDocument};

/// 把 `Arc<Mutex<TravelStore>>`（SQLite 旅行缓存）包装成 application 的旅行存储端口；
/// 语义与 Gate 6 前 `TravelResearchService` 直接持锁读缓存一致（短锁、不跨 await）。
pub struct TravelStoreAdapter {
    store: Arc<Mutex<TravelStore>>,
}

impl TravelStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<TravelStore>>) -> Self {
        Self { store }
    }
}

impl TravelStorePort for TravelStoreAdapter {
    fn get_guide(&self, city: &str, days: u8, now: i64) -> Result<Option<CityGuide>, String> {
        self.store
            .lock()
            .get_guide(city, days, now)
            .map_err(err_text)
    }

    fn upsert_guide(&self, guide: &CityGuide, now: i64) -> Result<CityGuide, String> {
        self.store.lock().upsert_guide(guide, now).map_err(err_text)
    }

    fn get_search_results(
        &self,
        query: &str,
        now: i64,
    ) -> Result<Option<Vec<SearchResult>>, String> {
        self.store
            .lock()
            .get_search_results(query, now)
            .map_err(err_text)
    }

    fn put_search_results(
        &self,
        query: &str,
        results: &[SearchResult],
        now: i64,
    ) -> Result<(), String> {
        self.store
            .lock()
            .put_search_results(query, results, now)
            .map_err(err_text)
    }

    fn get_document(&self, url: &str, now: i64) -> Result<Option<TravelDocument>, String> {
        self.store.lock().get_document(url, now).map_err(err_text)
    }

    fn put_document(&self, document: &TravelDocument, now: i64) -> Result<(), String> {
        self.store
            .lock()
            .put_document(document, now)
            .map_err(err_text)
    }
}
/// 基础设施错误 → 端口错误（保持与原 desktop 适配器一致的文案）。
fn err_text(error: crate::error::InfrastructureError) -> String {
    error.to_string()
}
