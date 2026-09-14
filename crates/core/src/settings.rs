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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
            theme_mode: ThemeMode::System,
            ui_theme: "default".to_string(),
            rss_refresh_minutes: 30,
            editor_font_size: 13,
            auto_save: false,
            markdown_default_view: MarkdownView::Split,
            travel: TravelSettings::default(),
            geography: GeographySettings::default(),
        }
    }
}