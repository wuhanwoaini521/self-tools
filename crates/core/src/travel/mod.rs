//! Travel 领域层（纯规则，无 I/O / 无 UI / 无网络）。
//!
//! 包含：领域模型（`model`）、结构化攻略（`guide`）、查询规划（`query_planner`）、
//! 来源可信度与评分（`ranking`）、去重与多源验证（`dedup`）、
//! LLM 输出容错解析（`llm_parse`）、缓存策略（`cache`）。

pub mod cache;
pub mod dedup;
pub mod guide;
pub mod llm_parse;
pub mod model;
pub mod query_planner;
pub mod ranking;

pub use cache::{DOCUMENT_TTL_SECS, GUIDE_TTL_SECS, SEARCH_TTL_SECS, is_fresh};
pub use dedup::{dedup_facts, dedup_search_results, normalize_url, verify_facts};
pub use guide::{
    AccommodationArea, Attraction, CityGuide, CityInfo, DistrictInfo, Food, GuideMeta,
    GuideSummary, Itineraries, Itinerary, ItineraryStop, Place, TransportGuide, TravelTip,
    TravelWarning, VerifiedFact, VerifiedValue,
};
pub use llm_parse::{TravelParseError, extract_json, parse_facts_json, parse_guide_json};
pub use model::{
    ContentState, FactCategory, ResearchPhase, SearchResult, SourceLevel, StepStatus,
    TravelDocument, TravelFact, TravelResearchEvent, TravelSource,
};
pub use query_planner::{QueryCategory, QueryTask, TravelQueryInput, TravelQueryPlanner};
pub use ranking::{classify_source, freshness_score, host_of, rate_source, relevance_score};
