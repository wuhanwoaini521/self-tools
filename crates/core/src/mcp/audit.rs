//! MCP 审计契约（V8 §78-§80）。
//!
//! 所有 remote MCP call 至少记录：时间 / principal / client / transport /
//! tool / risk / 授权结果 / 时长 / 结果码。
//! **绝不记录**：bearer token、secret、完整文档/记忆/文件正文（§79）。

use serde::{Deserialize, Serialize};

use super::principal::McpTransport;

/// 授权决策（§51：authorize 步骤的结果）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpDecision {
    Allowed,
    Denied,
}

impl McpDecision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            McpDecision::Allowed => "allowed",
            McpDecision::Denied => "denied",
        }
    }
}

/// 结果码（§78；不含正文，只表达协议/执行结果）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpResultCode {
    Ok,
    /// 协议错误（未知方法 / 坏帧）。
    ProtocolError,
    /// 认证失败（四态之一）。
    AuthFailed,
    /// 授权失败（scope 不足 / 未暴露）。
    Denied,
    /// 参数校验失败。
    InvalidParams,
    /// 工具不存在。
    UnknownTool,
    /// 工具执行失败。
    ToolFailed,
    /// SYSTEM 操作等待用户确认（§55：不是失败）。
    ConfirmationRequired,
}

impl McpResultCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            McpResultCode::Ok => "ok",
            McpResultCode::ProtocolError => "protocol_error",
            McpResultCode::AuthFailed => "auth_failed",
            McpResultCode::Denied => "denied",
            McpResultCode::InvalidParams => "invalid_params",
            McpResultCode::UnknownTool => "unknown_tool",
            McpResultCode::ToolFailed => "tool_failed",
            McpResultCode::ConfirmationRequired => "confirmation_required",
        }
    }
}

/// 审计条目（§78）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpAuditEntry {
    /// 请求 id（§80：串联 auth / tool / safe action / audit）。
    pub request_id: String,
    pub timestamp: i64,
    pub principal_id: String,
    pub client_id: String,
    pub transport: McpTransport,
    /// tool 名（`tools/list` 记 `__discover__`，让发现行为也可审计）。
    pub tool: String,
    /// tool 风险（未列出工具 = `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    pub decision: McpDecision,
    pub result: McpResultCode,
    pub duration_ms: u64,
}

impl McpAuditEntry {
    /// `tools/list` 的审计条目（§18：发现也是受控行为）。
    #[must_use]
    pub fn discovery(
        request_id: impl Into<String>,
        principal: &super::principal::McpPrincipal,
        now: i64,
        decision: McpDecision,
        duration_ms: u64,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            timestamp: now,
            principal_id: principal.principal_id.clone(),
            client_id: principal.client_id.clone(),
            transport: principal.transport,
            tool: "__discover__".to_string(),
            risk: None,
            decision,
            result: if decision == McpDecision::Allowed {
                McpResultCode::Ok
            } else {
                McpResultCode::Denied
            },
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::principal::McpPrincipal;

    #[test]
    fn discovery_entry_masks_tool_name() {
        let principal = McpPrincipal::anonymous_remote("curl");
        let entry = McpAuditEntry::discovery("r-1", &principal, 10, McpDecision::Denied, 3);
        assert_eq!(entry.tool, "__discover__");
        assert_eq!(entry.result, McpResultCode::Denied);
        assert_eq!(entry.client_id, "curl");
    }

    #[test]
    fn result_codes_round_trip() {
        for code in [
            McpResultCode::Ok,
            McpResultCode::AuthFailed,
            McpResultCode::ConfirmationRequired,
        ] {
            let json = serde_json::to_string(&code).expect("json");
            assert_eq!(serde_json::from_str::<McpResultCode>(&json).ok(), Some(code));
        }
        assert_eq!(McpResultCode::ConfirmationRequired.as_str(), "confirmation_required");
    }

    #[test]
    fn audit_entry_never_carries_token_fields() {
        // 结构层面保证：字段集合固定，没有 token / secret / content 槽位。
        let entry = McpAuditEntry {
            request_id: "r".into(),
            timestamp: 0,
            principal_id: "p".into(),
            client_id: "c".into(),
            transport: McpTransport::Http,
            tool: "server.get_status".into(),
            risk: Some("read".into()),
            decision: McpDecision::Allowed,
            result: McpResultCode::Ok,
            duration_ms: 1,
        };
        let json = serde_json::to_string(&entry).expect("json");
        for forbidden in ["token", "secret", "password", "content", "authorization"] {
            assert!(!json.contains(forbidden), "审计不得含 {forbidden}: {json}");
        }
    }
}
