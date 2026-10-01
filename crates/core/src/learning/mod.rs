//! Personal Knowledge & Learning OS (V11) 核心领域模型与算法。
//!
//! 提供跨模块统一的：
//! - `LearningEvent`（学习行为事件流）
//! - `LearningProgress` & Mastery 评分（多级熟练度与确定性掌握度算法）
//! - `UniversalReviewCard` & SRS 间隔复习调度器（SM-2 / 记忆衰减模型）
//! - `GraphNode` & `GraphEdge`（跨模块统一知识图谱抽象）
//! - `Collection` & `CollectionItem`（跨模块知识合集）
//! - `TodayDashboardData` & `ExploreRecommendation`（Today 与探索聚合数据契约）

pub mod model;

pub use model::{
    Collection, CollectionItem, CollectionItemRef, ContinueItem, EntityType, ExploreRecommendation,
    GraphEdge, GraphNeighborhood, GraphNode, LearningAction, LearningEvent, LearningProgress,
    LearningStatus, MasteryCalculator, RelationKind, ReviewCardType, ReviewQueueItem,
    ReviewQueueStats, ReviewRating, ReviewScheduleOutcome, SpacedRepetitionScheduler,
    TodayDashboardData, UniversalReviewCard,
};
