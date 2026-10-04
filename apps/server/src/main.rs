//! Self Tools 极简 HTTP 服务（Gate 9 试点）：只读 History 知识库。
//!
//! 本二进制只做：配置（env + CLI）→ 日志 → 组合根（只读 DuckDB 仓库 +
//! 用例服务）→ 路由（`routes`）→ 监听与优雅退出。不包含鉴权、CORS、写入端点。

mod ai_api;
mod geography_api;
mod knowledge_api;
mod language_api;
mod language_course_api;
mod language_data_api;
mod language_write_api;
mod learning_api;
mod news_api;
mod readiness_api;
mod routes;
mod travel_api;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use devtoolbox_application::history::HistoryService;

/// 默认只监听本机；要暴露到局域网时显式设置 `SELF_TOOLS_BIND`（Gate 9：默认 127.0.0.1）。
const DEFAULT_BIND: &str = "127.0.0.1:8080";
/// 唯一事实源（Gate 5.5）：`history-data-pipeline/dist/history.duckdb`。
const DEFAULT_HISTORY_DB: &str = "history-data-pipeline/dist/history.duckdb";
const ENV_BIND: &str = "SELF_TOOLS_BIND";
const ENV_HISTORY_DB: &str = "SELF_TOOLS_HISTORY_DB";
/// Language / 学习库 / 设置目录。默认指向**桌面端同一个目录**，
/// 这样网页端配好的 AI provider 与学习进度，桌面端立刻可见，反之亦然。
const ENV_DATA_DIR: &str = "SELF_TOOLS_DATA_DIR";
const DEFAULT_DATA_DIR: &str = "apps/desktop/config";

#[derive(Debug)]
struct Config {
    bind: SocketAddr,
    history_db: PathBuf,
    data_dir: PathBuf,
}

/// 配置解析：CLI 覆盖 env，env 覆盖默认值。未知参数 → 读取错误（exit 2）。
fn load_config() -> Result<Config, String> {
    let mut bind = std::env::var(ENV_BIND)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    let mut history_db = std::env::var(ENV_HISTORY_DB)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_HISTORY_DB.to_string());
    let mut data_dir = std::env::var(ENV_DATA_DIR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DATA_DIR.to_string());

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                bind = args.next().ok_or("--bind requires a value")?;
            }
            "--history-db" => {
                history_db = args.next().ok_or("--history-db requires a value")?;
            }
            "--data-dir" => {
                data_dir = args.next().ok_or("--data-dir requires a value")?;
            }
            "--help" | "-h" => {
                println!(
                    "usage: devtoolbox-server [--bind ADDR] [--history-db PATH]\n\
                     env:   SELF_TOOLS_BIND (default {DEFAULT_BIND})\n\
                     \tSELF_TOOLS_HISTORY_DB (default {DEFAULT_HISTORY_DB})\n\
                     \tRUST_LOG (default info)"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let bind: SocketAddr = bind
        .parse()
        .map_err(|error| format!("invalid bind address '{bind}' ({ENV_BIND}): {error}"))?;
    Ok(Config {
        bind,
        history_db: PathBuf::from(history_db),
        data_dir: PathBuf::from(data_dir),
    })
}

/// 轻量日志（Gate 9.5）：日志级别由 `RUST_LOG` 控制（默认 info）。
/// 仅记录启动 / 监听 / 知识库就绪 / 请求错误 / 关闭；不记录任何密钥。
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Ctrl+C 与 SIGTERM 都触发优雅关闭（Gate 9.5）。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received, closing HTTP server");
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match load_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error}");
            return ExitCode::from(2);
        }
    };
    init_logging();

    // ------------------------------------------------------------------
    // 共享组合根（Gate 9）
    //
    // 桌面端与网页端现在共用 `devtoolbox_runtime::AppCore`：同一份 settings、
    // 同一批 SQLite、同一套 provider。此前 server 在这里**另写了一套装配**，
    // 只覆盖一部分模块，于是 AI / Travel / 知识库 / 语言导入长期缺接口——
    // 每次补一个仍会漏，因为根因是「两套装配必然漂移」。
    // ------------------------------------------------------------------
    let core = match devtoolbox_runtime::AppCore::build(
        &config.data_dir,
        config.history_db.clone(),
        devtoolbox_runtime::server::web_trust(),
    ) {
        Ok(core) => Arc::new(core),
        Err(error) => {
            error!("runtime unavailable: {error}");
            return ExitCode::from(3);
        }
    };
    info!(
        db = %config.history_db.display(),
        "runtime ready (shared with desktop)"
    );

    let listener = match tokio::net::TcpListener::bind(config.bind).await {
        Ok(listener) => listener,
        Err(error) => {
            error!("cannot bind {}: {error}", config.bind);
            return ExitCode::from(1);
        }
    };

    let service = HistoryService::new(Box::new(
        devtoolbox_runtime::history_query::HistoryQueryAdapter::new(Arc::clone(&core.history_repo)),
    ));
    let language_store = Arc::clone(&core.language_store);
    let content: Arc<dyn devtoolbox_application::language::LanguageStorePort> =
        Arc::clone(&core.language_port);
    let learning_os = Arc::clone(&core.learning);
    let learning = Arc::new(
        devtoolbox_application::language::LanguageLearningService::new(
            Arc::clone(&content),
            Arc::clone(&learning_os),
        ),
    );
    let dictionary = Arc::new(devtoolbox_application::language::LanguageService::new(
        Arc::clone(&content),
    ));
    let course: Arc<devtoolbox_application::language::course::CourseService> = Arc::new(
        devtoolbox_application::language::course::CourseService::new(
            Arc::clone(&core.course_store)
                as Arc<dyn devtoolbox_application::language::course::CourseStorePort>,
            Arc::clone(&core.learning),
        ),
    );
    let knowledge = Arc::clone(&core.knowledge);
    let geography: Arc<devtoolbox_application::geography::GeographyService> = Arc::new(
        devtoolbox_application::geography::GeographyService::new(Arc::clone(&core.geography_port)),
    );
    // router 需要具体的 NewsService（History 侧同理）：AppCore 保留一份。
    let news = Arc::clone(&core.news_service);
    let news_ingest = Arc::clone(&core.news_ingest);
    let settings: Arc<dyn ai_api::SettingsAccess> = Arc::new(ai_api::FileSettingsAccess::new(
        std::path::Path::new(&config.data_dir),
    ));
    let travel = Arc::new(travel_api::TravelDeps {
        client: core.client.clone(),
        store: Arc::clone(&core.travel_store),
        registry: Arc::clone(&core.travel_registry),
        settings_loader: Arc::clone(&core.settings_loader),
    });

    let app = routes::router(
        Arc::new(service),
        Arc::clone(&learning),
        dictionary,
        content,
        course,
        config.data_dir.clone(),
        knowledge,
        Arc::clone(&language_store),
        travel,
        Arc::new(language_data_api::DataPaths {
            config_dir: config.data_dir.clone(),
        }),
        Arc::default(),
        settings,
        learning_os,
        geography,
        news,
        news_ingest,
    );
    info!(
        bind = %config.bind,
        "HTTP server listening (history + language; no auth; no CORS)"
    );
    match axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        Ok(()) => {
            info!("HTTP server stopped cleanly");
            ExitCode::SUCCESS
        }
        Err(error) => {
            error!("HTTP server error: {error}");
            ExitCode::from(1)
        }
    }
}
