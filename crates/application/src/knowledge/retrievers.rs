//! 知识源检索器：把三个域服务适配为统一 `KnowledgeSourceRetriever`（V6 §53）。
//!
//! 每个检索器只做「调用域服务 + 转成 `KnowledgeResult`（带 provenance）」，
//! 不含任何排序 / 预算逻辑（那是 `KnowledgeRetrievalService` 的职责）。

use std::sync::Arc;

use devtoolbox_core::documents::DocumentType;
use devtoolbox_core::knowledge::{KnowledgeResult, KnowledgeSourceKind};
use devtoolbox_core::settings::KnowledgeSettings;

use crate::documents::service::DocumentService;
use crate::error::ApplicationError;
use crate::files::ports::FileQuery;
use crate::files::service::FileService;
use crate::knowledge::ports::KnowledgeSourceRetriever;
use crate::memory::service::MemoryService;

/// Memory 检索器（只返回 ACTIVE + 非敏感，§22/§69）。
pub struct MemoryRetriever {
    service: Arc<MemoryService>,
}

impl MemoryRetriever {
    #[must_use]
    pub fn new(service: Arc<MemoryService>) -> Self {
        Self { service }
    }
}

impl KnowledgeSourceRetriever for MemoryRetriever {
    fn kind(&self) -> KnowledgeSourceKind {
        KnowledgeSourceKind::Memory
    }

    fn retrieve(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeResult>, ApplicationError> {
        let items = self.service.search(query, None, limit)?;
        Ok(self.service.to_knowledge_results(&items, query))
    }
}

/// Documents 检索器（词法命中 + 位置；§30）。
pub struct DocumentRetriever {
    service: Arc<DocumentService>,
}

impl DocumentRetriever {
    #[must_use]
    pub fn new(service: Arc<DocumentService>) -> Self {
        Self { service }
    }
}

impl KnowledgeSourceRetriever for DocumentRetriever {
    fn kind(&self) -> KnowledgeSourceKind {
        KnowledgeSourceKind::Document
    }

    fn retrieve(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeResult>, ApplicationError> {
        let hits = self.service.search(query, None::<DocumentType>, limit)?;
        Ok(self.service.to_knowledge_results(&hits, query))
    }
}

/// Files 检索器（允许根内；§48）。设置每次调用读取，改配置立即生效。
pub struct FileRetriever {
    service: Arc<FileService>,
    settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
}

impl FileRetriever {
    #[must_use]
    pub fn new(
        service: Arc<FileService>,
        settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self { service, settings }
    }
}

impl KnowledgeSourceRetriever for FileRetriever {
    fn kind(&self) -> KnowledgeSourceKind {
        KnowledgeSourceKind::File
    }

    fn retrieve(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeResult>, ApplicationError> {
        let settings = (self.settings)();
        let spec = FileQuery {
            query: query.to_string(),
            limit,
            ..FileQuery::default()
        };
        let entries = self.service.search(&settings, &spec)?;
        Ok(self.service.to_knowledge_results(&entries, query))
    }
}
