use std::{
    fs,
    path::{Path, PathBuf},
};

use devtoolbox_core::settings::AppSettings;

use crate::{InfrastructureError, error::io_error};

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
