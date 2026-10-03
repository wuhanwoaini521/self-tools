//! 共享端口适配器：Geography 存储 → `GeographyQueryPort`。
//!
//! 原先只在 `apps/desktop`，服务端无法复用 —— 网页端的 Geography 因此只能退回
//! 前端硬编码的 12 条示例数据。下沉后两端共用同一份装配语义与同一份真实库。

use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::InfrastructureError;
use crate::geography::GeographyStore;
use devtoolbox_application::geography::{GeographyPortError, GeographyQueryPort};
use devtoolbox_core::geography::{
    GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelation, GeoSource,
};

/// 把基础设施错误转换为端口错误（保留原始可显示文本）。
fn map_err(error: InfrastructureError) -> GeographyPortError {
    GeographyPortError(error.to_string())
}

#[derive(Clone)]
pub struct GeographyQueryAdapter {
    store: Arc<Mutex<GeographyStore>>,
}

impl GeographyQueryAdapter {
    pub fn new(store: Arc<Mutex<GeographyStore>>) -> Self {
        Self { store }
    }
}

impl GeographyQueryPort for GeographyQueryAdapter {
    fn all_entities(&self) -> Result<Vec<GeoEntity>, GeographyPortError> {
        self.store.lock().all_entities().map_err(map_err)
    }
    fn recent_ids(&self, limit: i64) -> Result<Vec<String>, GeographyPortError> {
        self.store.lock().recent_ids(limit).map_err(map_err)
    }
    fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError> {
        self.store.lock().favorite_ids().map_err(map_err)
    }
    fn map_snapshot(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), GeographyPortError> {
        self.store.lock().map_snapshot().map_err(map_err)
    }
    fn search(
        &self,
        query: &str,
        entity_type: Option<GeoEntityType>,
        limit: usize,
    ) -> Result<Vec<GeoEntity>, GeographyPortError> {
        self.store
            .lock()
            .search(query, entity_type, limit)
            .map_err(map_err)
    }
    fn entity(&self, id: &str) -> Result<Option<GeoEntity>, GeographyPortError> {
        self.store.lock().entity(id).map_err(map_err)
    }
    fn record_view(&self, id: &str) -> Result<(), GeographyPortError> {
        self.store.lock().record_view(id).map_err(map_err)
    }
    fn relations_for(&self, id: &str) -> Result<Vec<GeoRelation>, GeographyPortError> {
        self.store.lock().relations_for(id).map_err(map_err)
    }
    fn sources(&self, ids: &[String]) -> Result<Vec<GeoSource>, GeographyPortError> {
        self.store.lock().sources(ids).map_err(map_err)
    }
    fn toggle_favorite(&self, id: &str) -> Result<bool, GeographyPortError> {
        self.store.lock().toggle_favorite(id).map_err(map_err)
    }
}
