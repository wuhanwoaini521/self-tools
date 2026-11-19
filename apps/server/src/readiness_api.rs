//! 系统就绪度 HTTP 接口（网页端只读）。
//!
//! 与桌面端共用 `default_probes` + 同一份 settings / 同一个数据目录，
//! 因此两端结论一致——不会出现「桌面说数据库正常、网页说未配置」。

use std::path::PathBuf;
use std::sync::Arc;

use axum::Extension;
use devtoolbox_application::readiness::{ReadinessProbe, ReadinessService, default_probes};
use devtoolbox_core::readiness::{DiagnosticCheck, ReadinessReport};
use devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime;

use crate::ai_api::SettingsAccess;

fn build(
    store: &dyn SettingsAccess,
    data_dir: &std::path::Path,
    search_sources: usize,
) -> ReadinessService {
    let settings = store.load();
    let probes: Vec<Arc<dyn ReadinessProbe>> = default_probes(
        &settings,
        data_dir,
        false, // secure_context：前端判定
        false, // identity：前端判定
        // 检索源数量必须取自真正的全局检索注册表。
        // 历史实现传的是 `settings.server.services.len()`（Home Server 服务数），
        // 两者无关，导致即使 memory/documents/files/language 四个源都已装配，
        // readiness 也永远报「尚未注册检索源」。
        search_sources,
        false, // vision：当前未接入
    );
    ReadinessService::new(probes)
}

/// 前端会覆盖的两项（安全上下文 / 设备会话）在此标记清楚。
fn mark_frontend_owned(report: &mut ReadinessReport) {
    for check in &mut report.checks {
        if matches!(check.id.as_str(), "pwa_secure_context" | "device_session") {
            check.detail = "由前端判定（安全上下文 / 设备会话）".to_string();
        }
    }
}

/// `GET /api/v1/readiness`
pub async fn readiness_report(
    Extension(store): Extension<Arc<dyn SettingsAccess>>,
    Extension(data_dir): Extension<Arc<PathBuf>>,
    Extension(knowledge): Extension<Arc<KnowledgeRuntime>>,
) -> Result<axum::Json<ReadinessReport>, crate::ai_api::ApiError> {
    let mut report = build(
        store.as_ref(),
        data_dir.as_path(),
        knowledge.search.source_count(),
    )
    .report();
    mark_frontend_owned(&mut report);
    Ok(axum::Json(report))
}

/// `GET /api/v1/readiness/diagnostics`
pub async fn readiness_diagnostics(
    Extension(store): Extension<Arc<dyn SettingsAccess>>,
    Extension(data_dir): Extension<Arc<PathBuf>>,
    Extension(knowledge): Extension<Arc<KnowledgeRuntime>>,
) -> Result<axum::Json<Vec<DiagnosticCheck>>, crate::ai_api::ApiError> {
    Ok(axum::Json(
        build(
            store.as_ref(),
            data_dir.as_path(),
            knowledge.search.source_count(),
        )
        .diagnostics(),
    ))
}
