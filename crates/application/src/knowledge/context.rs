//! Knowledge Context 聚合（V6 §63/§64）。
//!
//! 结构化聚合而不是字符串拼接：`app_context` / `module_context` / `memories` /
//! `document_refs` / `file_refs` 各自独立，只有 `render_for_prompt` 一处渲染，
//! 并受 `KnowledgeBudget::max_chars` 硬截断。

use devtoolbox_core::knowledge::{KnowledgeDiagnostics, KnowledgeResult, KnowledgeSourceKind};
use devtoolbox_core::personal_ai::AppContext;
use serde::{Deserialize, Serialize};

use crate::personal_ai::context::ContextBundle;

/// 因预算被省略的条数（不隐藏事实：UI / 日志可如实展示）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OmittedSummary {
    pub memories: usize,
    pub documents: usize,
    pub files: usize,
}

impl OmittedSummary {
    #[must_use]
    pub fn total(&self) -> usize {
        self.memories + self.documents + self.files
    }
}

/// 统一个人知识上下文（V6 §63）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeContext {
    pub app_context: AppContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_context: Option<ContextBundle>,
    pub memories: Vec<KnowledgeResult>,
    pub document_refs: Vec<KnowledgeResult>,
    pub file_refs: Vec<KnowledgeResult>,
    pub omitted: OmittedSummary,
    pub diagnostics: KnowledgeDiagnostics,
}

impl Default for KnowledgeContext {
    fn default() -> Self {
        Self {
            app_context: AppContext::default(),
            module_context: None,
            memories: Vec::new(),
            document_refs: Vec::new(),
            file_refs: Vec::new(),
            omitted: OmittedSummary::default(),
            diagnostics: KnowledgeDiagnostics::default(),
        }
    }
}

impl KnowledgeContext {
    #[must_use]
    pub fn new(app_context: AppContext) -> Self {
        Self {
            app_context,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty() && self.document_refs.is_empty() && self.file_refs.is_empty()
    }

    #[must_use]
    pub fn total_items(&self) -> usize {
        self.memories.len() + self.document_refs.len() + self.file_refs.len()
    }

    /// 按来源分类（合并层用）。
    pub fn push(&mut self, result: KnowledgeResult) {
        match result.source_type {
            KnowledgeSourceKind::Memory => self.memories.push(result),
            KnowledgeSourceKind::Document => self.document_refs.push(result),
            KnowledgeSourceKind::File => self.file_refs.push(result),
            KnowledgeSourceKind::Module => self.document_refs.push(result),
        }
    }

    /// 渲染为 prompt 文本（唯一渲染入口；硬截断）。
    #[must_use]
    pub fn render_for_prompt(&self, max_chars: usize) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut lines: Vec<String> = vec![
            "[个人知识检索]（按相关性排序；每条都带来源，仅用于回答本次问题）".to_string(),
        ];
        for result in self
            .memories
            .iter()
            .chain(self.document_refs.iter())
            .chain(self.file_refs.iter())
        {
            let location = result
                .location
                .as_deref()
                .map(|location| format!(" · {location}"))
                .unwrap_or_default();
            lines.push(format!(
                "- [{}] {} [{}]{}: {}",
                result.source_type.label(),
                result.title,
                result.source_id,
                location,
                result.snippet.replace('\n', " ")
            ));
        }
        if self.omitted.total() > 0 {
            lines.push(format!(
                "（另有 {} 条因上下文预算未列出：记忆 {} / 文档 {} / 文件 {}）",
                self.omitted.total(),
                self.omitted.memories,
                self.omitted.documents,
                self.omitted.files
            ));
        }
        lines.push(
            "规则：只能依据以上内容回答个人资料相关问题；没有命中的内容必须明说「没有找到」，禁止编造。"
                .to_string(),
        );
        truncate_chars(&lines.join("\n"), max_chars)
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n…[已按上下文预算截断]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(kind: KnowledgeSourceKind, id: &str, title: &str, snippet: &str) -> KnowledgeResult {
        KnowledgeResult::new(kind, id, title, snippet, 0.8).with_location("§环境")
    }

    #[test]
    fn push_classifies_by_source_kind() {
        let mut context = KnowledgeContext::new(AppContext::default());
        context.push(result(KnowledgeSourceKind::Memory, "m1", "偏好", "喜欢历史"));
        context.push(result(KnowledgeSourceKind::Document, "d1", "Jenkins", "记录"));
        context.push(result(KnowledgeSourceKind::File, "f1", "a.pdf", "大小 1 MB"));
        assert_eq!(context.memories.len(), 1);
        assert_eq!(context.document_refs.len(), 1);
        assert_eq!(context.file_refs.len(), 1);
        assert_eq!(context.total_items(), 3);
        assert!(!context.is_empty());
    }

    #[test]
    fn render_includes_provenance_and_omissions() {
        let mut context = KnowledgeContext::new(AppContext::default());
        context.push(result(KnowledgeSourceKind::Memory, "mem-1", "偏好", "喜欢历史旅行"));
        context.omitted.documents = 2;
        let text = context.render_for_prompt(4_000);
        assert!(text.contains("[记忆]"));
        assert!(text.contains("[mem-1]"));
        assert!(text.contains("§环境"));
        assert!(text.contains("另有 2 条"));
        assert!(text.contains("没有找到"));
    }

    #[test]
    fn render_is_truncated_at_budget() {
        let mut context = KnowledgeContext::new(AppContext::default());
        for index in 0..50 {
            context.push(result(
                KnowledgeSourceKind::Document,
                &format!("doc-{index}"),
                "标题",
                &"内容".repeat(50),
            ));
        }
        let text = context.render_for_prompt(200);
        assert!(text.chars().count() < 300);
        assert!(text.contains("上下文预算截断"));
    }

    #[test]
    fn empty_context_renders_empty() {
        let context = KnowledgeContext::new(AppContext::default());
        assert!(context.is_empty());
        assert!(context.render_for_prompt(4_000).is_empty());
    }
}
