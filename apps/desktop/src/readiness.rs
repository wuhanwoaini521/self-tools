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
use devtoolbox_core::readiness::{
    DiagnosticCheck, ReadinessReport, mark_frontend_owned as mark_frontend_owned_report,
    mark_frontend_owned_diagnostics,
};

use crate::CommandError;

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
    // 前端侧两项（安全上下文 / 设备会话）由浏览器判定，后端无法得知。
    // 共用 core 的标记函数，而不是各自拼一句文案。
    mark_frontend_owned_report(&mut report.checks);
    Ok(report)
}

/// 逐项诊断（与报告同源，不重复探测）。
#[tauri::command]
pub fn readiness_diagnostics(app: AppHandle) -> Result<Vec<DiagnosticCheck>, CommandError> {
    let service = build_service(&app)?;
    let mut diagnostics = service.diagnostics();
    // 诊断面板也必须说「由前端判定」，否则这里会显示一个后端并未测过的结论。
    mark_frontend_owned_diagnostics(&mut diagnostics);
    Ok(diagnostics)
}
