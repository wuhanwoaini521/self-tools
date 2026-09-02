pub mod duckdb;
pub mod store;

pub use duckdb::{
    DatasetStats, HistoricalTextResult, HistoryDuckDbRepository, PersonPlaceResult,
    PersonRelationResult, PersonResult, WorkResult,
};
pub use store::HistoryStore;
