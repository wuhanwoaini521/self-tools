use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::travel::search::TravelSearchBackend;
use crate::{InfrastructureError, error::io_error};

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
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    #[must_use]
    pub fn new(config_directory: impl AsRef<Path>) -> Self {
        Self {
            path: config_directory.as_ref().join("settings.json"),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<AppSettings, InfrastructureError> {
        if !self.path.exists() {
            return Ok(AppSettings::default());
        }
        let contents =
            fs::read_to_string(&self.path).map_err(|source| io_error(&self.path, source))?;
        serde_json::from_str(&contents).map_err(|source| InfrastructureError::SettingsDecode {
            path: self.path.clone(),
            source,
        })
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), InfrastructureError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        let bytes =
            serde_json::to_vec_pretty(settings).map_err(InfrastructureError::SettingsEncode)?;
        let temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| io_error(parent, source))?;
        fs::write(temporary.path(), bytes).map_err(|source| io_error(&self.path, source))?;
        temporary
            .persist(&self.path)
            .map_err(|error| io_error(&self.path, error.error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::SettingsStore;

    #[test]
    fn defaults_then_round_trips() {
        let directory = tempdir().expect("temporary directory");
        let store = SettingsStore::new(directory.path());
        let mut settings = store.load().expect("default settings");
        settings.auto_save = true;
        store.save(&settings).expect("save settings");
        assert_eq!(store.load().expect("reload settings"), settings);
    }

    #[test]
    fn legacy_settings_without_ui_theme_get_the_default_theme() {
        let directory = tempdir().expect("temporary directory");
        let store = SettingsStore::new(directory.path());
        fs::write(
            store.path(),
            r#"{"schema_version":1,"recent_files":[],"workspace_path":null,"theme_mode":"dark","editor_font_size":14,"auto_save":false,"markdown_default_view":"split"}"#,
        )
        .expect("write legacy settings");
        let settings = store.load().expect("load legacy settings");
        assert_eq!(settings.ui_theme, "default");
    }
}
