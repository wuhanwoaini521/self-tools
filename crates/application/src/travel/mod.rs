//! Travel 用例编排。

pub mod service;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;

pub use service::{
    TravelResearchRequest, TravelResearchService, fact_user_prompt, guide_user_prompt,
    parse_query_list, query_user_prompt,
};
