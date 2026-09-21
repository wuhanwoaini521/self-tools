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
    /// Personal AI 模块设置（V4；全部 Optional，未配置时 AI Panel 显示未配置状态）。
    pub ai: AiSettings,
    /// Personal Knowledge 设置（V6；允许根为空时 Documents/Files 如实报告未配置）。
    pub knowledge: KnowledgeSettings,
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
            ai: AiSettings::default(),
            knowledge: KnowledgeSettings::default(),
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
        settings
            .file_roots
            .push(crate::files::KnowledgeRoot::new("docs", "资料", "/data/docs"));
        assert_eq!(settings.effective_document_roots().len(), 1);
        assert!(settings.is_configured());

        settings.document_roots.push(crate::files::KnowledgeRoot::new(
            "kn", "知识库", "/data/knowledge",
        ));
        assert_eq!(settings.effective_document_roots()[0].id, "kn");
    }
}
