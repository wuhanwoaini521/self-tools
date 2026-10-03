//! 系统就绪度：把后端 `ReadinessService` 暴露给前端（桌面命令 + 网页端只读端点）。
//!
//! 背景：`ReadinessService` 与全套探测早已实现且有测试，但**从未被装配**——前端拿不到
//! 后端报告，只能用 4 项本地兜底渲染 13 项，缺失项回落到「未配置」。于是页面会
//! **谎报** `database 未配置`，而数据库其实正常读写。那比没有这个页面更糟。
//!
//! 本模块是组合根侧的唯一装配点：把**真实路径**喂给既有探测
//! （`devtoolbox_application::readiness::default_probes`），不新增探测、不伪造状态。

use std::sync::Arc;

use tauri::AppHandle;

use devtoolbox_application::readiness::{ReadinessProbe, ReadinessService, default_probes};
use devtoolbox_core::readiness::{DiagnosticCheck, ReadinessReport};

use crate::CommandError;

/// 前端侧两项（安全上下文 / 设备会话）由浏览器判定，后端无法得知。
/// 这里如实标记为「由前端判定」，而不是伪造一个 ready。
fn frontend_owned_ids() -> [&'static str; 2] {
    ["pwa_secure_context", "device_session"]
}

fn build_service(app: &AppHandle) -> Result<ReadinessService, CommandError> {
    let config_dir = crate::project_config_directory_public(app)?;
    let settings = crate::settings_store(app)?
        .load()
        .map_err(|error| CommandError {
            code: "settings_error",
            message: error.to_string(),
        })?;

    // 关键修正：`default_probes` 里 DatabaseProbe / BackupProbe 接收的是**数据目录**，
    // 此前从未被调用，所以路径无从谈起。这里传真实的 config 目录。
    let probes: Vec<Arc<dyn ReadinessProbe>> = default_probes(
        &settings,
        &config_dir,
        // secure_context / identity 由前端判定 → 后端一律 false（前端会覆盖这两项）。
        false,
        false,
        settings.server.services.len(),
        false,
    );
    Ok(ReadinessService::new(probes))
}

/// 系统就绪度报告（后端真实探测 + 前端两项占位）。
#[tauri::command]
pub fn readiness_report(app: AppHandle) -> Result<ReadinessReport, CommandError> {
    let service = build_service(&app)?;
    let mut report = service.report();
    mark_frontend_owned(&mut report);
    Ok(report)
}

/// 逐项诊断（与报告同源，不重复探测）。
#[tauri::command]
pub fn readiness_diagnostics(app: AppHandle) -> Result<Vec<DiagnosticCheck>, CommandError> {
    let service = build_service(&app)?;
    Ok(service.diagnostics())
}

/// 把「由前端判定」的两项标成占位，避免它们被误读为后端结论。
fn mark_frontend_owned(report: &mut ReadinessReport) {
    for check in &mut report.checks {
        if frontend_owned_ids().contains(&check.id.as_str()) {
            check.detail = "由前端判定（安全上下文 / 设备会话）".to_string();
        }
    }
}
