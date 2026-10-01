//! Learning OS 应用层服务与端口。

pub mod ports;
pub mod service;
#[cfg(test)]
mod tests;

pub use ports::{LearningPortError, LearningStorePort};
pub use service::LearningService;
