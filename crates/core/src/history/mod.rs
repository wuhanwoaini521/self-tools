//! History Explorer 的纯领域层：模型、关系和离线推荐规则。

pub mod model;
pub mod recommendation;

pub use model::{
    Dynasty, HistoricalArtifact, HistoricalEvent, HistoricalInstitution, HistoricalPerson,
    HistoricalPlace, HistoryDetail, HistoryDetailView, HistoryDocument, HistoryFact, HistoryNode,
    HistoryNodeKind, HistoryPeriod, HistoryRelation, HistoryRelationKind, HistoryRelationView,
    HistorySearchGroup, HistorySection, HistorySource, SourceAuthority,
};
pub use recommendation::HistoryRecommendationService;
