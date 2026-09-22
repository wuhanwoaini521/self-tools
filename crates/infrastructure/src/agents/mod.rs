//! V10 决策层基础设施适配器：Jev decision provider。
//!
//! **隔离铁律**：Jev-specific（endpoint、Bearer key、模型名、choice/noul 形状）
//! 只在本模块；core/application 只认识 `DecisionProvider`。

pub mod jev_decision;

pub use jev_decision::{
    DEFAULT_BASE_URL, DEFAULT_MODEL, FakeJevTransport, JevConfig, JevDecisionProvider,
    JevHttpTransport, JevQuestion, JevRequestBody, JevResponseBody, JevTransport, JevUsage,
};
