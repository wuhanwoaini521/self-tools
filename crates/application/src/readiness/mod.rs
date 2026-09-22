//! 系统就绪度域（V11 §123-§127）。
//!
//! 依赖方向：只依赖 `devtoolbox-core::readiness` 契约；探测实现由组合根
//! 提供（infrastructure / Tauri / HTTP 适配），本模块只做只读聚合。

pub mod service;

pub use service::{ReadinessProbe, ReadinessService};

#[cfg(test)]
mod tests;
