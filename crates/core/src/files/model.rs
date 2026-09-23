//! Files 领域契约（V6 Track C，§40-§48）。
//!
//! Files ≠ Documents（§40）：Files 描述**允许根内的文件实体**，
//! 只提供 search / metadata / safe read / open；V6 不存在任何写、移动、删除能力。
//!
//! 安全模型（§44）分两层：
//! - 本模块 = 纯策略（允许根、deny 规则、遍历检测）——确定性、可单测；
//! - infrastructure = canonicalize（解析 symlink）与文件系统访问。
//!
//! 两者组合后，任何「先解析再校验」的路径都必须通过 [`FileAccessPolicy`]。

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// 文件内容类别（§47：不支持文本读取时只返回元数据）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileContentKind {
    Text,
    Binary,
    #[default]
    Unknown,
}

/// 允许根（V6 §43：绝不写死用户路径；由用户在设置中配置）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeRoot {
    /// 稳定 id（用户可读；默认由路径派生）。
    pub id: String,
    /// 展示名。
    pub label: String,
    /// 根目录绝对路径。
    pub path: String,
    /// 是否启用（禁用不改数据，只不参与检索/索引）。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl KnowledgeRoot {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            path: path.into(),
            enabled: true,
        }
    }
}

/// 文件索引条目（元数据；不缓存正文）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// 稳定 id（`root_id` + 相对路径）。
    pub file_id: String,
    pub root_id: String,
    /// 绝对路径。
    pub path: String,
    pub relative_path: String,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub indexed_at: i64,
    pub content_kind: FileContentKind,
    /// 是否被 deny 规则命中（命中则永不返回内容）。
    #[serde(default)]
    pub restricted: bool,
    /// 索引失败原因（§91）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_error: Option<String>,
}

/// 文件访问被拒绝的原因（§44/§71：全部为受控错误，绝不回显文件内容）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAccessDenied {
    /// 未配置任何允许根（§7：功能降级而非崩溃）。
    NoRootsConfigured,
    /// 路径不在任何允许根内。
    OutsideAllowedRoots,
    /// 路径包含 `..` 组件（遍历尝试）。
    Traversal,
    /// 解析 symlink 后逃逸出允许根。
    SymlinkEscape,
    /// 命中 deny 规则（凭据/密钥类文件，§71）。
    DeniedPattern,
    /// 文件不存在。
    NotFound,
    /// 目标不是普通文件。
    NotAFile,
    /// 文件过大（超过读取上限）。
    TooLarge,
    /// 不是文本内容（二进制）。
    NotText,
}

impl FileAccessDenied {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            FileAccessDenied::NoRootsConfigured => "no_roots_configured",
            FileAccessDenied::OutsideAllowedRoots => "outside_allowed_roots",
            FileAccessDenied::Traversal => "traversal",
            FileAccessDenied::SymlinkEscape => "symlink_escape",
            FileAccessDenied::DeniedPattern => "denied_pattern",
            FileAccessDenied::NotFound => "not_found",
            FileAccessDenied::NotAFile => "not_a_file",
            FileAccessDenied::TooLarge => "too_large",
            FileAccessDenied::NotText => "not_text",
        }
    }

    /// 面向用户的中文说明（不含任何文件内容）。
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            FileAccessDenied::NoRootsConfigured => {
                "未配置允许目录：请在 设置 → 知识 中添加允许搜索的目录"
            }
            FileAccessDenied::OutsideAllowedRoots => "路径不在允许目录内，已拒绝访问",
            FileAccessDenied::Traversal => "路径包含目录遍历（..），已拒绝访问",
            FileAccessDenied::SymlinkEscape => "路径经符号链接指向允许目录之外，已拒绝访问",
            FileAccessDenied::DeniedPattern => "该文件属于凭据/密钥类敏感文件，已拒绝访问",
            FileAccessDenied::NotFound => "文件不存在",
            FileAccessDenied::NotAFile => "目标不是普通文件",
            FileAccessDenied::TooLarge => "文件过大，仅返回元数据",
            FileAccessDenied::NotText => "不是文本文件，仅返回元数据",
        }
    }
}

// ---------------------------------------------------------------------------
// 索引 / 检索共享数据（infra 实现、application 消费）
// ---------------------------------------------------------------------------

/// 文件检索条件（V6 §48：filename / extension / modified range / root / query）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileQuery {
    /// 文件名/路径关键词（大小写不敏感）。
    pub query: String,
    pub extension: Option<String>,
    pub root_id: Option<String>,
    pub modified_after: Option<i64>,
    pub limit: usize,
    /// 是否包含 deny 规则命中的文件（默认 false）。
    pub include_restricted: bool,
}

/// 原始文件信息（尚未判定是否被 deny 规则限制）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFile {
    pub path: std::path::PathBuf,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub is_file: bool,
}

/// 只读文本读取结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileReadOutcome {
    Text(String),
    /// 二进制内容（V6 §47：绝不把字节塞给模型）。
    Binary,
    /// 超过读取上限。
    TooLarge,
}

/// 索引里的文件指纹（增量索引用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFingerprint {
    pub file_id: String,
    pub root_id: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

/// 文件索引统计（只计数）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileIndexStats {
    pub files: usize,
    pub text_files: usize,
    pub binary_files: usize,
    pub restricted: usize,
    pub failed: usize,
}

/// 默认 deny 规则（§71）。匹配文件名/路径片段（大小写不敏感）。
pub const DEFAULT_DENY_PATTERNS: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    ".ssh",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".aws",
    ".gnupg",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "credentials.json",
    "credentials.yaml",
    "credential.json",
    "secrets.yaml",
    "secrets.json",
    "secrets.toml",
    "keychain",
    ".pfx",
    ".p12",
    ".pem",
    ".key",
    ".keystore",
    ".jks",
    "known_hosts",
    "shadow",
];

/// 文件访问策略（纯函数集合 + 允许根清单）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileAccessPolicy {
    roots: Vec<KnowledgeRoot>,
    deny_patterns: Vec<String>,
}

impl FileAccessPolicy {
    /// 由允许根构造（使用默认 deny 规则）。
    #[must_use]
    pub fn new(roots: Vec<KnowledgeRoot>) -> Self {
        Self {
            roots,
            deny_patterns: DEFAULT_DENY_PATTERNS
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect(),
        }
    }

    /// 自定义 deny 规则（测试用；默认规则仍然生效）。
    #[must_use]
    pub fn with_extra_deny(mut self, patterns: &[&str]) -> Self {
        self.deny_patterns
            .extend(patterns.iter().map(|pattern| (*pattern).to_string()));
        self
    }

    #[must_use]
    pub fn roots(&self) -> &[KnowledgeRoot] {
        &self.roots
    }

    /// 启用的允许根。
    #[must_use]
    pub fn enabled_roots(&self) -> Vec<&KnowledgeRoot> {
        self.roots.iter().filter(|root| root.enabled).collect()
    }

    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.enabled_roots().is_empty()
    }

    /// 解析后的绝对路径是否落在某个允许根内（前缀匹配；两侧都已 canonicalize）。
    #[must_use]
    pub fn root_id_for(&self, canonical_path: &Path) -> Option<String> {
        self.enabled_roots()
            .into_iter()
            .filter(|root| {
                let root_path = Path::new(&root.path);
                canonical_path.starts_with(root_path)
            })
            .max_by_key(|root| root.path.len())
            .map(|root| root.id.clone())
    }

    /// 相对路径是否命中 deny 规则（§71）。
    ///
    /// 两类模式分开匹配，避免裸词把正常文件名一网打尽：
    /// - **扩展名类**（以 `.` 开头，如 `.pem` / `.key`）：`ends_with` 后缀匹配，
    ///   命中 `server.pem` 与 `a.pem.bak` 之外的常规形态；
    /// - **裸词类**（如 `id_rsa` / `shadow` / `keychain`）：只做**全等**匹配
    ///   （文件名或路径段），否则 `my-shadow` / `keychain.md` 这类正常文件会被误拒。
    #[must_use]
    pub fn is_denied(&self, relative_path: &str, file_name: &str) -> bool {
        let relative = relative_path.to_ascii_lowercase();
        let name = file_name.to_ascii_lowercase();
        let segments: Vec<&str> = relative.split(['/', '\\']).collect();
        self.deny_patterns.iter().any(|raw| {
            let pattern = raw.to_ascii_lowercase();
            let exact = name == pattern || segments.iter().any(|segment| *segment == pattern);
            if pattern.starts_with('.') {
                // 扩展名 / 隐藏文件类：后缀匹配（`.env.local` 由精确分支覆盖）。
                exact || name.ends_with(&pattern)
            } else {
                exact
            }
        })
    }

    /// 是否包含 `..` 组件（§44 拒绝 traversal）。
    #[must_use]
    pub fn contains_traversal(path: &Path) -> bool {
        path.components()
            .any(|component| matches!(component, Component::ParentDir))
    }

    /// 授权判定：`input` 是用户/模型给的原始路径，`canonical` 是解析 symlink 后的绝对路径。
    ///
    /// 返回允许根 id 或拒绝原因。**任何文件工具都必须先调用本函数。**
    pub fn authorize(&self, input: &Path, canonical: &Path) -> Result<String, FileAccessDenied> {
        if Self::contains_traversal(input) {
            return Err(FileAccessDenied::Traversal);
        }
        if !self.is_configured() {
            return Err(FileAccessDenied::NoRootsConfigured);
        }
        self.root_id_for(canonical)
            .ok_or(FileAccessDenied::OutsideAllowedRoots)
    }
}

/// 文件 id（稳定：`root_id` + 相对路径）。
#[must_use]
pub fn file_id(root_id: &str, relative_path: &str) -> String {
    crate::knowledge::stable_id("file", &[root_id, relative_path])
}

/// 文件名（跨平台：同时接受 `/` 与 `\` 分隔）。
#[must_use]
pub fn file_name_of(relative_path: &str) -> String {
    relative_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(relative_path)
        .to_string()
}

/// 展示用路径：去掉 Windows 的 verbatim 前缀（`\\?\D:\a` → `D:\a`）。
///
/// 授权与 `strip_prefix` 一律使用 canonical 形式；此函数只用于**对用户与模型
/// 展示**的路径（`FileMetadata::path`），避免把 `\\?\` 暴露到 UI 与 prompt。
#[must_use]
pub fn display_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    path.to_string()
}

/// 扩展名（小写；无扩展名 → None）。
#[must_use]
pub fn extension_of(relative_path: &str) -> Option<String> {
    let name = file_name_of(relative_path);
    let (_, extension) = name.rsplit_once('.')?;
    if extension.is_empty() || extension == name {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root(id: &str, path: &str) -> KnowledgeRoot {
        KnowledgeRoot::new(id, id, path)
    }

    #[test]
    fn unconfigured_policy_denies_everything() {
        let policy = FileAccessPolicy::new(Vec::new());
        assert!(!policy.is_configured());
        assert_eq!(
            policy.authorize(Path::new("/tmp/a.md"), Path::new("/tmp/a.md")),
            Err(FileAccessDenied::NoRootsConfigured)
        );
    }

    #[test]
    fn inside_root_is_allowed_outside_is_denied() {
        let policy = FileAccessPolicy::new(vec![root("docs", "/data/documents")]);
        assert_eq!(
            policy.authorize(
                Path::new("/data/documents/a/b.md"),
                Path::new("/data/documents/a/b.md")
            ),
            Ok("docs".to_string())
        );
        assert_eq!(
            policy.authorize(Path::new("/data/other/b.md"), Path::new("/data/other/b.md")),
            Err(FileAccessDenied::OutsideAllowedRoots)
        );
    }

    #[test]
    fn traversal_components_are_rejected_before_canonicalization() {
        let policy = FileAccessPolicy::new(vec![root("docs", "/data/documents")]);
        assert!(FileAccessPolicy::contains_traversal(Path::new(
            "/data/documents/../../etc/passwd"
        )));
        assert_eq!(
            policy.authorize(
                Path::new("/data/documents/../secrets/id_rsa"),
                Path::new("/data/secrets/id_rsa")
            ),
            Err(FileAccessDenied::Traversal)
        );
        assert!(!FileAccessPolicy::contains_traversal(Path::new(
            "/data/documents/a.md"
        )));
    }

    #[test]
    fn symlink_escape_is_outside_after_canonicalization() {
        let policy = FileAccessPolicy::new(vec![root("docs", "/data/documents")]);
        // 输入看似在根内，但 canonicalize 之后在根外 → 逃逸。
        let denial = policy
            .authorize(
                Path::new("/data/documents/link-to-ssh"),
                Path::new("/home/user/.ssh"),
            )
            .unwrap_err();
        assert_eq!(denial, FileAccessDenied::OutsideAllowedRoots);
        assert_eq!(denial.as_str(), "outside_allowed_roots");
    }

    #[test]
    fn longest_matching_root_wins_and_disabled_roots_are_ignored() {
        let mut nested = root("nested", "/data/documents/private");
        let policy_roots = vec![root("docs", "/data/documents"), nested.clone()];
        let policy = FileAccessPolicy::new(policy_roots);
        assert_eq!(
            policy
                .authorize(
                    Path::new("/data/documents/private/a.md"),
                    Path::new("/data/documents/private/a.md")
                )
                .unwrap(),
            "nested"
        );

        nested.enabled = false;
        let policy = FileAccessPolicy::new(vec![root("docs", "/data/documents"), nested]);
        assert_eq!(
            policy
                .authorize(
                    Path::new("/data/documents/private/a.md"),
                    Path::new("/data/documents/private/a.md")
                )
                .unwrap(),
            "docs"
        );
    }

    #[test]
    fn credential_patterns_are_denied() {
        let policy = FileAccessPolicy::new(vec![root("docs", "/data")]);
        for (relative, name) in [
            (".env", ".env"),
            ("config/.env.local", ".env.local"),
            (".ssh/id_rsa", "id_rsa"),
            ("keys/server.pem", "server.pem"),
            ("certs/client.p12", "client.p12"),
            ("aws/credentials.json", "credentials.json"),
            (".ssh/known_hosts", "known_hosts"),
        ] {
            assert!(policy.is_denied(relative, name), "应拒绝: {relative}");
        }
        for (relative, name) in [
            ("notes/docker.md", "docker.md"),
            ("notes/jenkins-notes.md", "jenkins-notes.md"),
            ("data/report.json", "report.json"),
            // 裸词模式只做全等：正常文件名不得被后缀误拒（审查 V6-SEC-005）。
            ("notes/my-shadow.md", "my-shadow.md"),
            ("notes/logshadow", "logshadow"),
            ("notes/keychain-notes.md", "keychain-notes.md"),
            ("notes/my_id_rsa_notes.md", "my_id_rsa_notes.md"),
            ("notes/ssh-setup.md", "ssh-setup.md"),
        ] {
            assert!(!policy.is_denied(relative, name), "不应拒绝: {relative}");
        }
    }

    #[test]
    fn denied_messages_never_leak_content() {
        for denial in [
            FileAccessDenied::NoRootsConfigured,
            FileAccessDenied::OutsideAllowedRoots,
            FileAccessDenied::Traversal,
            FileAccessDenied::SymlinkEscape,
            FileAccessDenied::DeniedPattern,
            FileAccessDenied::NotFound,
            FileAccessDenied::NotAFile,
            FileAccessDenied::TooLarge,
            FileAccessDenied::NotText,
        ] {
            assert!(!denial.as_str().is_empty());
            assert!(!denial.message().is_ascii());
        }
    }

    #[test]
    fn display_path_strips_windows_verbatim_prefix() {
        assert_eq!(display_path(r"\\?\D:\资料\a.md"), r"D:\资料\a.md");
        assert_eq!(
            display_path(r"\\?\UNC\server\share\a.md"),
            r"\\server\share\a.md"
        );
        assert_eq!(display_path("/data/a.md"), "/data/a.md");
    }

    #[test]
    fn metadata_round_trips_with_stable_file_id() {
        let metadata = FileMetadata {
            file_id: file_id("docs", "notes/a.md"),
            root_id: "docs".into(),
            path: PathBuf::from("/data/notes/a.md").to_string_lossy().into(),
            relative_path: "notes/a.md".into(),
            file_name: "a.md".into(),
            extension: Some("md".into()),
            size_bytes: 10,
            modified_at: 5,
            indexed_at: 6,
            content_kind: FileContentKind::Text,
            restricted: false,
            index_error: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        let back: FileMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back, metadata);
        assert_eq!(metadata.file_id, file_id("docs", "notes/a.md"));
        assert_ne!(metadata.file_id, file_id("other", "notes/a.md"));
    }
}
