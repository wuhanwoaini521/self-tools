use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{InfrastructureError, error::io_error};

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
    pub editor_font_size: u8,
    pub auto_save: bool,
    pub markdown_default_view: MarkdownView,
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
            editor_font_size: 13,
            auto_save: false,
            markdown_default_view: MarkdownView::Split,
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
