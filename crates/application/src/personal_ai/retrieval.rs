//! 检索增强端口（V6 §22/§55）——PersonalAgent 的**通用**可选 stage。
//!
//! 平台性能力，不含任何业务语义：Agent 只问「本次对话是否需要一段知识上下文」，
//! 由实现（`KnowledgeRetrievalService`）决定检索哪里、返回什么。
//!
//! 失败与无命中一律降级为 `None`（V6 Principle 7）：
//! Memory / Document / File 索引不可用时，普通功能与普通问答完全不受影响。

use async_trait::async_trait;

use devtoolbox_core::personal_ai::AppContext;

#[async_trait]
pub trait RetrievalAugmenter: Send + Sync {
    /// 返回 `Some(text)` → 追加为 system prompt 的知识段；`None` → 本次不注入。
    async fn augment(&self, query: &str, app_context: &AppContext) -> Option<String>;
}
