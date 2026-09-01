//! Geography Explorer 用例编排：前端不直接读取 SQLite，也不拼装关系。

mod service;

pub use devtoolbox_core::geography::{
    CompareMetric, GeoCompareView, GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine,
    GeoMapPoint, GeoRecommendation, GeoRelation, GeoRelationKind, GeoSearchGroup, GeoSource,
    GeographyHome,
};
pub use service::GeographyService;
