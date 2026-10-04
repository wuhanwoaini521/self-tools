//! 共享应用组合层（Composition Root）。
//!
//! ## 为什么有这个 crate
//!
//! 能力（store / 应用服务 / provider 装配）一直实现在 core+application+infrastructure，
//! 但**组合根**只写在桌面端（`apps/desktop/src/{composition,server,travel_ai,
//! history_enrichment,history_query,personal_ai}.rs`），server 引用不到。
//! 结果是网页端长期缺接口：AI、Travel、知识库、语言导入都要人工补一次端点，
//! 且补不全 —— **同一份能力，两套装配，必然漂移**。
//!
//! 现在两端共用本 crate：
//! - 桌面端：`AppState` 由 [`AppCore::build`] 组装，命令只做参数校验与传输；
//! - 网页端：HTTP handler 直接取 [`AppCore`] 的服务。
//!
//! 本 crate **不含任何 Tauri 依赖**（原生对话框、事件推送留在桌面端），
//! 因此两端的行为天然一致：同一份 settings、同一批数据库、同一套 provider。

pub mod composition;
pub mod history_enrichment;
pub mod history_query;
pub mod personal_ai;
pub mod server;
pub mod server_adapters;
pub mod travel_ai;
pub mod travel_providers;

mod app_core;
pub use app_core::AppCore;

// 组合根要用到的 core 类型在此再导出，方便调用方只依赖 runtime 一个 crate。
pub use devtoolbox_core::server::SessionTrust;
