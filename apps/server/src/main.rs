//! Self Tools 极简 HTTP 服务（Gate 9 试点）：只读 History 知识库。
//!
//! 本二进制只做：配置（env + CLI）→ 日志 → 组合根（只读 DuckDB 仓库 +
//! 用例服务）→ 路由（`routes`）→ 监听与优雅退出。不包含鉴权、CORS、写入端点。

mod history_query;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use devtoolbox_application::history::HistoryService;
use devtoolbox_infrastructure::HistoryDuckDbRepository;

/// 默认只监听本机；要暴露到局域网时显式设置 `SELF_TOOLS_BIND`（Gate 9：默认 127.0.0.1）。
const DEFAULT_BIND: &str = "127.0.0.1:8080";
/// 唯一事实源（Gate 5.5）：`history-data-pipeline/dist/history.duckdb`。
const DEFAULT_HISTORY_DB: &str = "history-data-pipeline/dist/history.duckdb";
const ENV_BIND: &str = "SELF_TOOLS_BIND";
const ENV_HISTORY_DB: &str = "SELF_TOOLS_HISTORY_DB";

#[derive(Debug)]
struct Config {
    bind: SocketAddr,
    history_db: PathBuf,
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

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                bind = args.next().ok_or("--bind requires a value")?;
            }
            "--history-db" => {
                history_db = args.next().ok_or("--history-db requires a value")?;
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

    // Gate 9：duckdb 路径可配置且**缺失即启动失败**，绝不静默降级。
    let repository = match HistoryDuckDbRepository::open(&config.history_db) {
        Ok(repository) => repository,
        Err(error) => {
            error!(
                "history database unavailable: {error} \
                 (set {ENV_HISTORY_DB} or pass --history-db)",
            );
            return ExitCode::from(1);
        }
    };
    info!(
        db = %config.history_db.display(),
        "history knowledge base ready (read-only)"
    );

    let listener = match tokio::net::TcpListener::bind(config.bind).await {
        Ok(listener) => listener,
        Err(error) => {
            error!("cannot bind {}: {error}", config.bind);
            return ExitCode::from(1);
        }
    };

    let service = HistoryService::new(Box::new(history_query::HistoryQueryAdapter::new(
        Arc::new(repository),
    )));
    let app = routes::router(Arc::new(service));
    info!(
        bind = %config.bind,
        "HTTP server listening (history read-only; no auth; no CORS)"
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