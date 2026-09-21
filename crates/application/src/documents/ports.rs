//! Documents 索引与来源端口（V6 Track B）。
//!
//! 端口属于用例层（application）：SQLite 索引库（`config/documents.db`）与文件系统
//! 扫描 / 内容抽取（infrastructure）由组合根装配。用例层不感知 SQL 与文件系统细节。
//!
//! 只读边界：来源端口只提供「扫描 + 抽取」，**没有**任何写文件能力（V6 Principle 6）。

use std::fmt;
use std::path::Path;

use devtoolbox_core::documents::{DocumentChunk, DocumentMeta, DocumentType};
use devtoolbox_core::files::KnowledgeRoot;

/// Documents 基础设施错误（适配层已转换为可显示文本；不含正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStoreError(pub String);

impl fmt::Display for DocumentStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DocumentStoreError {}

// 索引 / 检索的共享数据形状由 core 持有（infra 实现、application 消费，
// 与 `history_records` / `workspace` 同理），此处重导出以保持调用面稳定。
pub use devtoolbox_core::documents::{
    DocumentFingerprint, DocumentHit, DocumentIndexStats, ExtractedContent, ScannedDocument,
};

/// 文档索引端口（SQLite `config/documents.db`）。
pub trait DocumentIndexPort: Send + Sync {
    /// 写入/更新文档元数据（按 `document_id` 幂等）。
    fn upsert(&self, meta: &DocumentMeta) -> Result<(), DocumentStoreError>;

    /// 整体替换某文档的 chunks（先删后插，保证不残留旧版本，V6 §88）。
    fn replace_chunks(
        &self,
        document_id: &str,
        chunks: &[DocumentChunk],
    ) -> Result<(), DocumentStoreError>;

    fn get(&self, document_id: &str) -> Result<Option<DocumentMeta>, DocumentStoreError>;

    /// 按顺序号返回全部 chunk。
    fn chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>, DocumentStoreError>;

    /// 关键词候选（实现做 LIKE 粗筛即可；每个关键词都取候选后并集）。
    fn search_candidates(
        &self,
        keywords: &[String],
        document_type: Option<DocumentType>,
        limit: usize,
    ) -> Result<Vec<DocumentHit>, DocumentStoreError>;

    /// 最近索引的文档。
    fn recent(&self, limit: usize) -> Result<Vec<DocumentMeta>, DocumentStoreError>;

    /// 某根下已索引文档的指纹（增量索引用）。
    fn fingerprints(&self, root_id: &str) -> Result<Vec<DocumentFingerprint>, DocumentStoreError>;

    /// 移除某文档（索引清理，不是文件系统删除：文件消失后调用）。
    fn remove(&self, document_id: &str) -> Result<(), DocumentStoreError>;

    fn stats(&self) -> Result<DocumentIndexStats, DocumentStoreError>;
}

/// 文档来源端口：文件系统扫描 + 内容抽取（V6 §27/§35）。
///
/// 扫描**只能**发生在允许根内；抽取是只读的，绝不写入/移动文件。
pub trait DocumentSourcePort: Send + Sync {
    /// 递归扫描允许根内的候选文档（跳过噪音目录、只保留可索引类型）。
    /// 返回 `(文件列表, 是否因上限被截断)`。
    fn scan_root(
        &self,
        root: &KnowledgeRoot,
        limit: usize,
    ) -> Result<(Vec<ScannedDocument>, bool), DocumentStoreError>;

    /// 抽取正文（超限/不支持时返回 `MetadataOnly`，不报错）。
    fn extract(
        &self,
        path: &Path,
        document_type: DocumentType,
        max_bytes: u64,
    ) -> Result<ExtractedContent, DocumentStoreError>;
}
