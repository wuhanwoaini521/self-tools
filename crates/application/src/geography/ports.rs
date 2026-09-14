//! Geography 查询端口。
//!
//! 端口面由 GeographyService 的 4 个用例（home / search / detail /
//! toggle_favorite）实际调用的存储方法反推，不含任何连接管理或路径能力；
//! 类型（GeoEntity / GeoRelation / GeoSource …）由 `devtoolbox_core::geography`
//! 提供，错误是应用端口自有的 `GeographyPortError`。

use std::fmt;

use devtoolbox_core::geography::{
    GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelation, GeoSource,
};

/// Geography 查询端口的错误（适配层负责把基础设施错误转换为可显示文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeographyPortError(pub String);

impl fmt::Display for GeographyPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GeographyPortError {}

/// Geography 知识库（只读查询面 + 收藏写入）的端口契约。
pub trait GeographyQueryPort {
    fn all_entities(&self) -> Result<Vec<GeoEntity>, GeographyPortError>;
    fn recent_ids(&self, limit: i64) -> Result<Vec<String>, GeographyPortError>;
    fn favorite_ids(&self) -> Result<Vec<String>, GeographyPortError>;
    fn map_snapshot(
        &self,
    ) -> Result<(Vec<GeoMapPoint>, Vec<GeoMapLine>), GeographyPortError>;
    fn search(
        &self,
        query: &str,
        entity_type: Option<GeoEntityType>,
        limit: usize,
    ) -> Result<Vec<GeoEntity>, GeographyPortError>;
    fn entity(&self, id: &str) -> Result<Option<GeoEntity>, GeographyPortError>;
    fn record_view(&self, id: &str) -> Result<(), GeographyPortError>;
    fn relations_for(&self, id: &str) -> Result<Vec<GeoRelation>, GeographyPortError>;
    fn sources(&self, ids: &[String]) -> Result<Vec<GeoSource>, GeographyPortError>;
    fn toggle_favorite(&self, id: &str) -> Result<bool, GeographyPortError>;
}