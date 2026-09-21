//! App Context 解析（V4 §22-§26，P0）。
//!
//! Frontend 负责「我在哪」（`AppContext`），业务模块的 `ModuleContextProvider`
//! 负责「这个 entity 的业务上下文」。`ContextBudget` 保证不把整个数据库
//! 塞给模型（current entity first → page context → direct relations → extras）。

use devtoolbox_core::AgentError;
use devtoolbox_core::personal_ai::AppContext;
use serde::{Deserialize, Serialize};

/// 上下文预算：硬截断，不做 token optimizer（V4 §26）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContextBudget {
    /// 列表类内容最大条目数。
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    /// 上下文文本最大字符数（UTF-8）。
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
}

fn default_max_items() -> usize {
    30
}
fn default_max_chars() -> usize {
    6000
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_items: default_max_items(),
            max_chars: default_max_chars(),
        }
    }
}

/// 模块上下文提供方输出：结构化 compact context。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContextBundle {
    /// 模块 id。
    pub module: String,
    /// 人类可读主行（如「History · 毛泽东（1893–1976）」）。
    pub headline: String,
    /// 给模型的紧凑结构化上下文。
    pub summary: serde_json::Value,
}

/// 模块上下文提供方端口。同步执行（快速查询，遵守「工具必须自限时间」）。
pub trait ModuleContextProvider: Send + Sync {
    fn module_id(&self) -> &str;

    /// 把 UI 的 AppContext 解析为该模块的紧凑上下文。
    /// 返回 `AgentError::ContextError` 表示无法提供上下文（如 entity 不存在）。
    fn build_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError>;
}

/// 把 bundle 汇总成一段给模型的文本描述（放在 system prompt 的上下文段）。
#[must_use]
pub fn bundle_to_text(bundle: &ContextBundle) -> String {
    let summary = serde_json::to_string_pretty(&bundle.summary).unwrap_or_default();
    let mut text = format!("[current context] {}\n{}", bundle.headline, summary);
    if text.chars().count() > 16_000 {
        text = text.chars().take(16_000).collect();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_defaults_are_sane() {
        assert_eq!(ContextBudget::default().max_items, 30);
        assert_eq!(ContextBudget::default().max_chars, 6000);
    }

    #[test]
    fn bundle_to_text_includes_headline() {
        let bundle = ContextBundle {
            module: "history".into(),
            headline: "History · 毛泽东（1893–1976）".into(),
            summary: serde_json::json!({"events": [{"id": "e1"}]}),
        };
        let text = bundle_to_text(&bundle);
        assert!(text.contains("毛泽东"));
        assert!(text.contains("\"events\""));
    }
}
