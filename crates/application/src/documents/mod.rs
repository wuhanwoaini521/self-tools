//! Documents 域（V6 Track B）。

pub mod ports;
pub mod service;

pub use ports::{
    DocumentFingerprint, DocumentHit, DocumentIndexPort, DocumentIndexStats, DocumentSourcePort,
    DocumentStoreError, ExtractedContent, ScannedDocument,
};
pub use service::{DocumentConfig, DocumentService, IndexReport, document_score};

#[cfg(test)]
mod tests;
