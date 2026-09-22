//! `self-tools mcp` —— MCP transport 入口（V8 §24）。
//!
//! 支持两种 transport（§5 Local != Remote）：
//! - `--stdio`（默认）：本地 MCP client（Pi / Claude Desktop / 其它）经 STDIO 连接；
//! - `--http`：Streamable HTTP，默认只绑 loopback（§44）。
//!
//! 日志**只**写 stderr（§25：stdout 保留协议输出）。
//! 装配（ToolRegistry / identity / SafeAction）由组合根完成 —— 本文件只解析
//! 参数并启动传输，不含任何工具语义。

use std::net::SocketAddr;

// 这些依赖由 lib target 使用；显式引用让 binary target 的
// workspace 级 `unused-crate-dependencies` lint 保持有效（desktop 同模式）。
use async_trait as _;
use devtoolbox_application as _;
use devtoolbox_core as _;
use devtoolbox_infrastructure as _;
use serde as _;
use serde_json as _;

// 这些依赖由 lib target 使用；显式引用让 binary target 的
// `unused_crate_dependencies` lint 保持满意（与 desktop 的 main.rs 同模式）。
use devtoolbox_core as _;
use serde as _;
use serde_json as _;

use devtoolbox_mcp::compose::Composition;

/// 命令行参数（显式 > 环境变量 > 默认；未知参数 → 用法错误）。
#[derive(Debug)]
struct Cli {
    transport: Transport,
    bind: String,
    remote_enabled: bool,
    /// 业务数据目录（真实 store 装配；None = fail-closed 空能力集）。
    stores_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Stdio,
    Http,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            transport: Transport::Stdio,
            bind: "127.0.0.1:8787".to_string(),
            remote_enabled: false,
            stores_dir: None,
        }
    }
}

fn parse_args() -> Result<Cli, String> {
    let mut cli = Cli::default();
    if let Ok(bind) = std::env::var("SELF_TOOLS_MCP_BIND")
        && !bind.trim().is_empty()
    {
        cli.bind = bind.trim().to_string();
    }
    if std::env::var("SELF_TOOLS_MCP_REMOTE").as_deref() == Ok("1") {
        cli.remote_enabled = true;
    }
    if let Ok(dir) = std::env::var("SELF_TOOLS_MCP_STORES")
        && !dir.trim().is_empty()
    {
        cli.stores_dir = Some(std::path::PathBuf::from(dir.trim()));
    }
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stdio" => cli.transport = Transport::Stdio,
            "--http" => cli.transport = Transport::Http,
            "--bind" => {
                cli.bind = args
                    .next()
                    .ok_or("--bind requires a value")?
                    .trim()
                    .to_string();
            }
            "--remote" => cli.remote_enabled = true,
            "--stores" => {
                cli.stores_dir = args
                    .next()
                    .map(|value| std::path::PathBuf::from(value.trim()))
                    .filter(|path| !path.as_os_str().is_empty());
            }
            "--help" | "-h" => {
                println!(
                    "usage: self-tools mcp [--stdio|--http] [--bind ADDR] [--remote]\n\n\
                     transports:\n  \
                     --stdio   local MCP client over STDIO (default; stdout = protocol only)\n  \
                     --http    streamable HTTP (default bind 127.0.0.1:8787)\n\n\
                     safety:\n  \
                     --remote  allow non-loopback bind; requires a configured identity\n  \
                     provider, otherwise startup fails (V8 §46)\n  \
                     --stores DIR  wire the production stores (memory/documents/files/audit)\n  \
                     from DIR; without it MCP runs fail-closed with an empty catalog\n"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(cli)
}

fn main() -> std::process::ExitCode {
    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("mcp: {message}");
            eprintln!("mcp: try `self-tools mcp --help`");
            return std::process::ExitCode::from(2);
        }
    };

    let composition = match devtoolbox_mcp::compose::build(devtoolbox_mcp::compose::BuildOptions {
        stores_dir: cli.stores_dir.clone(),
    }) {
        Ok(composition) => composition,
        Err(message) => {
            eprintln!("mcp: composition failed: {message}");
            return std::process::ExitCode::from(1);
        }
    };

    match cli.transport {
        Transport::Stdio => run_stdio(composition),
        Transport::Http => match run_http(composition, &cli) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("mcp: {message}");
                std::process::ExitCode::from(1)
            }
        },
    }
}

/// STDIO：stdout 只走协议（§25）。
fn run_stdio(composition: Composition) -> std::process::ExitCode {
    let server = devtoolbox_mcp::stdio::StdioServer::new(composition.service());
    // §103/§67：STDIO 会话 id 含进程 id（同机多个 client 不共享 session）。
    let principal = devtoolbox_mcp::stdio::local_principal(&format!(
        "local-stdio-{}",
        std::process::id()
    ));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match server.serve(stdin.lock(), &mut out, &principal) {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // 协议错误只能去 stderr。
            eprintln!("mcp: stdio transport error: {error}");
            std::process::ExitCode::from(1)
        }
    }
}

/// HTTP：启动门禁（§46：无 auth 不能远程绑定）。
fn run_http(composition: Composition, cli: &Cli) -> Result<(), String> {
    let config = devtoolbox_mcp::http::HttpTransportConfig {
        bind: cli.bind.clone(),
        remote_enabled: cli.remote_enabled,
        max_body_bytes: 1024 * 1_024,
    };
    config.validate_startup(composition.identity_configured())?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("runtime build failed: {error}"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&config.bind)
            .await
            .map_err(|error| format!("bind {} failed: {error}", config.bind))?;
        eprintln!("mcp: listening on http://{}/mcp", config.bind);
        if config.bind.starts_with("127.0.0.1") || config.bind.starts_with("localhost") {
            eprintln!("mcp: loopback only; use --remote with a configured identity provider for LAN");
        }
        let state = devtoolbox_mcp::http::HttpState::new(
            composition.service(),
            true,
            config.max_body_bytes,
        );
        axum::serve(
            listener,
            devtoolbox_mcp::http::router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
            .await
            .map_err(|error| format!("serve failed: {error}"))
    })
}

// 测试 target 使用 dev-dependencies；显式引用让 bin target 的 lint 满意。
#[cfg(test)]
use async_trait as _;
#[cfg(test)]
use tower as _;

// lib target 的单元测试使用 tempfile；bin test target 编译它时同样需要引用。
#[cfg(test)]
use tempfile as _;
