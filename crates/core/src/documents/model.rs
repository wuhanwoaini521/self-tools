//! Documents 纯数据契约（V6 Track B，§27-§39）。
//!
//! Documents 与 Files 是**两件事**（V6 §40）：
//! - Documents = 已进入 self-tools 知识系统的文档（抽取 + 分块 + 内容检索）；
//! - Files = 允许根内的文件实体（元数据 + 安全读取 + 打开）。
//! 二者物理隔离（不同表、不同端口），本模块只描述 Documents 侧。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 文档类型（V6 §34：第一版只保证 Markdown / TXT / JSON；PDF 视抽取能力）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Markdown,
    Text,
    Json,
    Pdf,
    Other,
}

impl DocumentType {
    pub const ALL: [DocumentType; 5] = [
        DocumentType::Markdown,
        DocumentType::Text,
        DocumentType::Json,
        DocumentType::Pdf,
        DocumentType::Other,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentType::Markdown => "markdown",
            DocumentType::Text => "text",
            DocumentType::Json => "json",
            DocumentType::Pdf => "pdf",
            DocumentType::Other => "other",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            DocumentType::Markdown => "Markdown",
            DocumentType::Text => "文本",
            DocumentType::Json => "JSON",
            DocumentType::Pdf => "PDF",
            DocumentType::Other => "其他",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw.trim().to_ascii_lowercase())
    }
}

/// 可见性（V6 §72）：为未来权限扩展保留语义位，V6 不做策略分支。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentVisibility {
    #[default]
    Normal,
    Private,
}

/// chunk 在原文中的位置（§37 引用能力的基础）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentLocation {
    /// 最近的 Markdown 标题（无则 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// 页码（PDF；当前抽取器不提供 → None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// 字符区间（UTF-8 字符索引，左闭右开）。
    pub char_start: usize,
    pub char_end: usize,
}

impl DocumentLocation {
    /// 人类可读位置描述（`§标题` / `第 N 页` / `字符 a-b`）。
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(page) = self.page {
            parts.push(format!("第 {page} 页"));
        }
        if let Some(section) = self.section.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("§{section}"));
        }
        parts.push(format!("字符 {}-{}", self.char_start, self.char_end));
        parts.join(" · ")
    }
}

/// 统一分块单元（V6 §32）。`text` 为原文切片（不重写、不改写）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub document_id: String,
    pub chunk_id: String,
    /// 顺序号（0 起）。
    pub ordinal: usize,
    pub text: String,
    pub location: DocumentLocation,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

/// 文档索引元数据（V6 §30：search 返回 id/title/type/modified/snippet/source）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub document_id: String,
    /// 所属允许根 id。
    pub root_id: String,
    pub title: String,
    pub document_type: DocumentType,
    /// 绝对路径（本地只读资料）。
    pub path: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub indexed_at: i64,
    pub chunk_count: usize,
    /// 内容是否可检索（PDF 未抽取 / 超大文件 / 解析失败 → false，仅元数据）。
    pub content_available: bool,
    #[serde(default)]
    pub visibility: DocumentVisibility,
    /// 索引失败原因（§91 单文件失败不拖垮整轮索引）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_error: Option<String>,
}

impl DocumentMeta {
    #[must_use]
    pub fn is_indexed_content(&self) -> bool {
        self.content_available && self.index_error.is_none()
    }
}

/// 分块配置（V6 §33：集中配置，不散落硬编码）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChunkConfig {
    /// 目标 chunk 字符数。
    pub target_chars: usize,
    /// 相邻 chunk 重叠字符数（保持上下文连续）。
    pub overlap_chars: usize,
    /// 超过此大小只索引元数据（§92 大文件阈值）。
    pub max_document_bytes: u64,
    /// chunk 数上限（与 `max_document_bytes` 共同封顶内存）。
    pub max_chunks: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_chars: 1_200,
            overlap_chars: 200,
            max_document_bytes: 2_000_000,
            max_chunks: 2_000,
        }
    }
}

/// 读取请求（V6 §31：绝不默认整份读取）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentReadRequest {
    /// 指定 chunk（优先级最高）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    /// 指定章节（标题精确/子串匹配）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// 起始字符（未指定 chunk/section 时生效）。
    pub offset: usize,
    /// 返回字符上限。
    pub max_chars: usize,
}

impl Default for DocumentReadRequest {
    fn default() -> Self {
        Self {
            chunk_id: None,
            section: None,
            offset: 0,
            max_chars: 2_000,
        }
    }
}

/// 读取结果：文本 + 位置 + 截断标记（引用能力）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentReadResult {
    pub document_id: String,
    pub title: String,
    pub text: String,
    pub location: DocumentLocation,
    pub chunk_ids: Vec<String>,
    pub total_chunks: usize,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// 索引 / 检索共享数据（infra 实现、application 消费 —— 与 history_records 同理）
// ---------------------------------------------------------------------------

/// 索引库里的文档指纹（增量索引判定，V6 §90）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentFingerprint {
    pub document_id: String,
    pub root_id: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

/// 词法粗筛命中（精排在 application：标题命中 > 正文命中）。
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentHit {
    pub meta: DocumentMeta,
    /// 命中 chunk（正文命中时存在）。
    pub chunk_id: Option<String>,
    pub location: Option<String>,
    pub snippet: String,
    /// 命中标题而非正文。
    pub matched_in_title: bool,
}

/// 索引统计（观测用；只计数，V6 §94）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentIndexStats {
    pub documents: usize,
    pub chunks: usize,
    pub content_available: usize,
    pub metadata_only: usize,
    pub failed: usize,
}

/// 允许根内扫描到的候选文档（尚未抽取）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedDocument {
    pub path: std::path::PathBuf,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

/// 抽取结果：文本，或「只有元数据」+ 原因（PDF / 超大 / 编码失败）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractedContent {
    Text(String),
    /// 无法抽取正文时给出简短原因（面向用户，不含正文）。
    MetadataOnly(String),
}

/// 从路径推断文档类型。
#[must_use]
pub fn detect_document_type(path: &Path) -> DocumentType {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" => DocumentType::Markdown,
        "txt" | "text" | "log" | "csv" | "tsv" => DocumentType::Text,
        "json" | "jsonl" | "ndjson" => DocumentType::Json,
        "pdf" => DocumentType::Pdf,
        _ => DocumentType::Other,
    }
}

/// 是否属于 V6 默认纳入索引的文档类型（§34）。
#[must_use]
pub fn is_indexable_document(path: &Path) -> bool {
    matches!(
        detect_document_type(path),
        DocumentType::Markdown | DocumentType::Text | DocumentType::Json | DocumentType::Pdf
    )
}

/// 文档 id（稳定：`root_id` + 相对路径）。
#[must_use]
pub fn document_id(root_id: &str, relative_path: &str) -> String {
    crate::knowledge::stable_id("doc", &[root_id, relative_path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_type_detection() {
        assert_eq!(
            detect_document_type(Path::new("a/b/notes.md")),
            DocumentType::Markdown
        );
        assert_eq!(
            detect_document_type(Path::new("x.MARKDOWN")),
            DocumentType::Markdown
        );
        assert_eq!(detect_document_type(Path::new("a.txt")), DocumentType::Text);
        assert_eq!(detect_document_type(Path::new("a.json")), DocumentType::Json);
        assert_eq!(detect_document_type(Path::new("a.pdf")), DocumentType::Pdf);
        assert_eq!(
            detect_document_type(Path::new("a.docx")),
            DocumentType::Other
        );
        assert!(is_indexable_document(Path::new("a.pdf")));
        assert!(!is_indexable_document(Path::new("a.docx")));
    }

    #[test]
    fn document_ids_are_stable_and_root_scoped() {
        let a = document_id("root1", "notes/jenkins.md");
        assert_eq!(a, document_id("root1", "notes/jenkins.md"));
        assert_ne!(a, document_id("root2", "notes/jenkins.md"));
        assert!(a.starts_with("doc-"));
    }

    #[test]
    fn chunk_config_defaults_are_consistent() {
        let config = ChunkConfig::default();
        assert!(config.target_chars > config.overlap_chars);
        assert_eq!(config.target_chars, 1_200);
        assert!(config.max_document_bytes > 0);
    }

    #[test]
    fn location_describe_includes_available_parts() {
        let location = DocumentLocation {
            section: Some("Environment".into()),
            page: None,
            char_start: 10,
            char_end: 40,
        };
        assert_eq!(location.describe(), "§Environment · 字符 10-40");
        let paged = DocumentLocation {
            section: None,
            page: Some(3),
            char_start: 0,
            char_end: 5,
        };
        assert!(paged.describe().starts_with("第 3 页"));
    }
}
