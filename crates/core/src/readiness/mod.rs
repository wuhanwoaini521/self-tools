//! 系统就绪度契约（V11 §123-§127）。
//!
//! 依赖方向：`core::readiness` 无内部依赖。它只表达「哪些子系统已就绪、
//! 程度如何」的统一形状，不持有任何探测实现（application 定义探测端口，
//! infrastructure / 组合根负责装配具体探测）。
//!
//! 安全约束：`ReadinessCheck.detail` 是**面向展示的受控文本**，只允许
//! 「已配置 / 未配置 / 能力布尔」这类陈述，严禁 API key、base URL、模型名
//! 或任何 secret 泄漏（§126）。

pub mod model;

pub use model::{
    DiagnosticCheck, ReadinessCheck, ReadinessCheckId, ReadinessReport, ReadinessStatus,
    aggregate_status,
};
