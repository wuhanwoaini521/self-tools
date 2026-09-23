//! 备份/恢复域的数据模型（V11 §64-§70、§160）。
//!
//! 边界：本模块是**纯数据 + 纯函数**（可单测、可序列化）：描述「备份是什么、
//! 恢复要守住什么」。目录创建、快照、文件落盘在基础设施与用例层
//! （`application::backup`），core 不碰 IO。
//!
//! 三条不变量（恢复与演练都按此断言）：
//! 1. **清单是唯一真相**：恢复只按 [`BackupManifest`] 的条目写文件，绝不全量回拷
//!    备份目录，因此清单外的文件一律不受影响；
//! 2. **路径封闭**：条目路径是备份目录内的相对路径（只允许普通组件），
//!    恢复时 `..` / 绝对路径一律拒绝（与 V6 `FileAccessPolicy::contains_traversal`
//!    同一语义，§44）；
//! 3. **摘要可复核**：每条目带 SHA-256 内容摘要，恢复前后逐条校验，校验不过不写盘。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 备份条目对应的存储类型（决定快照方式与恢复后的校验强度）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// SQLite 数据库文件（必须安全快照，见模块文档）。
    Sqlite,
    /// DuckDB 数据库文件。
    DuckDb,
    /// JSON / 配置文件。
    Json,
    /// 其他本地文件（未知类型的兜底，保证旧清单可解析）。
    #[default]
    #[serde(other)]
    Other,
}

impl EntryKind {
    /// 稳定文本（清单 / 报告 / UI 显示；与 serde 表示一致）。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::DuckDb => "duck_db",
            Self::Json => "json",
            Self::Other => "other",
        }
    }

    /// 是否数据库类（需要「安全快照」，禁止裸拷活动库）。
    #[must_use]
    pub fn is_database(self) -> bool {
        matches!(self, Self::Sqlite | Self::DuckDb)
    }
}

/// 备份中的一个文件条目（相对路径 + 复核信息）。
///
/// `path` 是**备份目录内的相对路径**（只含普通组件），不等于活动库的绝对路径：
/// 恢复目的目录由调用方决定，清单因此可以在任意隔离目录里重放。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupEntry {
    /// 备份目录内的相对路径（例如 `memory.db`、`settings.json`）。
    pub path: String,
    /// 存储类型（决定快照与恢复策略）。
    pub kind: EntryKind,
    /// 文件内容的 SHA-256 十六进制摘要（见 [`sha256_hex`]）。
    pub sha256: String,
    /// 字节数（与摘要一起构成完整性双校验）。
    pub bytes: u64,
    /// 是否可能含敏感数据（凭据 / 个人数据）：外发备份前必须显式确认。
    pub sensitive: bool,
    /// 是否可从其他来源重建（索引 / 缓存）：可重建项默认不进备份。
    pub rebuildable: bool,
}

impl BackupEntry {
    /// 构造条目（路径与摘要由调用方给定；`kind` 决定恢复策略）。
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        kind: EntryKind,
        sha256: impl Into<String>,
        bytes: u64,
        sensitive: bool,
        rebuildable: bool,
    ) -> Self {
        Self {
            path: path.into(),
            kind,
            sha256: sha256.into(),
            bytes,
            sensitive,
            rebuildable,
        }
    }

    /// 条目是否与给定内容一致（摘要 + 体积双校验）。
    ///
    /// 用于恢复后复核，也用于清单与备份文件的比对。
    #[must_use]
    pub fn matches(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes.len() as u64 && self.sha256 == sha256_hex(bytes)
    }
}

/// 备份清单：一次备份的全部真相（V11 §160）。
///
/// 字段缺失时按默认值反序列化（`#[serde(default)]`）：旧版本清单 / 手工增删
/// 字段都不会让恢复直接失败，缺项表现为空集合，由恢复流程逐条判定。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupManifest {
    /// 备份时间（Unix epoch 秒）。
    pub created_at: i64,
    /// 产生备份的应用版本（升级兼容性判断）。
    pub app_version: String,
    /// 来源 id → 该来源的 schema 版本（恢复前的兼容校验）。
    pub schema_versions: BTreeMap<String, u32>,
    /// 文件条目（固定顺序，恢复按序执行便于复现报告）。
    pub entries: Vec<BackupEntry>,
    /// 备注（演练原因 / 值班记录等）。
    pub note: String,
}

impl BackupManifest {
    /// 序列化为 JSON 文本（写入 `manifest.json`）。
    ///
    /// # Errors
    /// 含不可序列化内容时返回序列化错误（当前结构不会发生）。
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 从 JSON 文本解析清单。
    ///
    /// # Errors
    /// 文本不是合法 JSON、或关键字段类型不符时返回解析错误。
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// 按相对路径查条目。
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<&BackupEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// 条目总数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否空清单（没有任何来源产出快照）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 备份总体积（字节；观测与容量规划）。
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|entry| entry.bytes).sum()
    }

    /// 是否含敏感条目（外发 / 分享备份前的门禁）。
    #[must_use]
    pub fn has_sensitive_entries(&self) -> bool {
        self.entries.iter().any(|entry| entry.sensitive)
    }
}

/// 一次恢复的执行报告（V11 §160：演练要看结果，不能只看「成功」两个字）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RestoreReport {
    /// 已从备份写入隔离目的目录的条目路径（写入顺序）。
    pub restored: Vec<String>,
    /// 落盘后重新校验通过的条目路径（摘要 / 体积双过）。
    pub verified: Vec<String>,
    /// 失败条目及原因（路径非法、摘要不符、读取失败等）。
    pub failed: Vec<String>,
    /// 整体完整性：无失败项才算通过。
    pub integrity_ok: bool,
}

impl RestoreReport {
    /// 恢复是否完整成功（无失败项）。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.integrity_ok && self.failed.is_empty()
    }

    /// 演练用摘要文本（`restored=N verified=N failed=N`）。
    #[must_use]
    pub fn summary(&self) -> String {
        let mut out = String::with_capacity(64);
        let _ = write!(out, "restored={}", self.restored.len());
        let _ = write!(out, " verified={}", self.verified.len());
        let _ = write!(out, " failed={}", self.failed.len());
        let _ = write!(out, " integrity_ok={}", self.integrity_ok);
        out
    }
}

/// 计算内容的 SHA-256 十六进制摘要（小写）。
///
/// 确定性：同内容跨进程、跨运行得到同一摘要。大文件请由实现层流式计算
/// （`infrastructure::backup` 用 `Sha256` + 分块读，不整份进内存）。
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// 备份清单内的条目路径是否安全（相对、无逃逸）。
///
/// 拒绝：空路径、`..`（traversal）、绝对路径、盘符 / 根前缀、`.` 段。
///
/// **跨平台**：`Path::components()` 在 Unix 上会把 `C:/Windows/x.db` 当成单个
/// Normal 组件（没有 `/` 分隔），因此盘符 / UNC / 反斜杠根必须显式拒绝 ——
/// 否则 Windows 风格的绝对路径在 macOS/Linux 上会被误判为「安全」。
#[must_use]
pub fn is_safe_backup_path(relative: &str) -> bool {
    if relative.is_empty() {
        return false;
    }
    // Windows 绝对路径：`C:\...` / `C:/...` / `\\server\share` / `\\?\...`。
    let bytes = relative.as_bytes();
    let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if has_drive || relative.starts_with('\\') {
        return false;
    }
    // 反斜杠分隔在 Windows 上是分隔符：先归一化再判，避免 `..\..\x` 逃逸。
    let normalized = relative.replace('\\', "/");
    Path::new(&normalized)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> BackupManifest {
        BackupManifest {
            created_at: 1_700_000_000,
            app_version: "0.1.0".to_string(),
            schema_versions: BTreeMap::from([("memory".to_string(), 1u32)]),
            entries: vec![BackupEntry::new(
                "memory.db",
                EntryKind::Sqlite,
                sha256_hex(b"fixture"),
                7,
                true,
                false,
            )],
            note: "drill".to_string(),
        }
    }

    #[test]
    fn manifest_json_round_trip_preserves_everything() {
        let manifest = sample_manifest();
        let text = manifest.to_json().expect("encode");
        let decoded = BackupManifest::from_json(&text).expect("decode");
        assert_eq!(decoded, manifest);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded.total_bytes(), 7);
        assert!(decoded.has_sensitive_entries());
        assert_eq!(
            decoded.entry("memory.db").map(|entry| entry.kind),
            Some(EntryKind::Sqlite)
        );
        assert!(decoded.entry("missing.db").is_none());
    }

    #[test]
    fn manifest_decodes_missing_fields_with_defaults() {
        let manifest = BackupManifest::from_json(r#"{"entries": []}"#).expect("decode");
        assert!(manifest.is_empty());
        assert_eq!(manifest.created_at, 0);
        assert!(manifest.schema_versions.is_empty());
        assert!(manifest.note.is_empty());
    }

    #[test]
    fn entry_kind_serde_names_are_stable() {
        let manifest = BackupManifest {
            entries: vec![BackupEntry::new(
                "a.duckdb",
                EntryKind::DuckDb,
                "x",
                1,
                false,
                false,
            )],
            ..BackupManifest::default()
        };
        let text = manifest.to_json().expect("encode");
        assert!(text.contains("\"duck_db\""), "{text}");
        assert_eq!(EntryKind::DuckDb.as_str(), "duck_db");
        assert!(EntryKind::DuckDb.is_database());
        assert!(EntryKind::Sqlite.is_database());
        assert!(!EntryKind::Json.is_database());
        // 未知 kind 退化为 Other（向前兼容，恢复不直接失败）。
        let fallback: EntryKind = serde_json::from_str("\"oracle\"").expect("decode unknown kind");
        assert_eq!(fallback, EntryKind::Other);
    }

    #[test]
    fn sha256_matches_known_vector_and_detects_changes() {
        // 已知向量：SHA-256("abc")。
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b""), sha256_hex(b""));
        // 单字节差异必须可检测。
        assert_ne!(sha256_hex(b"backup payload"), sha256_hex(b"backup payloae"));
        // 空输入摘要稳定且非空。
        assert_eq!(sha256_hex(b"").len(), 64);
    }

    #[test]
    fn entry_matches_requires_digest_and_length() {
        let entry = BackupEntry::new(
            "a.json",
            EntryKind::Json,
            sha256_hex(b"payload"),
            7,
            false,
            false,
        );
        assert!(entry.matches(b"payload"));
        assert!(!entry.matches(b"payloae"));
        assert!(!entry.matches(b"payload-extra"));
        // 空快照照常比对（空文件也有稳定摘要）。
        let empty = BackupEntry::new("b.json", EntryKind::Json, sha256_hex(b""), 0, false, false);
        assert!(empty.matches(b""));
        assert!(!empty.matches(b"x"));
    }

    #[test]
    fn safe_backup_path_rejects_traversal_and_absolute_paths() {
        assert!(is_safe_backup_path("memory.db"));
        assert!(is_safe_backup_path("nested/memory.db"));
        assert!(!is_safe_backup_path(""));
        assert!(!is_safe_backup_path("../outside.db"));
        assert!(!is_safe_backup_path("nested/../../outside.db"));
        assert!(!is_safe_backup_path("/etc/passwd"));
        assert!(!is_safe_backup_path("C:/Windows/system.db"));
        assert!(!is_safe_backup_path("./memory.db"));
    }

    #[test]
    fn safe_backup_path_rejects_windows_forms_on_every_platform() {
        // 回归：`Path::components()` 在 Unix 上把 `C:/x` 当普通组件 → 必须显式拒。
        for bad in [
            "c:\\windows\\system.db",
            "C:/Windows/system.db",
            "\\\\server\\share\\x.db",
            "\\\\?\\C:\\x.db",
            "..\\..\\outside.db",
            "nested\\..\\..\\outside.db",
            ".\\memory.db",
        ] {
            assert!(
                !is_safe_backup_path(bad),
                "Windows 风格路径必须在所有平台被拒: {bad}"
            );
        }
        // Windows 相对路径（反斜杠分隔但无逃逸）保持可用。
        assert!(is_safe_backup_path("nested\\memory.db"));
    }

    #[test]
    fn restore_report_summary_counts_entries() {
        let mut report = RestoreReport::default();
        assert!(!report.is_ok());
        report.restored.push("memory.db".to_string());
        report.verified.push("memory.db".to_string());
        report.integrity_ok = true;
        assert!(report.is_ok());
        assert_eq!(
            report.summary(),
            "restored=1 verified=1 failed=0 integrity_ok=true"
        );
    }
}
