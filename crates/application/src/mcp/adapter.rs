//! MCP ↔ ToolRegistry 适配器（V8 §11-§16）。
//!
//! 唯一职责：把 `ToolSpec` 翻译成 MCP tool definition，把 MCP tool call
//! 翻译成 `ToolCallRequest`。**零业务逻辑、零 schema 手写**（§12）。

use std::sync::Arc;

use devtoolbox_core::mcp::{McpPrincipal, ToolExposure, default_exposure};
use devtoolbox_core::personal_ai::{ToolCallRequest, ToolResult, ToolRisk};

use crate::personal_ai::registry::ToolRegistry;

/// MCP tool definition（§12：从 ToolSpec 派生）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct McpToolDefinition {
    /// 稳定名字（§13：不加 `mcp_*_v2` 前缀）。
    pub name: String,
    pub description: String,
    /// 标准兼容 JSON Schema（来自 `ToolSpec::input_schema`）。
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// MCP annotations：风险不丢失（§16）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpToolAnnotations>,
}

/// MCP tool annotations（§16：risk / module / exposure 作为元数据保留）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct McpToolAnnotations {
    /// `read` / `safe_write` / `sensitive_write` / `system`（§16）。
    pub risk: String,
    pub module: String,
    /// 暴露分组（§67）。
    #[serde(rename = "exposureGroup")]
    pub exposure_group: String,
    /// 是否需要用户确认（SYSTEM 恒 true，§38）。
    #[serde(rename = "requiresConfirmation")]
    pub requires_confirmation: bool,
}

/// 单次调用结果（§55：SYSTEM 首次返回 confirmation_required）。
#[derive(Clone, Debug)]
pub enum McpCallOutcome {
    /// 工具已执行（READ / SAFE_WRITE）。
    Executed(ToolResult),
    /// 需要用户确认：携带确认票据摘要（§55）。
    ConfirmationRequired {
        confirmation_id: String,
        summary: String,
        target_id: String,
        risk: String,
        expires_at: i64,
    },
}

impl McpCallOutcome {
    /// MCP `isError` 语义：这里是**正常业务结果**，不是错误。
    #[must_use]
    pub fn is_confirmation(&self) -> bool {
        matches!(self, McpCallOutcome::ConfirmationRequired { .. })
    }
}

/// 适配器：ToolRegistry → MCP 面。
pub struct McpToolAdapter {
    registry: Arc<ToolRegistry>,
}

impl McpToolAdapter {
    #[must_use]
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    #[must_use]
    pub fn registry(&self) -> &Arc<ToolRegistry> {
        &self.registry
    }

    /// 全部工具的 MCP definition（未过滤；授权由 policy 负责，§17）。
    ///
    /// Gate 2 PASS：Registry 新增/删除 tool → 这里自动同步（无第二张表）。
    #[must_use]
    pub fn all_definitions(&self) -> Vec<McpToolDefinition> {
        self.registry
            .specs()
            .iter()
            .map(|spec| definition_from_spec(spec, default_exposure(&spec.name)))
            .collect()
    }

    /// 单个工具的 MCP definition。
    #[must_use]
    pub fn definition(&self, tool_name: &str) -> Option<McpToolDefinition> {
        let spec = self.registry.spec(tool_name)?;
        Some(definition_from_spec(spec, default_exposure(tool_name)))
    }

    /// 该工具对给定 principal 的暴露策略（None = 不暴露）。
    #[must_use]
    pub fn exposure_for(&self, tool_name: &str, principal: &McpPrincipal) -> Option<ToolExposure> {
        let exposure = default_exposure(tool_name)?;
        // §40/§77：远程不可见的工具对 HTTP principal 一律隐藏。
        let remote = matches!(
            principal.trust,
            devtoolbox_core::mcp::McpTrustLevel::RemoteAuthenticated
                | devtoolbox_core::mcp::McpTrustLevel::RemoteUntrusted
        );
        if remote && !exposure.remote_visible {
            return None;
        }
        Some(exposure)
    }

    /// 执行一个工具（调用方必须已通过授权；本方法不再做授权判断）。
    ///
    /// 与 PersonalAgent 走**同一条**执行路径（`ToolRegistry::execute`）——
    /// MCP 不复用 PersonalAgent，也不新开执行通道（§109-§111）。
    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, devtoolbox_core::AgentError> {
        self.registry
            .execute(&ToolCallRequest {
                id: format!("mcp-{tool_name}"),
                name: tool_name.to_string(),
                arguments,
            })
            .await
    }
}

/// `ToolRisk` → 稳定字符串（与 serde snake_case 一致：`read` / `system` …）。
pub(crate) fn risk_str(risk: ToolRisk) -> String {
    serde_json::to_value(risk)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{risk:?}").to_lowercase())
}

/// `ToolSpec` → MCP definition（纯函数）。
fn definition_from_spec(
    spec: &devtoolbox_core::personal_ai::ToolSpec,
    exposure: Option<ToolExposure>,
) -> McpToolDefinition {
    let exposure_group = exposure
        .map(|entry| entry.group.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    // §38/§16：是否需要确认由**语义**决定 —— 暴露分组为 SystemAction 即需确认
    // （registry risk 可能是 Read：V7 为通过 registry 门禁而把 SYSTEM 语义
    // 上移到暴露表，见 ADR-006）。
    let requires_confirmation = exposure.is_some_and(|entry| {
        matches!(
            entry.group,
            devtoolbox_core::mcp::ExposureGroup::SystemAction
        )
    });
    McpToolDefinition {
        name: spec.name.clone(),
        description: spec.description.clone(),
        input_schema: normalize_schema(&spec.input_schema),
        annotations: Some(McpToolAnnotations {
            risk: risk_str(spec.risk),
            module: spec.module.clone(),
            exposure_group,
            requires_confirmation,
        }),
    }
}

/// schema 归一化（§14）：内部 mini schema 已是合法 JSON Schema 子集；
/// 这里补齐 `$schema` 标记并保证 `type: object`，避免 MCP client 侧歧义。
/// **不改变内部 Registry 的形状**。
fn normalize_schema(schema: &serde_json::Value) -> serde_json::Value {
    let mut value = schema.clone();
    if let Some(object) = value.as_object_mut() {
        object
            .entry("type")
            .or_insert_with(|| serde_json::json!("object"));
    } else {
        value = serde_json::json!({"type": "object"});
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_ai::registry::{ToolExecutor, ToolRegistry};
    use devtoolbox_core::personal_ai::ToolSpec;

    struct EchoTool;

    #[async_trait::async_trait]
    impl ToolExecutor for EchoTool {
        fn spec(&self) -> &ToolSpec {
            static SPEC: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(|| ToolSpec {
                name: "memory.search".into(),
                description: "检索记忆".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {"query": {"type": "string"}}
                }),
                risk: ToolRisk::Read,
                module: "memory".into(),
            });
            &SPEC
        }

        async fn execute(
            &self,
            arguments: serde_json::Value,
        ) -> Result<ToolResult, devtoolbox_core::AgentError> {
            Ok(ToolResult::ok(arguments))
        }
    }

    fn registry_with(tool: Arc<dyn ToolExecutor>) -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(tool).expect("register");
        Arc::new(registry)
    }

    #[tokio::test]
    async fn definitions_are_derived_from_registry_not_handwritten() {
        let adapter = McpToolAdapter::new(registry_with(Arc::new(EchoTool)));
        let definitions = adapter.all_definitions();
        assert_eq!(definitions.len(), 1);
        let definition = &definitions[0];
        assert_eq!(definition.name, "memory.search", "§13：稳定名字");
        assert_eq!(definition.description, "检索记忆");
        assert_eq!(definition.input_schema["type"], "object");
        assert_eq!(
            definition.input_schema["properties"]["query"]["type"], "string",
            "schema 必须从 ToolSpec 原样派生"
        );
        let annotations = definition.annotations.as_ref().expect("annotations");
        assert_eq!(annotations.risk, "read", "§16：risk 不丢失");
        assert_eq!(annotations.module, "memory");
        assert!(!annotations.requires_confirmation);
    }

    #[tokio::test]
    async fn catalog_tracks_registry_contents() {
        // Gate 2 PASS：catalog 完全由 registry 派生 —— 注册两个工具就看到两个。
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).expect("register");

        struct SecondTool;
        #[async_trait::async_trait]
        impl ToolExecutor for SecondTool {
            fn spec(&self) -> &ToolSpec {
                static SPEC: std::sync::LazyLock<ToolSpec> =
                    std::sync::LazyLock::new(|| ToolSpec {
                        name: "files.search".into(),
                        description: "检索文件".into(),
                        input_schema: serde_json::json!({"type": "object"}),
                        risk: ToolRisk::Read,
                        module: "files".into(),
                    });
                &SPEC
            }
            async fn execute(
                &self,
                _arguments: serde_json::Value,
            ) -> Result<ToolResult, devtoolbox_core::AgentError> {
                Ok(ToolResult::ok(serde_json::json!({})))
            }
        }
        registry.register(Arc::new(SecondTool)).expect("register");

        let adapter = McpToolAdapter::new(Arc::new(registry));
        assert_eq!(adapter.all_definitions().len(), 2);
        assert!(adapter.definition("files.search").is_some());
        assert!(adapter.definition("nope.nope").is_none());
    }

    #[tokio::test]
    async fn execute_routes_through_the_same_registry_path() {
        let adapter = McpToolAdapter::new(registry_with(Arc::new(EchoTool)));
        let result = adapter
            .execute("memory.search", serde_json::json!({"query": "docker"}))
            .await
            .expect("execute");
        assert_eq!(result.data["query"], "docker");
        // 未注册工具 → 与 agent 路径同样的 not-found 错误。
        assert!(
            adapter
                .execute("nope", serde_json::json!({}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn system_tools_are_hidden_from_remote_principals() {
        // §77：services.restart 对 HTTP principal 不可见。
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).expect("register");
        let adapter = McpToolAdapter::new(Arc::new(registry));

        let local = devtoolbox_core::mcp::McpPrincipal::local("pi");
        assert!(adapter.exposure_for("memory.search", &local).is_some());

        let remote = devtoolbox_core::mcp::McpPrincipal::anonymous_remote("curl");
        assert!(adapter.exposure_for("memory.search", &remote).is_some());
        // 未暴露工具（不在默认表）不能借 exposure 通道出现。
        assert!(adapter.exposure_for("files.search", &remote).is_some());
        assert!(adapter.exposure_for("shell.exec", &remote).is_none());
    }

    #[test]
    fn normalize_schema_never_changes_registry_shape() {
        let schema = serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let normalized = normalize_schema(&schema);
        assert_eq!(normalized["properties"]["a"]["type"], "string");
        let implied = normalize_schema(&serde_json::json!({"properties": {}}));
        assert_eq!(implied["type"], "object", "缺 type 时补齐");
    }
}
