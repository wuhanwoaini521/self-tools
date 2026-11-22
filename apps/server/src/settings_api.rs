//! 设置写入的 HTTP 面（Phase 1.5）。
//!
//! 两条写入通道，都要求网关验证过的 **admin** 身份，且都只写「不是主机权限」
//! 的部分：
//!
//! - `PUT /api/v1/settings/public`：`PublicSettingsUpdate` 是一个显式白名单，
//!   主题 / AI / Travel / 决策层的可公开字段；`workspace_path`、`knowledge`、
//!   `server.services` 这类绑定本机路径与命令的字段在反序列化阶段就被拒绝。
//! - `POST /api/v1/settings/secrets`：只接受 `SecretSettingsField` 枚举中的凭据，
//!   响应只有 `*_configured` 布尔，绝不回显值。
//!
//! 审计只记 principal、字段名、结果与耗时；`target_id` 里永远不出现写入的值。
//! 设置文件读失败时直接报错，不用默认值覆盖磁盘上的配置。

use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::Extension;

use devtoolbox_core::server::ActionOutcome;
use devtoolbox_core::settings::{
    PublicSettingsUpdate, SecretSettingsStatus, SecretSettingsUpdate, TravelSearchBackend,
};
use devtoolbox_runtime::server::ServerRuntime;

use crate::ai_api::{ApiError, SettingsAccess, SettingsStatus};
use crate::gateway_auth::AuthenticatedRemote;
use crate::server_api::require_admin;

/// 单个凭据的长度上限（字符）。真实 key 远小于此值；上限只用来拒绝异常载荷。
const MAX_SECRET_CHARS: usize = 4096;
/// URL / 模型名等公开文本字段的上限。
const MAX_TEXT_CHARS: usize = 2048;

/// settings.json 是「读—改—写」：两个并发请求会互相覆盖，所以写入串行化。
/// 单进程、单文件，一个静态锁就够；读路径不受影响。
static SETTINGS_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `PUT /api/v1/settings/public`
pub async fn put_public_settings(
    remote: Option<Extension<AuthenticatedRemote>>,
    Extension(store): Extension<Arc<dyn SettingsAccess>>,
    Extension(server): Extension<Arc<ServerRuntime>>,
    Json(update): Json<PublicSettingsUpdate>,
) -> Result<Json<SettingsStatus>, ApiError> {
    let remote = require_admin(remote)?;
    validate_public(&update)?;

    let _write_guard = SETTINGS_WRITE_LOCK.lock().await;
    let started = Instant::now();
    let mut settings = store.load_for_write().map_err(read_failed)?;
    let touched = update.apply_to(&mut settings);
    if touched.is_empty() {
        // 空更新不写盘、也不产生审计噪音。
        return Ok(Json(status_of(settings)));
    }
    let target = touched.join(",");
    if let Err(error) = store.save(&settings) {
        server.actions.record_config_change(
            &remote.session_id,
            "settings_update",
            &target,
            ActionOutcome::Failed,
            Some("settings_write_failed"),
            started.elapsed(),
        );
        return Err(write_failed(error));
    }
    server.actions.record_config_change(
        &remote.session_id,
        "settings_update",
        &target,
        ActionOutcome::Success,
        None,
        started.elapsed(),
    );
    Ok(Json(status_of(settings)))
}

/// `POST /api/v1/settings/secrets`
pub async fn put_secret_settings(
    remote: Option<Extension<AuthenticatedRemote>>,
    Extension(store): Extension<Arc<dyn SettingsAccess>>,
    Extension(server): Extension<Arc<ServerRuntime>>,
    Json(update): Json<SecretSettingsUpdate>,
) -> Result<Json<SecretSettingsStatus>, ApiError> {
    let remote = require_admin(remote)?;
    for item in &update.updates {
        if let Some(value) = &item.value
            && value.chars().count() > MAX_SECRET_CHARS
        {
            return Err(invalid(format!(
                "{} 超过 {MAX_SECRET_CHARS} 字符上限",
                item.field.as_str()
            )));
        }
    }

    let _write_guard = SETTINGS_WRITE_LOCK.lock().await;
    let started = Instant::now();
    let mut settings = store.load_for_write().map_err(read_failed)?;
    if update.updates.is_empty() {
        return Ok(Json(settings.public_view().secrets));
    }
    settings.apply_secret_updates(&update);
    let fields = update
        .updates
        .iter()
        .map(|item| item.field.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if let Err(error) = store.save(&settings) {
        server.actions.record_config_change(
            &remote.session_id,
            "settings_secret_update",
            &fields,
            ActionOutcome::Failed,
            Some("settings_write_failed"),
            started.elapsed(),
        );
        return Err(write_failed(error));
    }
    server.actions.record_config_change(
        &remote.session_id,
        "settings_secret_update",
        &fields,
        ActionOutcome::Success,
        None,
        started.elapsed(),
    );
    Ok(Json(settings.public_view().secrets))
}

fn status_of(settings: devtoolbox_core::settings::AppSettings) -> SettingsStatus {
    SettingsStatus {
        ai_configured: settings.ai.is_configured(),
        public: settings.public_view(),
    }
}

/// 白名单字段的格式校验：形状错误应在写盘前被拒绝。
fn validate_public(update: &PublicSettingsUpdate) -> Result<(), ApiError> {
    if let Some(theme) = &update.ui_theme {
        check_text("ui_theme", theme, 64)?;
        if theme.trim().is_empty() {
            return Err(invalid("ui_theme 不能为空".to_string()));
        }
    }
    if let Some(ai) = &update.ai {
        check_optional_text("ai.provider", &ai.provider, 128)?;
        check_optional_text("ai.model", &ai.model, 128)?;
        check_optional_url("ai.base_url", &ai.base_url)?;
        if let Some(timeout) = ai.timeout_secs
            && !(1..=600).contains(&timeout)
        {
            return Err(invalid("ai.timeout_secs 需要在 1..=600 之间".to_string()));
        }
    }
    if let Some(travel) = &update.travel {
        check_optional_url("travel.searxng_url", &travel.searxng_url)?;
        check_optional_url("travel.llm_base_url", &travel.llm_base_url)?;
        check_optional_text("travel.llm_model", &travel.llm_model, 128)?;
        check_optional_text("travel.qweather_api_host", &travel.qweather_api_host, 255)?;
        if travel.search_backend == Some(TravelSearchBackend::Searxng)
            && travel
                .searxng_url
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(invalid(
                "选择 searxng 后端时必须同时提供 travel.searxng_url".to_string(),
            ));
        }
    }
    if let Some(decision) = &update.decision {
        if let Some(mode) = &decision.mode {
            check_text("decision.mode", mode, 32)?;
            if mode.trim().is_empty() {
                return Err(invalid("decision.mode 不能为空".to_string()));
            }
        }
        check_optional_url("decision.jev_base_url", &decision.jev_base_url)?;
        check_optional_text("decision.jev_model", &decision.jev_model, 128)?;
        if let Some(timeout) = decision.jev_timeout_secs
            && !(1..=600).contains(&timeout)
        {
            return Err(invalid(
                "decision.jev_timeout_secs 需要在 1..=600 之间".to_string(),
            ));
        }
        if let Some(workers) = decision.max_workers
            && !(1..=32).contains(&workers)
        {
            return Err(invalid(
                "decision.max_workers 需要在 1..=32 之间".to_string(),
            ));
        }
    }
    Ok(())
}

fn check_text(name: &str, value: &str, max: usize) -> Result<(), ApiError> {
    if value.chars().count() > max {
        return Err(invalid(format!("{name} 超过 {max} 字符上限")));
    }
    Ok(())
}

fn check_optional_text(name: &str, value: &Option<String>, max: usize) -> Result<(), ApiError> {
    if let Some(value) = value {
        check_text(name, value, max)?;
    }
    Ok(())
}

/// 只接受 http(s)：设置里的 base_url 会变成后端出站请求目标。
fn check_optional_url(name: &str, value: &Option<String>) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Ok(());
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    check_text(name, trimmed, MAX_TEXT_CHARS)?;
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(invalid(format!("{name} 必须是 http(s) 地址")));
    }
    Ok(())
}

fn invalid(message: String) -> ApiError {
    ApiError {
        code: "settings_invalid",
        message,
    }
}

fn read_failed(message: String) -> ApiError {
    ApiError {
        code: "settings_write_failed",
        message: format!("读取现有设置失败，已中止写入：{message}"),
    }
}

fn write_failed(message: String) -> ApiError {
    ApiError {
        code: "settings_write_failed",
        message: format!("保存设置失败：{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use devtoolbox_core::settings::{AppSettings, SecretSettingsField, SecretSettingsValueUpdate};
    use devtoolbox_runtime::history_enrichment::SettingsLoader;
    use parking_lot::Mutex;
    use tower::ServiceExt;

    /// 可读写的内存设置存储；`save` 记录写入次数。
    struct MemorySettings {
        settings: Mutex<AppSettings>,
        saves: Mutex<usize>,
        fail_read: bool,
    }

    impl MemorySettings {
        fn new(settings: AppSettings) -> Self {
            Self {
                settings: Mutex::new(settings),
                saves: Mutex::new(0),
                fail_read: false,
            }
        }

        fn unreadable() -> Self {
            Self {
                settings: Mutex::new(AppSettings::default()),
                saves: Mutex::new(0),
                fail_read: true,
            }
        }
    }

    impl SettingsAccess for MemorySettings {
        fn load(&self) -> AppSettings {
            self.settings.lock().clone()
        }

        fn load_for_write(&self) -> Result<AppSettings, String> {
            if self.fail_read {
                return Err("settings.json 解析失败".to_string());
            }
            Ok(self.settings.lock().clone())
        }

        fn save(&self, settings: &AppSettings) -> Result<(), String> {
            *self.settings.lock() = settings.clone();
            *self.saves.lock() += 1;
            Ok(())
        }
    }

    fn loader(store: Arc<MemorySettings>) -> SettingsLoader {
        Arc::new(move || Ok(store.load()))
    }

    fn server_for(store: &Arc<MemorySettings>) -> Arc<ServerRuntime> {
        let directory = tempfile::tempdir().expect("tempdir");
        Arc::new(
            ServerRuntime::assemble(
                directory.path(),
                loader(Arc::clone(store)),
                devtoolbox_runtime::server::web_trust(),
                Vec::new(),
                Vec::new(),
                Arc::new(devtoolbox_runtime::server::DisabledServiceControl),
            )
            .expect("assemble"),
        )
    }

    fn admin() -> Option<Extension<AuthenticatedRemote>> {
        Some(Extension(AuthenticatedRemote::new(
            "lan-owner",
            crate::gateway_auth::GatewayRole::Admin,
        )))
    }

    fn router(store: Arc<MemorySettings>, server: Arc<ServerRuntime>) -> axum::Router {
        let access: Arc<dyn SettingsAccess> = store;
        axum::Router::new()
            .route(
                "/api/v1/settings/public",
                axum::routing::put(put_public_settings),
            )
            .route(
                "/api/v1/settings/secrets",
                axum::routing::post(put_secret_settings),
            )
            .layer(Extension(access))
            .layer(Extension(server))
    }

    #[tokio::test]
    async fn direct_loopback_settings_write_is_rejected_without_a_marker() {
        let store = Arc::new(MemorySettings::new(AppSettings::default()));
        let app = router(Arc::clone(&store), server_for(&store));
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/settings/public")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ui_theme":"warm-editorial"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(*store.saves.lock(), 0);
    }

    #[tokio::test]
    async fn admin_update_changes_only_whitelisted_fields_and_is_audited() {
        let mut initial = AppSettings {
            workspace_path: Some("E:/work".into()),
            ..AppSettings::default()
        };
        initial.ai.api_key = Some("keep-me".into());
        let store = Arc::new(MemorySettings::new(initial));
        let server = server_for(&store);

        let update = serde_json::from_str::<PublicSettingsUpdate>(
            r#"{"ui_theme":"warm-editorial","ai":{"model":"deepseek-chat"}}"#,
        )
        .expect("update");
        let Json(status) = put_public_settings(
            admin(),
            Extension(Arc::clone(&store) as Arc<dyn SettingsAccess>),
            Extension(Arc::clone(&server)),
            Json(update),
        )
        .await
        .expect("saved");

        assert_eq!(status.public.ui_theme, "warm-editorial");
        let saved = store.load();
        assert_eq!(saved.ai.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(saved.ai.api_key.as_deref(), Some("keep-me"));
        assert_eq!(
            saved.workspace_path.as_deref(),
            Some(std::path::Path::new("E:/work"))
        );

        let audit = server.recent_actions(10);
        let entry = audit.first().expect("audit entry");
        assert_eq!(entry.action_type, "settings_update");
        assert!(entry.target_id.contains("ui_theme"));
        assert!(entry.target_id.contains("ai.model"));
    }

    #[tokio::test]
    async fn secret_write_returns_only_configured_flags() {
        let store = Arc::new(MemorySettings::new(AppSettings::default()));
        let server = server_for(&store);
        let Json(status) = put_secret_settings(
            admin(),
            Extension(Arc::clone(&store) as Arc<dyn SettingsAccess>),
            Extension(Arc::clone(&server)),
            Json(SecretSettingsUpdate {
                updates: vec![SecretSettingsValueUpdate {
                    field: SecretSettingsField::AiApiKey,
                    value: Some("sk-live-value".into()),
                }],
            }),
        )
        .await
        .expect("saved");

        assert!(status.ai_api_key_configured);
        let json = serde_json::to_string(&status).expect("json");
        assert!(!json.contains("sk-live-value"));
        let audit = server.recent_actions(10);
        assert_eq!(audit[0].action_type, "settings_secret_update");
        assert_eq!(audit[0].target_id, "ai_api_key");
        assert!(
            !serde_json::to_string(&audit[0])
                .expect("json")
                .contains("sk-live-value")
        );
    }

    #[tokio::test]
    async fn unwritable_or_unreadable_settings_fail_closed_without_overwriting() {
        let store = Arc::new(MemorySettings::unreadable());
        let server = server_for(&store);
        let update =
            serde_json::from_str::<PublicSettingsUpdate>(r#"{"auto_save":false}"#).expect("update");
        let error = put_public_settings(
            admin(),
            Extension(Arc::clone(&store) as Arc<dyn SettingsAccess>),
            Extension(Arc::clone(&server)),
            Json(update),
        )
        .await
        .expect_err("read failure aborts the write");
        assert_eq!(error.code, "settings_write_failed");
        assert_eq!(*store.saves.lock(), 0);

        // 即使读取成功，非法 URL 也在写盘前被拒绝。
        let store = Arc::new(MemorySettings::new(AppSettings::default()));
        let server = server_for(&store);
        let error = put_public_settings(
            admin(),
            Extension(Arc::clone(&store) as Arc<dyn SettingsAccess>),
            Extension(Arc::clone(&server)),
            Json(
                serde_json::from_str(r#"{"ai":{"base_url":"file:///etc/passwd"}}"#)
                    .expect("update"),
            ),
        )
        .await
        .expect_err("invalid url");
        assert_eq!(error.code, "settings_invalid");
        assert_eq!(*store.saves.lock(), 0);
    }
}
