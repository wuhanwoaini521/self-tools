//! GeographyQueryPort 的桌面端适配器：把用例层接口绑定到
//! `GeographyStore`（infra 的 SQLite 存储）。每个方法原样转发，
//! 不做任何业务决策 —— 决策都在 application 用例层。
//!
//! Port 及其错误类型属于 application crate；infrastructure 不能反向依赖
//! application（否则成环），所以适配器放在桌面端组合层，并把基础设施错误
//! 转换为 `GeographyPortError` 的可显示文本（消息文本保持不变）。

use std::sync::{Arc, Mutex};

use devtoolbox_application::geography::{GeographyPortError, GeographyQueryPort};
use devtoolbox_core::geography::{
    GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelation, GeoSource,
};
use devtoolbox_infrastructure::{GeographyStore, InfrastructureError};

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
        self.store
            .lock()
            .expect("geography store poisoned")
            .all_entities()
            .map_err(map_err)
    }
    fn recent_ids(&self, limit: i64) -> Result<Vec<String>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .recent_ids(limit)
            .map_err(map_err)
    }
    fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .favorite_ids()
            .map_err(map_err)
    }
    fn map_snapshot(&self) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .map_snapshot()
            .map_err(map_err)
    }
    fn search(
        &self,
        query: &str,
        entity_type: Option<GeoEntityType>,
        limit: usize,
    ) -> Result<Vec<GeoEntity>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .search(query, entity_type, limit)
            .map_err(map_err)
    }
    fn entity(&self, id: &str) -> Result<Option<GeoEntity>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .entity(id)
            .map_err(map_err)
    }
    fn record_view(&self, id: &str) -> Result<(), GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .record_view(id)
            .map_err(map_err)
    }
    fn relations_for(&self, id: &str) -> Result<Vec<GeoRelation>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .relations_for(id)
            .map_err(map_err)
    }
    fn sources(&self, ids: &[String]) -> Result<Vec<GeoSource>, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .sources(ids)
            .map_err(map_err)
    }
    fn toggle_favorite(&self, id: &str) -> Result<bool, GeographyPortError> {
        self.store
            .lock()
            .expect("geography store poisoned")
            .toggle_favorite(id)
            .map_err(map_err)
    }
}