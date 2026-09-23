//! Knowledge 检索端口与模式（V6 Track D，§52-§56）。
//!
//! `KnowledgeSourceRetriever` 是**唯一**的知识源接入点：Memory / Documents / Files
//! 各自实现它，`KnowledgeRetrievalService` 只做合并 / 去重 / 排序 / 预算，
//! 不感知任何业务语义（§56：不新建 KnowledgeAgent / RetrievalAgent）。

use devtoolbox_core::knowledge::{KnowledgeResult, KnowledgeSourceKind};

use crate::error::ApplicationError;

/// 单个知识源的检索器。
pub trait KnowledgeSourceRetriever: Send + Sync {
    /// 该检索器负责的知识源。
    fn kind(&self) -> KnowledgeSourceKind;

    /// 执行检索（返回候选，可多于最终上限；合并层负责截断）。
    fn retrieve(&self, query: &str, limit: usize)
    -> Result<Vec<KnowledgeResult>, ApplicationError>;
}

/// 检索模式：决定「哪些源参与、每源多少条」（V6 §22/§64/§68）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrievalMode {
    /// 自动注入（对话前置 stage）：memory ≤ 5、document ≤ 3、file 默认 0。
    Augment,
    /// 显式工具调用（`knowledge.search`）：四类源都参与。
    Explicit,
}
