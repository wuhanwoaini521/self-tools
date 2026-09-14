//! Travel 用例编排。

pub mod ports;
pub mod service;
pub mod session;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;

pub use ports::TravelStorePort;
pub use service::{
    ResearchOutcome, TravelResearchRequest, TravelResearchService, fact_user_prompt,
    guide_user_prompt, parse_query_list, parse_search_intents, query_user_prompt,
};
pub use session::{
    SharedResearchSession, TravelResearchSession, TravelSessionRegistry, TravelSessionView,
};
