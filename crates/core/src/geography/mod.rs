//! Geography Explorer 的纯领域模型与确定性规则。

mod compare;
mod model;
mod recommendation;

pub use compare::{GeoCompareError, compare_entities};
pub use model::*;
pub use recommendation::GeoRecommendationService;
