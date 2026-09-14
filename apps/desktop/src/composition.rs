//! 桌面端组合根（Composition Root）：把基础设施实现绑定到 application 的端口。
//!
//! 本模块只做装配（创建适配器、把 Store/Port 接成 Service），不含业务规则、
//! SQL 或 UI；端口 trait 全部定义在 `devtoolbox_application`，基础设施类型
//! 全部来自 `devtoolbox_infrastructure`，二者在这里汇合。
//!
//! 每个领域一个轻量适配器（文档 + 设置 + History + Geography），
//! 避免出现中心化的 God Object。

use std::path::{Path, PathBuf};

use devtoolbox_application::workflows::{
    DocumentStoreError, DocumentStorePort, SettingsStoreError, SettingsStorePort,
};
use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::workspace::WorkspaceFile;
use devtoolbox_infrastructure::{
    SettingsStore, read_utf8, scan_markdown_files, write_utf8_atomic,
};

// ---------- 文档 / 工作区（文件系统适配器） ----------

/// 无状态文件系统文档存储：每次调用直接读 / 写 / 扫描，不持有任何状态。
#[derive(Clone, Copy, Default)]
pub struct DocumentStoreAdapter;

impl DocumentStorePort for DocumentStoreAdapter {
    fn read(&self, path: &Path) -> Result<String, DocumentStoreError> {
        read_utf8(path).map_err(|error| DocumentStoreError(error.to_string()))
    }
    fn write(&self, path: &Path, text: &str) -> Result<(), DocumentStoreError> {
        write_utf8_atomic(path, text).map_err(|error| DocumentStoreError(error.to_string()))
    }
    fn scan_markdown(&self, root: &Path) -> Result<Vec<WorkspaceFile>, DocumentStoreError> {
        scan_markdown_files(root).map_err(|error| DocumentStoreError(error.to_string()))
    }
}

// ---------- 设置（SettingsStore 适配器） ----------

/// 把 `devtoolbox_infrastructure::SettingsStore` 包装成 application 的设置端口。
pub struct SettingsStoreAdapter {
    store: SettingsStore,
}

impl SettingsStoreAdapter {
    #[must_use]
    pub fn new(store: SettingsStore) -> Self {
        Self { store }
    }
}

impl SettingsStorePort for SettingsStoreAdapter {
    fn path(&self) -> PathBuf {
        self.store.path().to_owned()
    }
    fn load(&self) -> Result<AppSettings, SettingsStoreError> {
        self.store.load().map_err(|error| SettingsStoreError(error.to_string()))
    }
    fn save(&self, settings: &AppSettings) -> Result<(), SettingsStoreError> {
        self.store
            .save(settings)
            .map_err(|error| SettingsStoreError(error.to_string()))
    }
}