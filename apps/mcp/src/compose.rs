//! MCP 组合根（V8：把 ToolRegistry + identity + audit + SafeAction 装配成
//! 可运行的 `McpService`）。
//!
//! 当前装配范围（Phase 1，V8 §152 的「Local MCP only」最小集）：
//! - ToolRegistry 从环境变量声明的域装配（默认空 = fail-closed）；
//! - identity = `DenyAllIdentityProvider`（远程一律拒绝），loopback 由
//!   transport 层给 LOCAL_TRUSTED；
//! - SafeAction = 空注册表（无已注册服务 → 任何 restart 请求被拒）。
//!
//! 完整装配（真实 memory/files/server stores）属于后续 Gate：在能证明
//! 安全之前，宁可是「能力少」而不是「能力错」（§152）。

use std::sync::Arc;

use devtoolbox_application::mcp::adapter::McpToolAdapter;
use devtoolbox_application::mcp::auth::{DenyAllIdentityProvider, RemoteIdentityProvider};
use devtoolbox_application::mcp::service::{McpAuditPort, McpService, McpServiceConfig};
use devtoolbox_application::personal_ai::registry::{ToolExecutor, ToolRegistry};
use devtoolbox_application::server::action::{
    InMemoryActionAudit, InMemoryConfirmationStore, SafeActionConfig, SafeActionService,
    ServiceControlPort,
};
use devtoolbox_application::server::ports::ServiceProbePort;
use devtoolbox_application::server::registry::ServiceRegistryService;
use devtoolbox_core::mcp::McpAuditEntry;
use devtoolbox_core::server::{HealthStatus, ServiceDescriptor, ServiceStatus};
use std::sync::Mutex;

/// 内存审计（进程内；重启即丢——不引外部日志栈，V8 §111）。
#[derive(Default)]
pub struct MemoryAuditStore {
    entries: Mutex<Vec<McpAuditEntry>>,
}

impl McpAuditPort for MemoryAuditStore {
    fn record(&self, entry: &McpAuditEntry) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        entries.push(entry.clone());
    }
}

/// 空探活（未装配真实平台时的 fail-closed 状态）。
#[derive(Debug, Default)]
pub struct UnknownProbe;

impl ServiceProbePort for UnknownProbe {
    fn probe(&self, service: &ServiceDescriptor) -> ServiceStatus {
        ServiceStatus {
            service_id: service.id.clone(),
            status: HealthStatus::Unknown,
            detail: "平台探活未装配".into(),
            checked_at: 0,
        }
    }
}

/// 拒绝所有重启（未装配真实平台控制时的 fail-closed）。
#[derive(Debug, Default)]
pub struct DenyAllControl;

impl ServiceControlPort for DenyAllControl {
    fn restart(&self, _service_id: &str) -> Result<(), String> {
        Err("service_control_not_configured".into())
    }
}

/// 装配结果。
pub struct Composition {
    registry: Arc<ToolRegistry>,
    identity: Arc<dyn RemoteIdentityProvider>,
    audit: Arc<MemoryAuditStore>,
    actions: Arc<SafeActionService>,
}

impl Composition {
    pub fn service(&self) -> Arc<McpService> {
        Arc::new(
            McpService::new(
                Arc::new(McpToolAdapter::new(Arc::clone(&self.registry))),
                Arc::clone(&self.identity),
                Arc::clone(&self.audit) as Arc<dyn McpAuditPort>,
                McpServiceConfig::default(),
            )
            .with_system_actions(Arc::clone(&self.actions)),
        )
    }

    /// 是否配置了远程身份提供者（启动门禁用，§46）。
    pub fn identity_configured(&self) -> bool {
        // DenyAll 恒 false：远程绑定会被 startup gate 拒绝。
        // 接入真实 OAuth/OIDC provider 后改为读取其配置状态。
        false
    }
}

/// 装配选项。
#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// 业务数据目录（真实 store 装配；None = fail-closed 空能力集）。
    pub stores_dir: Option<std::path::PathBuf>,
    /// 允许指向已含业务库的目录（默认 false：防多进程共开，§G1）。
    pub allow_existing: bool,
}

/// 装配。
///
/// - `stores_dir = None`（默认）：fail-closed 空能力集（Phase 1 行为）；
/// - `stores_dir = Some(dir)`：装配**与 desktop 相同的** repository 抽象
///   （§10：不建第二份 DB），MCP 与 PersonalAgent 访问同一业务数据（§11）。
pub fn build(options: BuildOptions) -> Result<Composition, String> {
    build_impl(options)
}

/// 兼容入口：等价于 `build(BuildOptions::default())`。
pub fn build_default() -> Result<Composition, String> {
    build(BuildOptions {
        stores_dir: None,
        allow_existing: false,
    })
}

fn build_impl(options: BuildOptions) -> Result<Composition, String> {
    build_with(options, Vec::new())
}

/// 装配 + 额外注册的 agent 风格工具（供 Runtime gate 测试注入探针工具）。
pub fn build_with(
    options: BuildOptions,
    extra_tools: Vec<Arc<dyn ToolExecutor>>,
) -> Result<Composition, String> {
    let _ = &extra_tools;
    build_inner(options, extra_tools)
}

fn build_inner(
    options: BuildOptions,
    extra_tools: Vec<Arc<dyn ToolExecutor>>,
) -> Result<Composition, String> {
    let Some(dir) = options.stores_dir.clone() else {
        return build_empty(extra_tools);
    };
    let dir = validate_stores_dir(&dir, options.allow_existing)?;
    std::fs::create_dir_all(&dir).map_err(|error| format!("create stores dir: {error}"))?;
    build_stores(&dir, extra_tools)
}

/// `--stores` 目录校验（V9-G1：并发 / 数据完整性）。
///
/// MCP 进程与 desktop 进程共开同一 SQLite 会立即 `SQLITE_BUSY`（rusqlite 未设
/// busy_timeout、默认 rollback journal），因此**拒绝**指向桌面配置目录：
/// MCP 要么用独立目录，要么不装配真实 store（fail-closed 空能力集）。
fn validate_stores_dir(
    dir: &std::path::Path,
    options_allow_existing: bool,
) -> Result<std::path::PathBuf, String> {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    // 判定：目标目录**已经含有业务库** → 视为桌面在用目录并拒绝（§G1）。
    // 显式豁免仅给测试 / 一次性迁移：`SELF_TOOLS_MCP_ALLOW_EXISTING=1`。
    if !options_allow_existing {
        for db in ["memory.db", "documents.db", "files.db", "server_actions.db"] {
            if canonical.join(db).is_file() {
                return Err(format!(
                    "--stores 目录 {} 已包含业务库 {db}（疑似桌面在用目录）：MCP 与桌面共开同一 SQLite 会 SQLITE_BUSY，请改用独立目录（或用 SELF_TOOLS_MCP_ALLOW_EXISTING=1 显式豁免）",
                    canonical.display()
                ));
            }
        }
    }
    Ok(canonical)
}

fn build_empty(extra_tools: Vec<Arc<dyn ToolExecutor>>) -> Result<Composition, String> {
    let mut registry = ToolRegistry::new();
    for tool in extra_tools {
        registry.register(tool).map_err(|error| error.to_string())?;
    }
    Ok(Composition::from_parts(
        Arc::new(registry),
        Arc::new(DenyAllIdentityProvider),
        Arc::new(MemoryAuditStore::default()),
        empty_actions(),
    ))
}

fn build_stores(
    dir: &std::path::Path,
    extra_tools: Vec<Arc<dyn ToolExecutor>>,
) -> Result<Composition, String> {
    use devtoolbox_application::documents::DocumentService;
    use devtoolbox_application::files::FileService;
    use devtoolbox_application::knowledge::{
        DocumentRetriever, FileRetriever, KnowledgeRetrievalService, MemoryRetriever,
    };
    use devtoolbox_application::memory::MemoryService;
    use devtoolbox_application::personal_ai::registry::ModuleRegistry;
    use devtoolbox_application::server::registry::ServiceRegistryService;
    use devtoolbox_application::server::service::ServerService;
    use devtoolbox_infrastructure::{
        DocumentIndexSqliteStore, FileIndexSqliteStore, MemorySqliteStore, ServerActionAuditSqlite,
    };

    let mut registry = ToolRegistry::new();
    let mut modules = ModuleRegistry::new();

    // 三个知识域：复用 desktop 相同的 SQLite 存储（§10）。
    // 每个域的 store 都需要适配到 application 端口（与 desktop 的
    // `knowledge.rs` 同形态；这里内联为 MCP 侧薄适配器）。
    let memory = Arc::new(MemoryService::new(Arc::new(MemoryStoreAdapter::new(
        MemorySqliteStore::open(dir.join("memory.db")).map_err(|error| error.to_string())?,
    ))));
    let documents = Arc::new(DocumentService::new(
        Arc::new(DocumentIndexAdapter::new(
            DocumentIndexSqliteStore::open(dir.join("documents.db"))
                .map_err(|error| error.to_string())?,
        )),
        Arc::new(DocumentSourcePortProbe),
    ));
    let files = Arc::new(FileService::new(
        Arc::new(FileSystemPortProbe),
        Arc::new(FileIndexAdapter::new(
            FileIndexSqliteStore::open(dir.join("files.db")).map_err(|error| error.to_string())?,
        )),
    ));
    devtoolbox_application::personal_ai::register_memory(
        &mut modules,
        &mut registry,
        Arc::clone(&memory),
    )
    .map_err(|error| error.to_string())?;
    devtoolbox_application::personal_ai::register_documents(
        &mut modules,
        &mut registry,
        Arc::clone(&documents),
        Arc::new(devtoolbox_core::settings::KnowledgeSettings::default)
            as Arc<dyn Fn() -> devtoolbox_core::settings::KnowledgeSettings + Send + Sync>,
    )
    .map_err(|error| error.to_string())?;
    devtoolbox_application::personal_ai::register_files(
        &mut modules,
        &mut registry,
        Arc::clone(&files),
        Arc::new(devtoolbox_core::settings::KnowledgeSettings::default)
            as Arc<dyn Fn() -> devtoolbox_core::settings::KnowledgeSettings + Send + Sync>,
    )
    .map_err(|error| error.to_string())?;
    // knowledge facade（三源检索）。
    let retrieval = Arc::new(KnowledgeRetrievalService::new(
        vec![
            Arc::new(MemoryRetriever::new(Arc::clone(&memory))),
            Arc::new(DocumentRetriever::new(Arc::clone(&documents))),
            Arc::new(FileRetriever::new(
                Arc::clone(&files),
                Arc::new(devtoolbox_core::settings::KnowledgeSettings::default)
                    as Arc<dyn Fn() -> devtoolbox_core::settings::KnowledgeSettings + Send + Sync>,
            )),
        ],
        devtoolbox_core::knowledge::KnowledgeBudget::default(),
    ));
    devtoolbox_application::personal_ai::register_knowledge(
        &mut modules,
        &mut registry,
        Arc::clone(&retrieval),
    )
    .map_err(|error| error.to_string())?;

    // server 域：真实审计 + 空注册表（服务注册由 settings 提供，MCP 侧默认空）。
    let audit = Arc::new(
        ServerActionAuditSqlite::open_store(dir.join("server_actions.db"))
            .map_err(|error| error.to_string())?,
    );
    let services = Arc::new(ServiceRegistryService::new(
        Vec::new(),
        Arc::new(UnknownProbe),
    ));
    let app_registry = Arc::new(
        devtoolbox_application::server::registry::ApplicationRegistryService::new(
            Vec::new(),
            Arc::new(ApplicationRegistryProbe),
        ),
    );
    let server_service = Arc::new(ServerService::new(
        Arc::new(SystemMetricsProbe),
        Arc::clone(&services),
        Arc::clone(&app_registry),
        devtoolbox_application::server::service::ServerConfig::default(),
    ));
    devtoolbox_application::personal_ai::register_server(
        &mut modules,
        &mut registry,
        Arc::clone(&server_service),
        Arc::clone(&services),
        app_registry,
        empty_actions(),
        Arc::new(LogTailProbe),
        devtoolbox_core::server::SessionTrust::LocalDesktop,
    )
    .map_err(|error| error.to_string())?;

    for tool in extra_tools {
        registry.register(tool).map_err(|error| error.to_string())?;
    }

    let audit_bridge = Arc::new(SqliteAuditBridge::new(audit));
    let actions = Arc::new(SafeActionService::new(
        Arc::clone(&services),
        Arc::new(DenyAllControl),
        Arc::new(InMemoryConfirmationStore::default()),
        audit_bridge as Arc<dyn devtoolbox_application::server::ActionAuditPort>,
        Arc::new(devtoolbox_core::server::DefaultActionRiskPolicy),
        SafeActionConfig::default(),
    ));

    Ok(Composition::from_parts(
        Arc::new(registry),
        Arc::new(DenyAllIdentityProvider),
        Arc::new(MemoryAuditStore::default()),
        actions,
    ))
}

fn empty_actions() -> Arc<SafeActionService> {
    let services = Arc::new(ServiceRegistryService::new(
        Vec::new(),
        Arc::new(UnknownProbe),
    ));
    Arc::new(SafeActionService::new(
        services,
        Arc::new(DenyAllControl),
        Arc::new(InMemoryConfirmationStore::default()),
        Arc::new(InMemoryActionAudit::default()),
        Arc::new(devtoolbox_core::server::DefaultActionRiskPolicy),
        SafeActionConfig::default(),
    ))
}

impl Composition {
    fn from_parts(
        registry: Arc<ToolRegistry>,
        identity: Arc<dyn RemoteIdentityProvider>,
        audit: Arc<MemoryAuditStore>,
        actions: Arc<SafeActionService>,
    ) -> Self {
        Self {
            registry,
            identity,
            audit,
            actions,
        }
    }

    /// 工具数量（Runtime gate：证明 MCP catalog 来自真实注册）。
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        self.registry
            .specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Store → Port 适配器（MCP 组合根内联；与 desktop `knowledge.rs` 同形态）
// ---------------------------------------------------------------------------

/// `MemorySqliteStore` → `MemoryStorePort`。
pub struct MemoryStoreAdapter {
    store: devtoolbox_infrastructure::MemorySqliteStore,
}

impl MemoryStoreAdapter {
    #[must_use]
    pub fn new(store: devtoolbox_infrastructure::MemorySqliteStore) -> Self {
        Self { store }
    }
}

impl devtoolbox_application::memory::MemoryStorePort for MemoryStoreAdapter {
    fn upsert(
        &self,
        item: &devtoolbox_core::memory::MemoryItem,
    ) -> Result<(), devtoolbox_application::memory::MemoryStoreError> {
        self.store
            .upsert(item)
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
    fn get(
        &self,
        id: &str,
    ) -> Result<
        Option<devtoolbox_core::memory::MemoryItem>,
        devtoolbox_application::memory::MemoryStoreError,
    > {
        self.store
            .get(id)
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
    fn query(
        &self,
        spec: &devtoolbox_core::memory::MemoryQuery,
    ) -> Result<
        Vec<devtoolbox_core::memory::MemoryItem>,
        devtoolbox_application::memory::MemoryStoreError,
    > {
        self.store
            .query(spec)
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
    fn touch_used(
        &self,
        ids: &[String],
        now: i64,
    ) -> Result<(), devtoolbox_application::memory::MemoryStoreError> {
        self.store
            .touch_used(ids, now)
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
    fn count_by_status(
        &self,
    ) -> Result<
        Vec<(devtoolbox_core::memory::MemoryStatus, usize)>,
        devtoolbox_application::memory::MemoryStoreError,
    > {
        self.store
            .count_by_status()
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
    fn count_by_category(
        &self,
    ) -> Result<
        Vec<(devtoolbox_core::memory::MemoryCategory, usize)>,
        devtoolbox_application::memory::MemoryStoreError,
    > {
        self.store
            .count_by_category()
            .map_err(|error| devtoolbox_application::memory::MemoryStoreError(error.to_string()))
    }
}

/// `DocumentIndexSqliteStore` → `DocumentIndexPort`。
pub struct DocumentIndexAdapter {
    store: devtoolbox_infrastructure::DocumentIndexSqliteStore,
}

impl DocumentIndexAdapter {
    #[must_use]
    pub fn new(store: devtoolbox_infrastructure::DocumentIndexSqliteStore) -> Self {
        Self { store }
    }
}

impl devtoolbox_application::documents::DocumentIndexPort for DocumentIndexAdapter {
    fn upsert(
        &self,
        meta: &devtoolbox_core::documents::DocumentMeta,
    ) -> Result<(), devtoolbox_application::documents::DocumentStoreError> {
        self.store.upsert(meta).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn replace_chunks(
        &self,
        document_id: &str,
        chunks: &[devtoolbox_core::documents::DocumentChunk],
    ) -> Result<(), devtoolbox_application::documents::DocumentStoreError> {
        self.store
            .replace_chunks(document_id, chunks)
            .map_err(|error| {
                devtoolbox_application::documents::DocumentStoreError(error.to_string())
            })
    }
    fn get(
        &self,
        document_id: &str,
    ) -> Result<
        Option<devtoolbox_core::documents::DocumentMeta>,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store.get(document_id).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn chunks(
        &self,
        document_id: &str,
    ) -> Result<
        Vec<devtoolbox_core::documents::DocumentChunk>,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store.chunks(document_id).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn search_candidates(
        &self,
        keywords: &[String],
        document_type: Option<devtoolbox_core::documents::DocumentType>,
        limit: usize,
    ) -> Result<
        Vec<devtoolbox_core::documents::DocumentHit>,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store
            .search_candidates(keywords, document_type, limit)
            .map_err(|error| {
                devtoolbox_application::documents::DocumentStoreError(error.to_string())
            })
    }
    fn recent(
        &self,
        limit: usize,
    ) -> Result<
        Vec<devtoolbox_core::documents::DocumentMeta>,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store.recent(limit).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn fingerprints(
        &self,
        root_id: &str,
    ) -> Result<
        Vec<devtoolbox_core::documents::DocumentFingerprint>,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store.fingerprints(root_id).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn remove(
        &self,
        document_id: &str,
    ) -> Result<(), devtoolbox_application::documents::DocumentStoreError> {
        self.store.remove(document_id).map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
    fn stats(
        &self,
    ) -> Result<
        devtoolbox_core::documents::DocumentIndexStats,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        self.store.stats().map_err(|error| {
            devtoolbox_application::documents::DocumentStoreError(error.to_string())
        })
    }
}

/// `FileIndexSqliteStore` → `FileIndexPort`。
pub struct FileIndexAdapter {
    store: devtoolbox_infrastructure::FileIndexSqliteStore,
}

impl FileIndexAdapter {
    #[must_use]
    pub fn new(store: devtoolbox_infrastructure::FileIndexSqliteStore) -> Self {
        Self { store }
    }
}

impl devtoolbox_application::files::FileIndexPort for FileIndexAdapter {
    fn upsert_many(
        &self,
        entries: &[devtoolbox_core::files::FileMetadata],
    ) -> Result<(), devtoolbox_application::files::FileIndexError> {
        self.store
            .upsert_many(entries)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn get(
        &self,
        file_id: &str,
    ) -> Result<
        Option<devtoolbox_core::files::FileMetadata>,
        devtoolbox_application::files::FileIndexError,
    > {
        self.store
            .get(file_id)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn find_by_path(
        &self,
        path: &str,
    ) -> Result<
        Option<devtoolbox_core::files::FileMetadata>,
        devtoolbox_application::files::FileIndexError,
    > {
        self.store
            .find_by_path(path)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn search(
        &self,
        spec: &devtoolbox_core::files::FileQuery,
    ) -> Result<
        Vec<devtoolbox_core::files::FileMetadata>,
        devtoolbox_application::files::FileIndexError,
    > {
        self.store
            .search(spec)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn recent(
        &self,
        limit: usize,
    ) -> Result<
        Vec<devtoolbox_core::files::FileMetadata>,
        devtoolbox_application::files::FileIndexError,
    > {
        self.store
            .recent(limit)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn fingerprints(
        &self,
        root_id: &str,
    ) -> Result<
        Vec<devtoolbox_core::files::FileFingerprint>,
        devtoolbox_application::files::FileIndexError,
    > {
        self.store
            .fingerprints(root_id)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn remove(&self, file_id: &str) -> Result<(), devtoolbox_application::files::FileIndexError> {
        self.store
            .remove(file_id)
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
    fn stats(
        &self,
    ) -> Result<devtoolbox_core::files::FileIndexStats, devtoolbox_application::files::FileIndexError>
    {
        self.store
            .stats()
            .map_err(|error| devtoolbox_application::files::FileIndexError(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// MCP 侧探活 / 源端口（fail-closed 占位：MCP 不触碰真实文件系统与 launchd）
// ---------------------------------------------------------------------------

/// 文档来源：不扫描（MCP 侧无扫描触发；索引由桌面端完成）。
#[derive(Debug, Default)]
pub struct DocumentSourcePortProbe;

impl devtoolbox_application::documents::DocumentSourcePort for DocumentSourcePortProbe {
    fn scan_root(
        &self,
        _root: &devtoolbox_core::files::KnowledgeRoot,
        _limit: usize,
    ) -> Result<
        (Vec<devtoolbox_core::documents::ScannedDocument>, bool),
        devtoolbox_application::documents::DocumentStoreError,
    > {
        Ok((Vec::new(), false))
    }

    fn extract(
        &self,
        _path: &std::path::Path,
        _document_type: devtoolbox_core::documents::DocumentType,
        _max_bytes: u64,
    ) -> Result<
        devtoolbox_core::documents::ExtractedContent,
        devtoolbox_application::documents::DocumentStoreError,
    > {
        Err(devtoolbox_application::documents::DocumentStoreError(
            "mcp: document extraction not available".into(),
        ))
    }
}

/// 文件系统：全部拒绝（MCP 侧不提供文件系统访问；只检索索引）。
#[derive(Debug, Default)]
pub struct FileSystemPortProbe;

impl devtoolbox_application::files::FileSystemPort for FileSystemPortProbe {
    fn canonicalize(
        &self,
        _path: &str,
    ) -> Result<std::path::PathBuf, devtoolbox_core::files::FileAccessDenied> {
        Err(devtoolbox_core::files::FileAccessDenied::NotFound)
    }
    fn metadata(
        &self,
        _path: &std::path::PathBuf,
    ) -> Result<devtoolbox_core::files::RawFile, devtoolbox_core::files::FileAccessDenied> {
        Err(devtoolbox_core::files::FileAccessDenied::NotFound)
    }
    fn read_text(
        &self,
        _path: &std::path::PathBuf,
        _max_bytes: u64,
    ) -> Result<devtoolbox_core::files::FileReadOutcome, devtoolbox_core::files::FileAccessDenied>
    {
        Err(devtoolbox_core::files::FileAccessDenied::NotFound)
    }
    fn walk(
        &self,
        _root: &devtoolbox_core::files::KnowledgeRoot,
        _limit: usize,
    ) -> Result<
        (Vec<devtoolbox_core::files::RawFile>, bool),
        devtoolbox_core::files::FileAccessDenied,
    > {
        Ok((Vec::new(), false))
    }
}

/// 系统指标：MCP 侧不采样（返回 Unknown）。
#[derive(Debug, Default)]
pub struct SystemMetricsProbe;

impl devtoolbox_application::server::ports::SystemMetricsProvider for SystemMetricsProbe {
    fn metrics(&self) -> Result<devtoolbox_core::server::SystemMetrics, String> {
        Ok(devtoolbox_core::server::SystemMetrics::default())
    }
}

/// 日志尾读：MCP 侧不读文件。
#[derive(Debug, Default)]
pub struct LogTailProbe;

impl devtoolbox_application::server::ports::LogTailPort for LogTailProbe {
    fn tail(
        &self,
        _service: &devtoolbox_core::server::ServiceDescriptor,
        _log_source_id: &str,
        _max_lines: usize,
        _max_bytes: usize,
        _max_age_secs: u64,
    ) -> Result<devtoolbox_core::server::LogReadResult, String> {
        Err("mcp: log access not available".into())
    }
}

/// 应用注册表探活：HTTP 探活由桌面端负责。
#[derive(Debug, Default)]
pub struct ApplicationRegistryProbe;

impl devtoolbox_application::server::ports::ApplicationProbePort for ApplicationRegistryProbe {
    fn probe(
        &self,
        app: &devtoolbox_core::server::ApplicationDescriptor,
    ) -> devtoolbox_core::server::ApplicationStatus {
        devtoolbox_core::server::ApplicationStatus {
            app_id: app.id.clone(),
            status: devtoolbox_core::server::HealthStatus::Unknown,
            detail: "mcp: probe not available".into(),
            checked_at: 0,
        }
    }
}

/// 审计桥：SQLite 审计 → MCP audit（同一 store，§11）。
pub struct SqliteAuditBridge {
    store: Arc<devtoolbox_infrastructure::ServerActionAuditSqlite>,
}

impl SqliteAuditBridge {
    #[must_use]
    pub fn new(store: Arc<devtoolbox_infrastructure::ServerActionAuditSqlite>) -> Self {
        Self { store }
    }
}

impl devtoolbox_application::server::ActionAuditPort for SqliteAuditBridge {
    fn record(&self, entry: &devtoolbox_core::server::AuditEntry) {
        if let Err(error) = self.store.record(entry) {
            eprintln!("[mcp] audit write failed: {error}");
        }
    }
    fn recent(&self, limit: usize) -> Vec<devtoolbox_core::server::AuditEntry> {
        self.store.recent(limit).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_application::server::action::ActionPlan;
    use devtoolbox_core::server::SessionTrust;

    #[test]
    fn phase1_composition_fails_closed() {
        let composition = build(BuildOptions::default()).expect("compose");
        // 空注册表 → 无工具可发现。
        assert!(composition.registry.specs().is_empty());
        // 未配置身份 → 远程绑定被 startup gate 拒绝（§46）。
        assert!(!composition.identity_configured());
        // 空服务注册表 → restart 请求被拒。
        let request = devtoolbox_application::server::action::restart_request(
            "self-tools",
            "cli",
            "Self Tools",
        );
        match composition
            .actions
            .plan(&request, SessionTrust::LocalDesktop)
        {
            ActionPlan::Denied { reason, .. } => assert_eq!(reason, "unknown_service"),
            other => panic!("expected denied, got {other:?}"),
        }
    }
}
