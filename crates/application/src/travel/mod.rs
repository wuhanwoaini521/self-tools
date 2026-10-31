//! Travel 用例编排。

pub mod plan_service;
pub mod ports;
pub mod service;
pub mod session;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;

pub use plan_service::{TravelPlanChangeError, TravelPlanChangeService};
pub use ports::{
    GeographyPlace, GeographyPlaceLookup, TravelPlanPortError, TravelPlanStorePort, TravelStorePort,
};
pub use service::{
    ResearchOutcome, TravelResearchRequest, TravelResearchService, fact_user_prompt,
    guide_user_prompt, parse_query_list, parse_search_intents, query_user_prompt,
};
pub use session::{
    SharedResearchSession, TravelResearchSession, TravelSessionRegistry, TravelSessionView,
};

pub use ports::{TravelAiPort, TravelSearchHit, TripContext};
