//! Geography Explorer 用例编排：前端不直接读取 SQLite，也不拼装关系。
//!
//! `GeographyService` 通过 `GeographyQueryPort` 读取查询面，用例决策
//! （推荐、分组、关系视图、来源合并）全部在应用层；Port 的实际实现由
//! 平台适配层（Desktop / 未来 HTTP）绑定到 `GeographyStore`。

mod ports;
mod service;

pub use devtoolbox_core::geography::{
    GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRecommendation,
    GeoRelation, GeoRelationKind, GeoSearchGroup, GeoSource, GeographyHome,
};
pub use ports::{GeographyPortError, GeographyQueryPort};
pub use service::GeographyService;
