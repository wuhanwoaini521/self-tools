//! Personal AI 统一错误模型（V4 §33）。
//!
//! 所有 AI 层错误收敛到 `AgentError`，稳定 `code` 供前端分类展示，
//! 避免前端统一显示「发生错误」。

use thiserror::Error;

/// 稳定错误 code（前端据此分类展示；新增变体必须补 code，勿复用）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentErrorKind {
    /// 模型服务未配置或不可用（无 key / 无 base_url / 网络不可达）。
    ModelUnavailable,
    /// 模型调用超时。
    ProviderTimeout,
    /// 模型服务返回错误（HTTP 错误 / 响应解析失败等）。
    Provider,
    /// 工具不存在。
    ToolNotFound,
    /// 工具参数校验失败。
    ToolInvalidArgument,
    /// 工具执行失败。
    ToolExecutionFailed,
    /// 上下文构建失败。
    ContextError,
    /// Tool 调用轮数超过上限。
    MaxToolRounds,
    /// 会话错误。
    Session,
}

impl AgentErrorKind {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::ModelUnavailable => "personal_ai_model_unavailable",
            Self::ProviderTimeout => "personal_ai_provider_timeout",
            Self::Provider => "personal_ai_provider_error",
            Self::ToolNotFound => "personal_ai_tool_not_found",
            Self::ToolInvalidArgument => "personal_ai_tool_invalid_argument",
            Self::ToolExecutionFailed => "personal_ai_tool_execution_failed",
            Self::ContextError => "personal_ai_context_error",
            Self::MaxToolRounds => "personal_ai_max_tool_rounds",
            Self::Session => "personal_ai_session_error",
        }
    }
}

/// 个人 AI 层错误。kind 用于机器分类，message 直接面向用户。
#[derive(Debug, Error, Clone)]
#[error("{}: {message}", .kind.code())]
pub struct AgentError {
    pub kind: AgentErrorKind,
    pub message: String,
}

impl AgentError {
    #[must_use]
    pub fn new(kind: AgentErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn model_unavailable(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ModelUnavailable, message)
    }
    #[must_use]
    pub fn provider_timeout(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ProviderTimeout, message)
    }
    #[must_use]
    pub fn provider(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::Provider, message)
    }
    #[must_use]
    pub fn tool_not_found(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ToolNotFound, message)
    }
    #[must_use]
    pub fn tool_invalid_argument(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ToolInvalidArgument, message)
    }
    #[must_use]
    pub fn tool_execution_failed(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ToolExecutionFailed, message)
    }
    #[must_use]
    pub fn context(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::ContextError, message)
    }
    #[must_use]
    pub fn max_tool_rounds(max: usize) -> Self {
        Self::new(
            AgentErrorKind::MaxToolRounds,
            format!("tool calling exceeded the safety limit of {max} rounds"),
        )
    }
    #[must_use]
    pub fn session(message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::Session, message)
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let kinds = [
            AgentErrorKind::ModelUnavailable,
            AgentErrorKind::ProviderTimeout,
            AgentErrorKind::Provider,
            AgentErrorKind::ToolNotFound,
            AgentErrorKind::ToolInvalidArgument,
            AgentErrorKind::ToolExecutionFailed,
            AgentErrorKind::ContextError,
            AgentErrorKind::MaxToolRounds,
            AgentErrorKind::Session,
        ];
        let codes: std::collections::HashSet<&str> = kinds.iter().map(|k| k.code()).collect();
        assert_eq!(codes.len(), kinds.len(), "codes must be unique");
    }
}
