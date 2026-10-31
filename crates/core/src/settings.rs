//! 应用设置的纯数据契约（`settings.json` 的持久化形状）。
//!
//! 与 `history_records` 同理：这些类型既被持久化适配器（infrastructure
//! `SettingsStore`）读写，也被 application 工作流与设置端口使用，因此由
//! core 持有，避免 app → infra 的依赖方向。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 搜索后端配置（来自应用设置，映射前端 TravelSettings）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelSearchBackend {
    /// 自动：Bing 国内 > 百度
    #[default]
    Auto,
    /// 本地 SearXNG（需配置 `searxng_url`）
    Searxng,
    /// 仅百度
    Baidu,
    /// 仅必应
    Bing,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TravelSettings {
    /// 搜索后端（Auto / SearXNG / Baidu / Bing）。
    pub search_backend: TravelSearchBackend,
    /// 本地 SearXNG 地址（如 http://localhost:8080；未配置则走自动后端）。
    pub searxng_url: Option<String>,
    /// LLM API base（OpenAI Compatible；如 https://api.deepseek.com/v1 或 http://localhost:11434/v1）。
    pub llm_base_url: Option<String>,
    /// LLM API key（本地 Ollama 可留空）。
    pub llm_api_key: Option<String>,
    /// LLM 模型名（如 deepseek-chat / qwen-plus）。
    pub llm_model: Option<String>,
    /// 高德开放平台 Key（可选；未配置不影响 Travel 核心功能）。
    pub amap_api_key: Option<String>,
    /// 和风天气 Key（可选）。
    pub qweather_api_key: Option<String>,
    /// 和风天气控制台分配的专属 API Host（2026 年起不再使用公共域名）。
    pub qweather_api_host: Option<String>,
    /// 百度地图开放平台 Key（可选）。
    pub baidu_map_api_key: Option<String>,
}

impl Default for TravelSettings {
    fn default() -> Self {
        Self {
            search_backend: TravelSearchBackend::Auto,
            searxng_url: None,
            llm_base_url: None,
            llm_api_key: None,
            llm_model: None,
            amap_api_key: None,
            qweather_api_key: None,
            qweather_api_host: None,
            baidu_map_api_key: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct GeographySettings {
    /// 地理模块使用的高德 Web 服务 API Key（可选）。
    pub amap_api_key: Option<String>,
    /// 地理模块使用的高德 Web 端 JS API 安全密钥（可选）。
    pub amap_security_js_code: Option<String>,
}

// `server.thresholds` 含 f32 → 无 `Eq`；AppSettings 随之只用 `PartialEq`。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AppSettings {
    pub schema_version: u8,
    pub recent_files: Vec<PathBuf>,
    pub workspace_path: Option<PathBuf>,
    pub theme_mode: ThemeMode,
    /// UI 风格主题 id(如 "default" / "warm-editorial"),
    /// 由前端 ThemeManager 注册表校验并回退;Rust 侧仅透传存储,不做主题枚举分支。
    pub ui_theme: String,
    /// RSS 自动刷新间隔(分钟),由前端定时器消费。
    pub rss_refresh_minutes: u32,
    pub editor_font_size: u8,
    pub auto_save: bool,
    pub markdown_default_view: MarkdownView,
    /// Travel 模块设置（全部 Optional，未配置时模块仍可运行）。
    pub travel: TravelSettings,
    /// Geography 模块设置（全部 Optional，未配置时模块仍可运行）。
    pub geography: GeographySettings,
    /// Personal AI 模块设置（V4；全部 Optional，未配置时 AI Panel 显示未配置状态）。
    pub ai: AiSettings,
    /// Personal Knowledge 设置（V6；允许根为空时 Documents/Files 如实报告未配置）。
    pub knowledge: KnowledgeSettings,
    /// Home Server 设置（V7；注册表为空 = 无能力，fail-closed）。
    pub server: ServerSettings,
    /// 决策层设置（V10；默认 rule 模式，Jev 未配置 = 纯规则）。
    pub decision: DecisionSettings,
}

/// Settings payload safe to return to a client. Provider credentials remain in
/// `AppSettings`; this DTO exposes only configured flags and non-secret values.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PublicAppSettings {
    pub schema_version: u8,
    pub recent_files: Vec<PathBuf>,
    pub workspace_path: Option<PathBuf>,
    pub theme_mode: ThemeMode,
    pub ui_theme: String,
    pub rss_refresh_minutes: u32,
    pub editor_font_size: u8,
    pub auto_save: bool,
    pub markdown_default_view: MarkdownView,
    pub travel: PublicTravelSettings,
    pub geography: PublicGeographySettings,
    pub ai: PublicAiSettings,
    pub knowledge: KnowledgeSettings,
    pub server: ServerSettings,
    pub decision: PublicDecisionSettings,
    pub secrets: SecretSettingsStatus,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PublicTravelSettings {
    pub search_backend: TravelSearchBackend,
    pub searxng_url: Option<String>,
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    pub qweather_api_host: Option<String>,
}

/// Geography currently has no client-readable settings. Keep the section stable
/// in the public contract without exposing its browser-map credentials.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PublicGeographySettings {}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PublicAiSettings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct PublicDecisionSettings {
    pub mode: String,
    pub jev_base_url: Option<String>,
    pub jev_model: Option<String>,
    pub jev_timeout_secs: u64,
    pub jev_failure_fallback: bool,
    pub multi_agent_enabled: bool,
    pub max_workers: usize,
}

impl Default for PublicDecisionSettings {
    fn default() -> Self {
        let settings = DecisionSettings::default();
        Self {
            mode: settings.mode,
            jev_base_url: settings.jev_base_url,
            jev_model: settings.jev_model,
            jev_timeout_secs: settings.jev_timeout_secs,
            jev_failure_fallback: settings.jev_failure_fallback,
            multi_agent_enabled: settings.multi_agent_enabled,
            max_workers: settings.max_workers,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SecretSettingsStatus {
    pub ai_api_key_configured: bool,
    pub travel_llm_api_key_configured: bool,
    pub travel_amap_api_key_configured: bool,
    pub travel_qweather_api_key_configured: bool,
    pub travel_baidu_map_api_key_configured: bool,
    pub geography_amap_api_key_configured: bool,
    pub geography_amap_security_js_code_configured: bool,
    pub jev_api_key_configured: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretSettingsField {
    AiApiKey,
    TravelLlmApiKey,
    TravelAmapApiKey,
    TravelQweatherApiKey,
    TravelBaiduMapApiKey,
    GeographyAmapApiKey,
    GeographyAmapSecurityJsCode,
    JevApiKey,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SecretSettingsValueUpdate {
    pub field: SecretSettingsField,
    /// `null` clears a stored credential; the raw value is accepted only on this
    /// dedicated write path and is never included in a settings read response.
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SecretSettingsUpdate {
    pub updates: Vec<SecretSettingsValueUpdate>,
}

impl AppSettings {
    /// Build the client-safe settings DTO. No credential value is copied.
    #[must_use]
    pub fn public_view(&self) -> PublicAppSettings {
        PublicAppSettings {
            schema_version: self.schema_version,
            recent_files: self.recent_files.clone(),
            workspace_path: self.workspace_path.clone(),
            theme_mode: self.theme_mode.clone(),
            ui_theme: self.ui_theme.clone(),
            rss_refresh_minutes: self.rss_refresh_minutes,
            editor_font_size: self.editor_font_size,
            auto_save: self.auto_save,
            markdown_default_view: self.markdown_default_view.clone(),
            travel: PublicTravelSettings {
                search_backend: self.travel.search_backend.clone(),
                searxng_url: self.travel.searxng_url.clone(),
                llm_base_url: self.travel.llm_base_url.clone(),
                llm_model: self.travel.llm_model.clone(),
                qweather_api_host: self.travel.qweather_api_host.clone(),
            },
            geography: PublicGeographySettings::default(),
            ai: PublicAiSettings {
                provider: self.ai.provider.clone(),
                model: self.ai.model.clone(),
                base_url: self.ai.base_url.clone(),
                timeout_secs: self.ai.timeout_secs,
            },
            knowledge: self.knowledge.clone(),
            server: self.server.clone(),
            decision: PublicDecisionSettings {
                mode: self.decision.mode.clone(),
                jev_base_url: self.decision.jev_base_url.clone(),
                jev_model: self.decision.jev_model.clone(),
                jev_timeout_secs: self.decision.jev_timeout_secs,
                jev_failure_fallback: self.decision.jev_failure_fallback,
                multi_agent_enabled: self.decision.multi_agent_enabled,
                max_workers: self.decision.max_workers,
            },
            secrets: SecretSettingsStatus {
                ai_api_key_configured: has_secret(&self.ai.api_key),
                travel_llm_api_key_configured: has_secret(&self.travel.llm_api_key),
                travel_amap_api_key_configured: has_secret(&self.travel.amap_api_key),
                travel_qweather_api_key_configured: has_secret(&self.travel.qweather_api_key),
                travel_baidu_map_api_key_configured: has_secret(&self.travel.baidu_map_api_key),
                geography_amap_api_key_configured: has_secret(&self.geography.amap_api_key),
                geography_amap_security_js_code_configured: has_secret(
                    &self.geography.amap_security_js_code,
                ),
                jev_api_key_configured: has_secret(&self.decision.jev_api_key),
            },
        }
    }

    /// Apply non-secret client settings while preserving backend-owned credentials.
    pub fn apply_public_settings(&mut self, public: &PublicAppSettings) {
        self.schema_version = public.schema_version;
        self.recent_files.clone_from(&public.recent_files);
        self.workspace_path.clone_from(&public.workspace_path);
        self.theme_mode = public.theme_mode.clone();
        self.ui_theme.clone_from(&public.ui_theme);
        self.rss_refresh_minutes = public.rss_refresh_minutes;
        self.editor_font_size = public.editor_font_size;
        self.auto_save = public.auto_save;
        self.markdown_default_view = public.markdown_default_view.clone();
        self.travel.search_backend = public.travel.search_backend.clone();
        self.travel
            .searxng_url
            .clone_from(&public.travel.searxng_url);
        self.travel
            .llm_base_url
            .clone_from(&public.travel.llm_base_url);
        self.travel.llm_model.clone_from(&public.travel.llm_model);
        self.travel
            .qweather_api_host
            .clone_from(&public.travel.qweather_api_host);
        self.ai.provider.clone_from(&public.ai.provider);
        self.ai.model.clone_from(&public.ai.model);
        self.ai.base_url.clone_from(&public.ai.base_url);
        self.ai.timeout_secs = public.ai.timeout_secs;
        self.knowledge.clone_from(&public.knowledge);
        self.server.clone_from(&public.server);
        self.decision.mode.clone_from(&public.decision.mode);
        self.decision
            .jev_base_url
            .clone_from(&public.decision.jev_base_url);
        self.decision
            .jev_model
            .clone_from(&public.decision.jev_model);
        self.decision.jev_timeout_secs = public.decision.jev_timeout_secs;
        self.decision.jev_failure_fallback = public.decision.jev_failure_fallback;
        self.decision.multi_agent_enabled = public.decision.multi_agent_enabled;
        self.decision.max_workers = public.decision.max_workers;
    }

    /// Apply an explicit credential write/clear. Empty strings are treated as
    /// clears; otherwise values are preserved exactly as entered.
    pub fn apply_secret_updates(&mut self, update: &SecretSettingsUpdate) {
        for item in &update.updates {
            let value = item.value.clone().filter(|value| !value.trim().is_empty());
            match item.field {
                SecretSettingsField::AiApiKey => self.ai.api_key = value,
                SecretSettingsField::TravelLlmApiKey => self.travel.llm_api_key = value,
                SecretSettingsField::TravelAmapApiKey => self.travel.amap_api_key = value,
                SecretSettingsField::TravelQweatherApiKey => self.travel.qweather_api_key = value,
                SecretSettingsField::TravelBaiduMapApiKey => self.travel.baidu_map_api_key = value,
                SecretSettingsField::GeographyAmapApiKey => self.geography.amap_api_key = value,
                SecretSettingsField::GeographyAmapSecurityJsCode => {
                    self.geography.amap_security_js_code = value;
                }
                SecretSettingsField::JevApiKey => self.decision.jev_api_key = value,
            }
        }
    }
}

impl Default for PublicAppSettings {
    fn default() -> Self {
        AppSettings::default().public_view()
    }
}

fn has_secret(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownView {
    Editor,
    Split,
    Preview,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            recent_files: Vec::new(),
            workspace_path: None,
            theme_mode: ThemeMode::Light,
            ui_theme: "default".to_string(),
            rss_refresh_minutes: 30,
            editor_font_size: 13,
            auto_save: false,
            markdown_default_view: MarkdownView::Split,
            travel: TravelSettings::default(),
            geography: GeographySettings::default(),
            ai: AiSettings::default(),
            knowledge: KnowledgeSettings::default(),
            server: ServerSettings::default(),
            decision: DecisionSettings::default(),
        }
    }
}

/// Personal AI 模型配置（V4 §28）。与 Travel 的 LLM 配置同先例：
/// 存于 gitignored `config/settings.json`，永不进 git / 日志 / 前端。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct AiSettings {
    /// 提供方标识（当前仅 `openai-compatible`；缺省按该路线处理）。
    pub provider: Option<String>,
    /// 模型名（如 `deepseek-chat` / `qwen-plus` / `gpt-4o-mini`）。
    pub model: Option<String>,
    /// OpenAI-Compatible API base（如 `https://api.deepseek.com/v1` 或 `http://localhost:11434/v1`）。
    pub base_url: Option<String>,
    /// API key（本地 Ollama 可留空）。
    pub api_key: Option<String>,
    /// 模型调用超时（秒）；缺省 120。
    pub timeout_secs: Option<u64>,
}

impl AiSettings {
    /// 是否具备发起模型调用的条件（base + model 齐全）。
    #[must_use]
    pub fn is_configured(&self) -> bool {
        let base = self.base_url.as_deref().unwrap_or_default().trim();
        let model = self.model.as_deref().unwrap_or_default().trim();
        !base.is_empty() && !model.is_empty()
    }
}

/// Personal Knowledge 设置（V6 §43/§89）。
///
/// 默认**空**：不写死任何用户路径；未配置时 Documents/Files 模块如实报告
/// 「未配置允许目录」，AI 无法读取任何文件（§7 降级而非崩溃）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct KnowledgeSettings {
    /// 允许 AI 搜索 / 读取元数据 / 安全读取的文件根。
    pub file_roots: Vec<crate::files::KnowledgeRoot>,
    /// 进入文档索引的根（空 = 复用 `file_roots`）。
    pub document_roots: Vec<crate::files::KnowledgeRoot>,
    /// 单文档索引上限（字节）；超过则只索引元数据（§92）。
    pub max_document_bytes: u64,
    /// 单次安全读取的字符上限。
    pub max_read_chars: usize,
    /// 索引文件数上限（防止误配大目录导致全盘扫描）。
    pub max_indexed_files: usize,
    /// 启动时执行一次轻量同步（§89；不监听文件系统变更）。
    pub startup_sync: bool,
}

impl Default for KnowledgeSettings {
    fn default() -> Self {
        Self {
            file_roots: Vec::new(),
            document_roots: Vec::new(),
            max_document_bytes: crate::documents::ChunkConfig::default().max_document_bytes,
            max_read_chars: 20_000,
            max_indexed_files: 20_000,
            startup_sync: true,
        }
    }
}

impl KnowledgeSettings {
    /// 实际使用的文档根（未配置时复用文件根，避免两套配置冗余）。
    #[must_use]
    pub fn effective_document_roots(&self) -> Vec<crate::files::KnowledgeRoot> {
        if self.document_roots.is_empty() {
            self.file_roots.clone()
        } else {
            self.document_roots.clone()
        }
    }

    /// 文件侧访问策略。
    #[must_use]
    pub fn file_policy(&self) -> crate::files::FileAccessPolicy {
        crate::files::FileAccessPolicy::new(self.file_roots.clone())
    }

    /// 分块配置（把设置里的体积上限接进 `ChunkConfig`，§33 集中配置）。
    #[must_use]
    pub fn chunk_config(&self) -> crate::documents::ChunkConfig {
        crate::documents::ChunkConfig {
            max_document_bytes: self.max_document_bytes,
            ..crate::documents::ChunkConfig::default()
        }
    }

    /// 是否已配置任何可用根。
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.file_roots.is_empty() || !self.document_roots.is_empty()
    }
}

/// Home Server 设置（V7 §26/§42/§55/§67）。
///
/// 注册表**显式配置**：空 = 未注册任何服务/应用 = AI 无可操作目标。
/// 阈值与冷却全部配置化（§22/§59/§67）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    /// 已注册服务（§27）。
    #[serde(default)]
    pub services: Vec<crate::server::ServiceDescriptor>,
    /// 已注册应用（§43）。
    #[serde(default)]
    pub applications: Vec<crate::server::ApplicationDescriptor>,
    /// 健康阈值（§22）。
    #[serde(default)]
    pub thresholds: crate::server::Thresholds,
    /// 确认票据有效期秒数（§59：30–120）。
    #[serde(default = "default_confirmation_ttl")]
    pub confirmation_ttl_secs: i64,
    /// 同一目标冷却秒数（§67）。
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: i64,
    /// 每会话 SYSTEM 操作上限（§67）。
    #[serde(default = "default_session_limit")]
    pub max_system_per_session: usize,
    /// 审计保留条数 / 天数（§112）。
    #[serde(default = "default_audit_entries")]
    pub audit_max_entries: usize,
    #[serde(default = "default_audit_days")]
    pub audit_retention_days: i64,
    /// MCP 设置（V8 §47：transport 开关 + 绑定 + 远程）。
    #[serde(default)]
    pub mcp: McpSettings,
}

/// MCP 传输设置（V8 §47）。
///
/// 远程**默认关闭**（Principle 7）：`remote_enabled = true` 且配置了身份
/// 提供者之前，非 loopback 绑定会在启动时被拒（§46）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpSettings {
    /// 是否启用 MCP transport。
    #[serde(default = "default_mcp_enabled")]
    pub enabled: bool,
    /// STDIO transport 开关（§24）。
    #[serde(default = "default_true")]
    pub stdio_enabled: bool,
    /// Streamable HTTP transport 开关（§43）。
    #[serde(default)]
    pub http_enabled: bool,
    /// HTTP 绑定地址（默认 loopback，§44）。
    #[serde(default = "default_mcp_bind")]
    pub bind: String,
    /// 远程（非 loopback）访问开关（§45）。
    #[serde(default)]
    pub remote_enabled: bool,
    /// HTTP 端口（与 bind 二选一；bind 已含端口时忽略）。
    #[serde(default = "default_mcp_port")]
    pub port: u16,
}

fn default_mcp_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_mcp_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_mcp_port() -> u16 {
    8787
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: default_mcp_enabled(),
            stdio_enabled: default_true(),
            http_enabled: false,
            bind: default_mcp_bind(),
            remote_enabled: false,
            port: default_mcp_port(),
        }
    }
}

fn default_confirmation_ttl() -> i64 {
    60
}

fn default_cooldown() -> i64 {
    60
}

fn default_session_limit() -> usize {
    5
}

fn default_audit_entries() -> usize {
    500
}

fn default_audit_days() -> i64 {
    30
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            services: Vec::new(),
            applications: Vec::new(),
            thresholds: crate::server::Thresholds::default(),
            confirmation_ttl_secs: default_confirmation_ttl(),
            cooldown_secs: default_cooldown(),
            max_system_per_session: default_session_limit(),
            audit_max_entries: default_audit_entries(),
            audit_retention_days: default_audit_days(),
            mcp: McpSettings::default(),
        }
    }
}

/// 决策层设置（V10 §26/§37/§52）。
///
/// 默认 `rule`：未配置 Jev key 时禁止 Shadow/Active（fail-closed）。
/// `jev_failure_fallback` 恒 true（不可关闭）：provider 失败必须回落 Rule。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionSettings {
    /// 决策模式：`rule` / `jev_shadow` / `jev_active`。
    #[serde(default = "default_decision_mode")]
    pub mode: String,
    /// Jev API key（只进后端配置；**永不**进日志 / 前端 / git）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jev_api_key: Option<String>,
    /// Jev API base（缺省官方）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jev_base_url: Option<String>,
    /// Jev 模型别名（缺省 `jev-latest`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jev_model: Option<String>,
    /// 单次决策超时（秒；1..60，缺省 5）。
    #[serde(default = "default_decision_timeout")]
    pub jev_timeout_secs: u64,
    /// Jev 失败是否回落 Rule（恒 true；字段保留供将来扩展，反序列化忽略 false）。
    #[serde(default = "default_true")]
    pub jev_failure_fallback: bool,
    /// 多 Agent 编排总开关（§52：默认开，但有界）。
    #[serde(default = "default_true")]
    pub multi_agent_enabled: bool,
    /// worker 并发硬上限（来自 AgentBudget.max_agents；默认 4）。
    #[serde(default = "default_max_workers")]
    pub max_workers: usize,
}

fn default_decision_mode() -> String {
    "rule".to_string()
}

fn default_decision_timeout() -> u64 {
    5
}

fn default_max_workers() -> usize {
    4
}

impl Default for DecisionSettings {
    fn default() -> Self {
        Self {
            mode: default_decision_mode(),
            jev_api_key: None,
            jev_base_url: None,
            jev_model: None,
            jev_timeout_secs: default_decision_timeout(),
            jev_failure_fallback: true,
            multi_agent_enabled: true,
            max_workers: default_max_workers(),
        }
    }
}

impl DecisionSettings {
    /// 解析后的决策模式（非法值 → Rule，fail-closed）。
    #[must_use]
    pub fn mode_enum(&self) -> crate::agents::DecisionMode {
        crate::agents::DecisionMode::parse(&self.mode).unwrap_or_default()
    }

    /// Jev 是否已配置（只暴露布尔；绝不暴露 key 本身）。
    #[must_use]
    pub fn jev_configured(&self) -> bool {
        self.jev_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
    }

    /// 生效模式：未配置 Jev 时强制 Rule（§28/§52）。
    #[must_use]
    pub fn effective_mode(&self) -> crate::agents::DecisionMode {
        if !self.jev_configured() {
            return crate::agents::DecisionMode::Rule;
        }
        self.mode_enum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_json_still_decodes() {
        // V5 之前写入的 settings.json（没有 knowledge 字段）。
        let legacy = r#"{
            "schema_version": 1,
            "theme_mode": "light",
            "ui_theme": "warm-editorial",
            "travel": {"search_backend": "auto"},
            "ai": {"model": "deepseek-chat"}
        }"#;
        let settings: AppSettings = serde_json::from_str(legacy).expect("legacy settings decode");
        assert_eq!(settings.knowledge, KnowledgeSettings::default());
        // V7：旧 settings 没有 server 段 → 空注册表（无能力，fail-closed）。
        assert!(settings.server.services.is_empty());
        assert!(settings.server.applications.is_empty());
        assert_eq!(settings.server.confirmation_ttl_secs, 60);
        // V8：旧 settings 没有 mcp 段 → 默认 STDIO 开、HTTP 关、远程关（§47）。
        assert!(settings.server.mcp.enabled);
        assert!(settings.server.mcp.stdio_enabled);
        assert!(!settings.server.mcp.http_enabled);
        assert!(!settings.server.mcp.remote_enabled);
        assert_eq!(settings.server.mcp.bind, "127.0.0.1");
        assert!(settings.knowledge.file_roots.is_empty());
        assert!(!settings.knowledge.is_configured());
        assert!(!settings.knowledge.file_policy().is_configured());
        assert!(settings.knowledge.effective_document_roots().is_empty());
        assert_eq!(
            settings.knowledge.chunk_config().max_document_bytes,
            settings.knowledge.max_document_bytes
        );
    }

    #[test]
    fn document_roots_fall_back_to_file_roots() {
        let mut settings = KnowledgeSettings::default();
        settings.file_roots.push(crate::files::KnowledgeRoot::new(
            "docs",
            "资料",
            "/data/docs",
        ));
        assert_eq!(settings.effective_document_roots().len(), 1);
        assert!(settings.is_configured());

        settings
            .document_roots
            .push(crate::files::KnowledgeRoot::new(
                "kn",
                "知识库",
                "/data/knowledge",
            ));
        assert_eq!(settings.effective_document_roots()[0].id, "kn");
    }

    #[test]
    fn decision_settings_default_to_rule_and_fail_closed() {
        let settings = DecisionSettings::default();
        assert_eq!(settings.mode_enum(), crate::agents::DecisionMode::Rule);
        assert!(!settings.jev_configured());
        // 未配置 key → 即使 mode 写了 active 也必须落到 rule。
        let mut active = DecisionSettings {
            mode: "jev_active".into(),
            ..DecisionSettings::default()
        };
        assert_eq!(active.effective_mode(), crate::agents::DecisionMode::Rule);
        // 配置 key 后才允许 shadow/active。
        active.jev_api_key = Some("k".into());
        assert_eq!(
            active.effective_mode(),
            crate::agents::DecisionMode::JevActive
        );
        assert!(active.jev_configured());
        // 非法 mode → rule。
        let broken = DecisionSettings {
            mode: "teleport".into(),
            jev_api_key: Some("k".into()),
            ..DecisionSettings::default()
        };
        assert_eq!(broken.effective_mode(), crate::agents::DecisionMode::Rule);
    }

    #[test]
    fn decision_mode_parses_from_settings_string() {
        assert_eq!(
            crate::agents::DecisionMode::parse("jev_shadow"),
            Some(crate::agents::DecisionMode::JevShadow)
        );
        assert_eq!(
            crate::agents::DecisionMode::parse("jev-active"),
            Some(crate::agents::DecisionMode::JevActive)
        );
        assert_eq!(crate::agents::DecisionMode::parse(""), None);
    }

    #[test]
    fn legacy_settings_decode_without_decision_section() {
        let legacy = r#"{"schema_version":1,"server":{"mcp":{"enabled":true}}}"#;
        let settings: AppSettings = serde_json::from_str(legacy).expect("decode");
        assert_eq!(settings.decision, DecisionSettings::default());
        assert_eq!(settings.decision.mode, "rule");
    }

    #[test]
    fn decision_serialization_skips_empty_key() {
        let settings = DecisionSettings::default();
        let json = serde_json::to_value(&settings).expect("serialize");
        assert!(json.get("jev_api_key").is_none(), "空 key 不得序列化");
        assert_eq!(json["mode"], "rule");
        assert_eq!(json["max_workers"], 4);
    }

    #[test]
    fn public_settings_expose_secret_status_but_never_secret_values() {
        let mut settings = AppSettings::default();
        settings.ai.api_key = Some("ai-secret-marker".into());
        settings.travel.llm_api_key = Some("travel-secret-marker".into());
        settings.geography.amap_api_key = Some("map-secret-marker".into());
        settings.decision.jev_api_key = Some("jev-secret-marker".into());

        let json = serde_json::to_string(&settings.public_view()).expect("serialize public view");
        for secret in [
            "ai-secret-marker",
            "travel-secret-marker",
            "map-secret-marker",
            "jev-secret-marker",
        ] {
            assert!(!json.contains(secret), "public settings leaked {secret}");
        }
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(parsed["secrets"]["ai_api_key_configured"], true);
        assert_eq!(parsed["secrets"]["travel_llm_api_key_configured"], true);
        assert_eq!(parsed["secrets"]["geography_amap_api_key_configured"], true);
        assert_eq!(parsed["secrets"]["jev_api_key_configured"], true);
        assert!(parsed["ai"].get("api_key").is_none());
        assert!(parsed["travel"].get("llm_api_key").is_none());
        assert!(parsed["geography"].get("amap_api_key").is_none());
        assert!(parsed["decision"].get("jev_api_key").is_none());
    }

    #[test]
    fn public_settings_update_preserves_secrets_and_secret_updates_are_explicit() {
        let mut settings = AppSettings::default();
        settings.ai.api_key = Some("preserve-me".into());
        settings.travel.amap_api_key = Some("also-preserve".into());

        let mut public = settings.public_view();
        public.ai.base_url = Some("https://example.test/v1".into());
        public.secrets.ai_api_key_configured = false; // Client status is not authoritative.
        settings.apply_public_settings(&public);
        assert_eq!(settings.ai.api_key.as_deref(), Some("preserve-me"));
        assert_eq!(
            settings.travel.amap_api_key.as_deref(),
            Some("also-preserve")
        );
        assert_eq!(
            settings.ai.base_url.as_deref(),
            Some("https://example.test/v1")
        );

        settings.apply_secret_updates(&SecretSettingsUpdate {
            updates: vec![SecretSettingsValueUpdate {
                field: SecretSettingsField::AiApiKey,
                value: Some("replacement".into()),
            }],
        });
        assert_eq!(settings.ai.api_key.as_deref(), Some("replacement"));
        settings.apply_secret_updates(&SecretSettingsUpdate {
            updates: vec![SecretSettingsValueUpdate {
                field: SecretSettingsField::AiApiKey,
                value: None,
            }],
        });
        assert_eq!(settings.ai.api_key, None);
    }
}
