//! 文档 / 工作区 / 设置工作流的存储端口。
//!
//! 端口属于用例层（application crate）：平台适配层（Desktop / 未来 HTTP）
//! 在组合根把这些 trait 绑定到文件系统 / `SettingsStore` 等基础设施实现，
//! 用例层不感知文件系统与序列化细节。错误是端口自有的可显示文本包装，
//! 由适配层负责把基础设施错误转换进来（消息文本保持不变）。

use std::fmt;
use std::path::{Path, PathBuf};

use devtoolbox_core::settings::AppSettings;
use devtoolbox_core::workspace::WorkspaceFile;

/// 文档存储端口的错误（适配层已把基础设施错误转换为可显示文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStoreError(pub String);

impl fmt::Display for DocumentStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DocumentStoreError {}

/// 文档 / 工作区文件的读取、写入与扫描端口。
pub trait DocumentStorePort {
    fn read(&self, path: &Path) -> Result<String, DocumentStoreError>;
    fn write(&self, path: &Path, text: &str) -> Result<(), DocumentStoreError>;
    fn scan_markdown(&self, root: &Path) -> Result<Vec<WorkspaceFile>, DocumentStoreError>;
}

/// 设置存储端口：`settings.json` 的读 / 写，路径用于构造用户可见错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsStoreError(pub String);

impl fmt::Display for SettingsStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SettingsStoreError {}

pub trait SettingsStorePort {
    /// settings.json 的路径（用于错误消息，与老 `SettingsStore::path()` 一致）。
    fn path(&self) -> PathBuf;
    fn load(&self) -> Result<AppSettings, SettingsStoreError>;
    fn save(&self, settings: &AppSettings) -> Result<(), SettingsStoreError>;
}

/// 这是 application 侧测试可复用的内存 Fake 端口。
#[cfg(test)]
pub mod fakes {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard};

    /// 内存文档存储：`scan_markdown` 只返回 `.md` / `.markdown` 后缀文件。
    #[derive(Default)]
    pub struct InMemoryDocumentStore {
        files: Mutex<HashMap<PathBuf, String>>,
    }

    impl DocumentStorePort for InMemoryDocumentStore {
        fn read(&self, path: &Path) -> Result<String, DocumentStoreError> {
            self.files
                .lock()
                .expect("document store poisoned")
                .get(path)
                .cloned()
                .ok_or_else(|| DocumentStoreError(format!("文件不存在: {}", path.display())))
        }
        fn write(&self, path: &Path, text: &str) -> Result<(), DocumentStoreError> {
            self.files
                .lock()
                .expect("document store poisoned")
                .insert(path.to_owned(), text.to_owned());
            Ok(())
        }
        fn scan_markdown(&self, root: &Path) -> Result<Vec<WorkspaceFile>, DocumentStoreError> {
            let files = self.files.lock().expect("document store poisoned");
            let mut entries = files
                .iter()
                .filter(|(path, _)| path.starts_with(root))
                .filter(|(path, _)| {
                    path.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            extension.eq_ignore_ascii_case("md")
                                || extension.eq_ignore_ascii_case("markdown")
                        })
                })
                .map(|(path, _)| WorkspaceFile {
                    path: path.clone(),
                    relative_path: path
                        .strip_prefix(root)
                        .expect("starts_with guarantees strip")
                        .to_owned(),
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|file| file.relative_path.to_string_lossy().to_lowercase());
            Ok(entries)
        }
    }

    /// 内存设置存储：可选读全部失败（模拟目录不存在等 I/O 错误）。
    pub struct InMemorySettingsStore {
        pub settings: Mutex<AppSettings>,
        pub fail: bool,
    }

    impl Default for InMemorySettingsStore {
        fn default() -> Self {
            Self {
                settings: Mutex::new(AppSettings::default()),
                fail: false,
            }
        }
    }

    impl SettingsStorePort for InMemorySettingsStore {
        fn path(&self) -> PathBuf {
            PathBuf::from("settings.json")
        }
        fn load(&self) -> Result<AppSettings, SettingsStoreError> {
            if self.fail {
                return Err(SettingsStoreError("broken settings.json".into()));
            }
            let guard: MutexGuard<'_, AppSettings> =
                self.settings.lock().expect("settings poisoned");
            Ok(guard.clone())
        }
        fn save(&self, settings: &AppSettings) -> Result<(), SettingsStoreError> {
            if self.fail {
                return Err(SettingsStoreError("cannot write settings.json".into()));
            }
            *self.settings.lock().expect("settings poisoned") = settings.clone();
            Ok(())
        }
    }
}