//! Runtime gate：MCP 必须访问与 PersonalAgent 相同的业务数据（V9 §11/§129）。
//!
//! 用真实 SQLite store 装配 MCP 组合根，经 `McpService` 调用知识工具，
//! 断言读到同一份数据（没有 `mcp_*.db` 第二份业务库）。

#![allow(unused_crate_dependencies)]

use devtoolbox_application::personal_ai::registry::ToolRegistry;
use devtoolbox_application::server::ports::SystemMetricsProvider;
use devtoolbox_core::memory::{MemoryCategory, MemoryDraft};
use devtoolbox_core::mcp::{McpCredential, McpTrustLevel};
use devtoolbox_infrastructure::MemorySqliteStore;

use devtoolbox_mcp::compose::{BuildOptions, build};
use devtoolbox_mcp::testing::TestHarness;

/// 经**真实 store** 写入一条记忆（与 desktop 相同的实现）。
fn seed_memory(dir: &std::path::Path, content: &str) -> String {
    let store = MemorySqliteStore::open(dir.join("memory.db")).expect("open memory store");
    let service = devtoolbox_application::memory::MemoryService::new(std::sync::Arc::new(
        devtoolbox_mcp::compose::MemoryStoreAdapter::new(store),
    ));
    let item = service
        .save_confirmed(MemoryDraft::new(MemoryCategory::Environment, content))
        .expect("seed");
    item.id
}

#[tokio::test]
async fn mcp_reads_the_same_memory_store_as_the_agent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _ = seed_memory(dir.path(), "Docker 数据目录是 /Volumes/Data/docker");

    // MCP 组合根：真实 store 装配。
    let composition = build(BuildOptions {
        stores_dir: Some(dir.path().to_path_buf()),
    })
    .expect("compose");
    let names = composition.tool_names();
    assert!(
        names.iter().any(|name| name == "memory.search"),
        "真实装配后 memory 工具必须在 catalog 中: {names:?}"
    );

    // 经 MCP service 调用 memory.search：必须读到种子数据（§129：不能 silent empty）。
    let service = composition.service();
    let principal = devtoolbox_core::mcp::McpPrincipal::local("runtime-gate");
    let mut principal = principal;
    principal.scopes = vec![devtoolbox_core::mcp::McpScope::parse("selftools.read").expect("scope")];
    let invocation = service
        .call_tool(
            &principal,
            "memory.search",
            serde_json::json!({"query": "docker"}),
        )
        .await
        .expect("call");
    assert!(!invocation.is_error, "{:?}", invocation.payload);
    let text = invocation.payload.to_string();
    assert!(text.contains("/Volumes/Data/docker"), "必须读到真实 store 数据: {text}");
    assert!(!text.contains("没有找到"), "不得静默空结果");
}

#[tokio::test]
async fn fail_closed_without_stores_dir() {
    // §10/§12：未指定 stores_dir → 空能力集（不得悄悄建第二份库）。
    let dir = tempfile::tempdir().expect("tempdir");
    let composition = build(BuildOptions {
        stores_dir: Some(dir.path().to_path_buf()),
    });
    assert!(composition.is_ok(), "空目录可以装配真实 store");
    let empty = build(BuildOptions::default()).expect("empty compose");
    assert!(empty.tool_names().is_empty(), "无 stores_dir → 无工具");
}

#[test]
fn memory_store_adapter_shares_one_database() {
    // §10：MCP 与业务侧共用同一文件（不存在 mcp_memory.db）。
    let dir = tempfile::tempdir().expect("tempdir");
    let id = seed_memory(dir.path(), "家中服务器是 macOS");
    assert!(dir.path().join("memory.db").is_file());
    assert!(
        !dir.path().join("mcp_memory.db").exists(),
        "不得创建第二份业务库"
    );

    // 再经同一个 store 端口读：证明适配器与种子写入方共享同一份数据。
    let store = MemorySqliteStore::open(dir.path().join("memory.db")).expect("open");
    let service = devtoolbox_application::memory::MemoryService::new(std::sync::Arc::new(
        devtoolbox_mcp::compose::MemoryStoreAdapter::new(store),
    ));
    let found = service.get(&id, true).expect("get");
    assert!(found.content.contains("macOS"), "同一 store 同一数据: {found:?}");
    let _ = TestHarness::new();
}

#[test]
fn metrics_probe_reports_unknown_rather_than_lying() {
    // MCP 侧不采样系统指标：Unknown 而不是假数据。
    let metrics = devtoolbox_mcp::compose::SystemMetricsProbe
        .metrics()
        .expect("metrics");
    assert!(metrics.is_unknown(), "{metrics:?}");
    let _ = ToolRegistry::new();
    let _ = (McpTrustLevel::LocalTrusted, McpCredential::None);
}
