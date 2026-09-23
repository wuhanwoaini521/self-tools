//! 授权与暴露策略（V8 §17-§19/§37-§42/§67-§68）。
//!
//! `tools/list`（discovery）与 `tools/call`（execution）走**同一个**
//! `authorize`：未授权 client 连 catalog 都拿不到（§18/§19）。
//!
//! 决策只有两种：`Allowed` 或 `Denied{reason}`，reason 用 §41 的稳定码。

use devtoolbox_core::mcp::{AuthFailure, McpPrincipal, McpScope, ToolExposure};
use devtoolbox_core::personal_ai::ToolRisk;

use super::adapter::McpToolAdapter;

/// 授权决策。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationDecision {
    Allowed,
    Denied {
        /// 稳定 reason 码（§41 + 未暴露）。
        reason: String,
        /// 人类可读说明（不含任何凭证内容）。
        detail: String,
    },
}

impl AuthorizationDecision {
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, AuthorizationDecision::Allowed)
    }

    #[must_use]
    pub fn denied(reason: impl Into<String>, detail: impl Into<String>) -> Self {
        AuthorizationDecision::Denied {
            reason: reason.into(),
            detail: detail.into(),
        }
    }
}

/// 授权策略（纯函数 + 端口注入的身份校验）。
pub struct McpAuthorizationPolicy;

impl McpAuthorizationPolicy {
    /// 认证门禁（§26/§42）：principal 必须已认证、未过期、非不可信远程。
    #[must_use]
    pub fn authenticate(principal: &McpPrincipal, now: i64) -> AuthorizationDecision {
        if !principal.authenticated {
            return AuthorizationDecision::denied(
                AuthFailure::Unauthenticated.as_str(),
                AuthFailure::Unauthenticated.message(),
            );
        }
        if principal.is_expired(now) {
            return AuthorizationDecision::denied(
                AuthFailure::ExpiredToken.as_str(),
                AuthFailure::ExpiredToken.message(),
            );
        }
        if !principal.trust.can_expose_tools() {
            // §40：REMOTE_UNTRUSTED = 0 tools。
            return AuthorizationDecision::denied(
                AuthFailure::Unauthenticated.as_str(),
                "无法验证的远程调用方不暴露任何工具",
            );
        }
        AuthorizationDecision::Allowed
    }

    /// discovery 授权（§18）：返回该 principal 可见的 tool 名集合。
    #[must_use]
    pub fn visible_tools(adapter: &McpToolAdapter, principal: &McpPrincipal) -> Vec<String> {
        adapter
            .all_definitions()
            .into_iter()
            .filter(|definition| {
                Self::authorize_tool(adapter, principal, &definition.name).is_allowed()
            })
            .map(|definition| definition.name)
            .collect()
    }

    /// 单个 tool 的暴露 + scope 授权（§17/§19）。
    #[must_use]
    pub fn authorize_tool(
        adapter: &McpToolAdapter,
        principal: &McpPrincipal,
        tool_name: &str,
    ) -> AuthorizationDecision {
        // 0) 过期 / 未认证门禁（§41/§42）：即使工具存在也不得继续。
        if !principal.authenticated || principal.is_expired(unix_now()) {
            let failure = if !principal.authenticated {
                AuthFailure::Unauthenticated
            } else {
                AuthFailure::ExpiredToken
            };
            return AuthorizationDecision::denied(failure.as_str(), failure.message());
        }
        // 1) 必须已在 ToolRegistry（否则无从谈风险）。
        let Some(spec) = adapter.registry().spec(tool_name) else {
            return AuthorizationDecision::denied("unknown_tool", "工具不存在");
        };
        // 2) 必须被 MCP 暴露表收录（§19：未列出 = 不暴露）。
        let Some(exposure) = adapter.exposure_for(tool_name, principal) else {
            return AuthorizationDecision::denied("not_exposed", "该工具未对当前调用方暴露");
        };
        // 3) scope 检查（§36/§37）。
        let required = match McpScope::parse(exposure.required_scope) {
            Some(scope) => scope,
            None => {
                // 表内 scope 必须是已知值：配置错误 → fail-closed。
                return AuthorizationDecision::denied("scope_unknown", "工具 scope 配置无效");
            }
        };
        if !principal.has_scope(&required) {
            return AuthorizationDecision::denied(
                AuthFailure::InsufficientScope.as_str(),
                format!("需要 scope：{}", required.as_str()),
            );
        }
        // 4) 风险一致性（§37/§108）：registry 的 risk 与暴露分组不得矛盾。
        if !risk_matches_exposure(spec.risk, exposure) {
            return AuthorizationDecision::denied("risk_mismatch", "工具风险与暴露策略不一致");
        }
        AuthorizationDecision::Allowed
    }
}

/// 暴露分组与 registry risk 的一致性校验（防表与实现漂移，§108）。
///
/// 注意 V7 的既有决策（ADR-006）：`services.restart` 在 `ToolRegistry` 注册为
/// **Read**（因为 registry 的 `allowed_risk` 门禁只放行 Read+SafeWrite），
/// 它的 SYSTEM 语义由**暴露表**（`ExposureGroup::SystemAction`）与
/// `SafeActionService` 的票据表达。因此这里允许
/// `Read ↔ SystemAction` 这一种「表比实现更严」的组合，其余组合必须一致。
#[must_use]
pub fn risk_matches_exposure(risk: ToolRisk, exposure: ToolExposure) -> bool {
    use devtoolbox_core::mcp::ExposureGroup;
    match exposure.group {
        ExposureGroup::BasicRead | ExposureGroup::KnowledgeRead | ExposureGroup::ModuleRead => {
            matches!(risk, ToolRisk::Read)
        }
        ExposureGroup::SafeWrite => matches!(risk, ToolRisk::SafeWrite),
        ExposureGroup::SystemAction => {
            matches!(risk, ToolRisk::System | ToolRisk::Read)
        }
    }
}

/// scope 授权失败时的 MCP 错误码（§41：不模糊成 internal error）。
#[must_use]
pub fn mcp_error_code(decision: &AuthorizationDecision) -> &'static str {
    match decision {
        AuthorizationDecision::Allowed => "ok",
        AuthorizationDecision::Denied { reason, .. } => match reason.as_str() {
            "unauthenticated" => "unauthenticated",
            "expired_token" => "expired_token",
            "insufficient_scope" => "insufficient_scope",
            "provider_unavailable" => "provider_unavailable",
            "not_exposed" => "tool_not_exposed",
            "unknown_tool" => "unknown_tool",
            _ => "denied",
        },
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::adapter::{McpToolAdapter, McpToolDefinition};
    use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};
    use devtoolbox_core::personal_ai::ToolSpec;
    use std::sync::Arc;

    /// 测试工具（与 `registry.rs` 的 FakeTool 同形态：spec 存字段）。
    struct FixedTool {
        spec: ToolSpec,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for FixedTool {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        async fn execute(
            &self,
            _arguments: serde_json::Value,
        ) -> Result<devtoolbox_core::ToolResult, devtoolbox_core::AgentError> {
            Ok(devtoolbox_core::ToolResult::ok(serde_json::json!({})))
        }
    }

    fn adapter_with(tools: Vec<(&'static str, ToolRisk, &'static str)>) -> Arc<McpToolAdapter> {
        let mut registry = ToolRegistry::new();
        for (name, risk, module) in tools {
            registry
                .register(Arc::new(FixedTool {
                    spec: ToolSpec {
                        name: name.to_string(),
                        description: "t".into(),
                        input_schema: serde_json::json!({"type": "object"}),
                        risk,
                        module: module.into(),
                    },
                }))
                .expect("register");
        }
        Arc::new(McpToolAdapter::new(Arc::new(registry)))
    }

    fn principal_with(scopes: Vec<&'static str>) -> McpPrincipal {
        McpPrincipal {
            principal_id: "p".into(),
            client_id: "c".into(),
            transport: devtoolbox_core::mcp::McpTransport::Http,
            trust: devtoolbox_core::mcp::McpTrustLevel::RemoteAuthenticated,
            authenticated: true,
            scopes: scopes
                .into_iter()
                .filter_map(devtoolbox_core::mcp::McpScope::parse)
                .collect(),
            issuer: None,
            expires_at: None,
        }
    }

    #[test]
    fn discovery_is_filtered_by_scope() {
        // §98 Case 1/3/4：有 scope 才看得见。
        let adapter = adapter_with(vec![
            ("memory.search", ToolRisk::Read, "memory"),
            ("server.get_status", ToolRisk::Read, "server"),
        ]);
        let only_server = principal_with(vec!["server.read"]);
        let visible = McpAuthorizationPolicy::visible_tools(&adapter, &only_server);
        assert!(
            visible.contains(&"server.get_status".to_string()),
            "{visible:?}"
        );
        assert!(
            !visible.contains(&"memory.search".to_string()),
            "§148：无 memory.read 不得看见 memory 工具"
        );

        let both = principal_with(vec!["selftools.read"]);
        assert_eq!(
            McpAuthorizationPolicy::visible_tools(&adapter, &both).len(),
            2
        );
    }

    #[test]
    fn unauthenticated_principal_sees_nothing() {
        // §98 Case 2 / Demo I。
        let adapter = adapter_with(vec![("server.get_status", ToolRisk::Read, "server")]);
        let anonymous = McpPrincipal::anonymous_remote("curl");
        assert!(McpAuthorizationPolicy::visible_tools(&adapter, &anonymous).is_empty());
        assert!(!McpAuthorizationPolicy::authenticate(&anonymous, 0).is_allowed());
        assert!(
            !McpAuthorizationPolicy::authorize_tool(&adapter, &anonymous, "server.get_status")
                .is_allowed()
        );
    }

    #[test]
    fn unexposed_tool_is_hidden_even_for_local_trusted() {
        // §19：不在默认表的工具（如 shell.exec）永不可见。
        let adapter = adapter_with(vec![("shell.exec", ToolRisk::Read, "shell")]);
        let local = McpPrincipal::local("pi");
        assert!(
            McpAuthorizationPolicy::authorize_tool(&adapter, &local, "shell.exec")
                != AuthorizationDecision::Allowed
        );
        assert!(McpAuthorizationPolicy::visible_tools(&adapter, &local).is_empty());
    }

    #[test]
    fn system_action_requires_server_action_scope_and_is_remote_hidden() {
        // §98 Case 5 / Demo J。
        let adapter = adapter_with(vec![("services.restart", ToolRisk::Read, "server")]);
        let read_only = principal_with(vec!["server.read"]);
        let denied =
            McpAuthorizationPolicy::authorize_tool(&adapter, &read_only, "services.restart");
        assert!(!denied.is_allowed());
        // 顺序：remote 不可见先于 scope 检查（不向未授权方泄露 scope 需求）。
        assert_eq!(mcp_error_code(&denied), "tool_not_exposed");

        // SYSTEM 对远程不可见（§77）：即使有 scope 也不行。
        let action_scope = principal_with(vec!["server.action"]);
        let hidden =
            McpAuthorizationPolicy::authorize_tool(&adapter, &action_scope, "services.restart");
        assert!(!hidden.is_allowed(), "remote 不得暴露 SYSTEM");

        // 本地受信 + scope  →  允许（但执行仍走 SafeAction 确认，§38）。
        let mut local = McpPrincipal::local("pi");
        local.scopes = vec![devtoolbox_core::mcp::McpScope::parse("server.action").expect("scope")];
        assert!(
            McpAuthorizationPolicy::authorize_tool(&adapter, &local, "services.restart")
                .is_allowed()
        );
    }

    #[test]
    fn unknown_tool_is_denied() {
        let adapter = adapter_with(Vec::new());
        let local = McpPrincipal::local("pi");
        let denied = McpAuthorizationPolicy::authorize_tool(&adapter, &local, "nope.nope");
        assert!(!denied.is_allowed());
        assert_eq!(mcp_error_code(&denied), "unknown_tool");
    }

    #[test]
    fn expired_credentials_are_denied() {
        let mut principal = principal_with(vec!["selftools.read"]);
        principal.expires_at = Some(10);
        let adapter = adapter_with(vec![("memory.search", ToolRisk::Read, "memory")]);
        assert!(!McpAuthorizationPolicy::authenticate(&principal, 11).is_allowed());
        assert!(
            !McpAuthorizationPolicy::authorize_tool(&adapter, &principal, "memory.search")
                .is_allowed(),
            "过期 principal 不得调用工具"
        );
        assert!(McpAuthorizationPolicy::authenticate(&principal, 5).is_allowed());
    }

    #[test]
    fn risk_mismatch_is_fail_closed() {
        use devtoolbox_core::mcp::ExposureGroup;
        let basic = ToolExposure {
            group: ExposureGroup::BasicRead,
            required_scope: "selftools.read",
            remote_visible: true,
        };
        let system = ToolExposure {
            group: ExposureGroup::SystemAction,
            required_scope: "server.action",
            remote_visible: false,
        };
        // Read 工具映射到 BasicRead：允许。
        assert!(risk_matches_exposure(ToolRisk::Read, basic));
        // V7 形态：registry risk=Read 但暴露表标 SystemAction → 允许（表更严）。
        assert!(risk_matches_exposure(ToolRisk::Read, system));
        // 漂移保护：Read 工具声明 SafeWrite 分组 → 拒绝。
        let write = ToolExposure {
            group: ExposureGroup::SafeWrite,
            required_scope: "memory.write",
            remote_visible: true,
        };
        assert!(!risk_matches_exposure(ToolRisk::Read, write));
    }

    #[test]
    fn definition_annotations_expose_risk() {
        // §16：风险在 MCP 面不丢失。
        let adapter = adapter_with(vec![("services.restart", ToolRisk::Read, "server")]);
        let definition: McpToolDefinition =
            adapter.definition("services.restart").expect("definition");
        let annotations = definition.annotations.expect("annotations");
        // V7 形态（ADR-006）：registry risk = Read，SYSTEM 语义由暴露表 + 票据表达。
        assert_eq!(annotations.exposure_group, "system_action");
        assert!(annotations.requires_confirmation, "§38：SYSTEM 必须确认");
    }
}
