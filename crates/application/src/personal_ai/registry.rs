//! 注册表（V4 §16-§18，Gate 2）。
//!
//! - `ToolRegistry`：discover / schema / validate / execute，**零 if/else 分派**。
//! - `ModuleRegistry`：模块描述 + ContextProvider + 工具归属。
//! - 新增模块 = register + 注册其工具；**不改 PersonalAgent**。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use devtoolbox_core::{
    AgentError, ModuleDescriptor, ToolCallRequest, ToolResult, ToolRisk, ToolSpec,
    personal_ai::AppContext,
};

use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::json_schema::Validator;

// ---------------------------------------------------------------------------
// ToolExecutor（application 内注册；组合根只装配）
// ---------------------------------------------------------------------------

/// 工具执行器。`execute` 必须同步且自限时间（本地快速查询）；未来需要
/// 慢工具的模块须自行做预算（如内部缓存），agent 循环不跨 await 持锁。
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 工具契约（schema / risk / module）。
    fn spec(&self) -> &ToolSpec;

    /// 执行。参数已通过 schema 校验（`ToolRegistry::execute` 前置）。
    /// V5：async（支持 `history.ensure_enrichment` 等慢/IO 工具）；由 agent 循环 await。
    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError>;

    /// Execute with the trusted current AppContext snapshot. Context-sensitive tools
    /// override this method; legacy tools stay source-compatible through the default.
    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _context: &AppContext,
    ) -> Result<ToolResult, AgentError> {
        self.execute(arguments).await
    }
}

/// 工具执行错误（统一映射为 `AgentError::ToolExecutionFailed`）。
#[derive(Debug, thiserror::Error)]
#[error("tool execution failed: {0}")]
pub struct ToolError(pub String);

// ---------------------------------------------------------------------------
// ToolRegistry

/// 工具注册表：注册 / 发现 / schema / 参数校验 / 执行。
///
/// `Clone` 是浅拷贝（内部 `Arc<dyn ToolExecutor>`）：组合根可把同一份工具集
/// 共享给 `PersonalHub` 与 `OrchestrationService`（V10 §35），无须双份注册。
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册契约完整的工具（包括受限工具，供 catalog 与权限审计发现）。
    /// 风险门禁在模型暴露和每次执行时强制；注册本身不代表允许自动执行。
    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) -> Result<(), AgentError> {
        let spec = tool.spec();
        let name = spec.name.clone();
        if self.tools.contains_key(&name) {
            return Err(AgentError::tool_invalid_argument(format!(
                "tool already registered: {name}"
            )));
        }
        if !name.contains('.') {
            return Err(AgentError::tool_invalid_argument(format!(
                "tool name must be `module.action`, got: {name}"
            )));
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    /// 全部工具契约（discover）。
    #[must_use]
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = self
            .tools
            .values()
            .map(|tool| tool.spec().clone())
            .collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        specs
    }

    /// 单个工具契约。
    #[must_use]
    pub fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.get(name).map(|tool| tool.spec())
    }

    /// 指定模块的工具契约。
    #[must_use]
    pub fn specs_for_module(&self, module: &str) -> Vec<ToolSpec> {
        self.specs()
            .into_iter()
            .filter(|spec| spec.module == module)
            .collect()
    }

    /// 校验参数（不入执行器，保证「任意 JSON 不进业务层」V4 §114）。
    pub fn validate_args(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), AgentError> {
        let spec = self.spec(name).ok_or_else(|| {
            AgentError::tool_not_found(format!("tool `{name}` is not registered"))
        })?;
        Validator::validate(&spec.input_schema, arguments)
            .map_err(|detail| AgentError::tool_invalid_argument(format!("{name}: {detail}")))
    }

    /// 执行一个模型发出的 Tool Call（含校验；失败转受控 `ToolResult`，不 panic）。
    pub async fn execute(&self, call: &ToolCallRequest) -> Result<ToolResult, AgentError> {
        let tool = self.tools.get(&call.name).ok_or_else(|| {
            AgentError::tool_not_found(format!("tool `{}` is not registered", call.name))
        })?;
        ensure_automatic_execution_allowed(&call.name, tool.spec().risk)?;
        self.validate_args(&call.name, &call.arguments)?;
        tool.execute(call.arguments.clone()).await
    }

    /// Scoped variant for tools whose target must come from the current UI context.
    pub async fn execute_with_context(
        &self,
        call: &ToolCallRequest,
        context: &AppContext,
    ) -> Result<ToolResult, AgentError> {
        let tool = self.tools.get(&call.name).ok_or_else(|| {
            AgentError::tool_not_found(format!("tool `{}` is not registered", call.name))
        })?;
        ensure_automatic_execution_allowed(&call.name, tool.spec().risk)?;
        self.validate_args(&call.name, &call.arguments)?;
        tool.execute_with_context(call.arguments.clone(), context)
            .await
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ModuleRegistry
// ---------------------------------------------------------------------------

/// 模块注册项：描述 + 可选 ContextProvider。
pub struct ModuleRegistration {
    pub descriptor: ModuleDescriptor,
    pub context_provider: Option<Arc<dyn ModuleContextProvider>>,
}

/// 模块注册表：让 Personal Hub 知道 self-tools 有哪些模块、能力与上下文提供方。
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleRegistration>,
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// 注册模块。重复 id → 错误。
    pub fn register(&mut self, registration: ModuleRegistration) -> Result<(), AgentError> {
        let id = registration.descriptor.id.clone();
        if self.modules.contains_key(&id) {
            return Err(AgentError::tool_invalid_argument(format!(
                "module already registered: {id}"
            )));
        }
        self.modules.insert(id, registration);
        Ok(())
    }

    /// 全部模块描述（discover）。
    #[must_use]
    pub fn descriptors(&self) -> Vec<ModuleDescriptor> {
        let mut list: Vec<ModuleDescriptor> = self
            .modules
            .values()
            .map(|registration| registration.descriptor.clone())
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// 模块是否存在。
    #[must_use]
    pub fn contains(&self, module_id: &str) -> bool {
        self.modules.contains_key(module_id)
    }

    /// 模块的上下文提供方。
    #[must_use]
    pub fn context_provider(&self, module_id: &str) -> Option<Arc<dyn ModuleContextProvider>> {
        self.modules
            .get(module_id)
            .and_then(|r| r.context_provider.clone())
    }

    /// 解析 AppContext 为 Compact Context（V4 §25）。
    /// 上下文缺失 / 未注册模块 → `AgentError::ContextError`。
    pub fn resolve_context(
        &self,
        app_context: &AppContext,
        budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let module = app_context
            .module
            .as_deref()
            .ok_or_else(|| AgentError::context("no module in app context"))?;
        let provider = self.context_provider(module).ok_or_else(|| {
            AgentError::context(format!("module `{module}` has no context provider"))
        })?;
        provider.build_context(app_context, budget)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

/// 风险门禁：Read 与 SafeWrite 可自动执行；SensitiveWrite/System 仅可注册和发现，
/// 在实现了独立的用户确认流程前不得暴露给模型或直接执行。
#[must_use]
pub fn allowed_risk(risk: ToolRisk) -> bool {
    matches!(risk, ToolRisk::Read | ToolRisk::SafeWrite)
}

fn ensure_automatic_execution_allowed(name: &str, risk: ToolRisk) -> Result<(), AgentError> {
    if allowed_risk(risk) {
        return Ok(());
    }
    Err(AgentError::tool_execution_failed(format!(
        "tool `{name}` is not eligible for automatic execution (risk `{risk:?}`)"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- FakeTool ----
    struct FakeTool {
        spec: ToolSpec,
        fail: bool,
    }
    #[async_trait::async_trait]
    impl ToolExecutor for FakeTool {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
            if self.fail {
                return Err(AgentError::tool_execution_failed("boom"));
            }
            Ok(ToolResult::ok(json!({"echo": arguments})))
        }
    }

    fn search_tool() -> Arc<FakeTool> {
        Arc::new(FakeTool {
            spec: ToolSpec {
                name: "history.search".into(),
                description: "search history events".into(),
                input_schema: json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {"query": {"type": "string"}, "limit": {"type": "integer"}}
                }),
                risk: ToolRisk::Read,
                module: "history".into(),
            },
            fail: false,
        })
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn registry_discover_validate_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(search_tool()).unwrap();
        assert_eq!(registry.len(), 1);

        // discover
        let specs = registry.specs();
        assert_eq!(specs[0].name, "history.search");
        assert_eq!(specs[0].risk, ToolRisk::Read);

        // schema access
        assert!(registry.spec("history.search").is_some());
        assert!(registry.spec("missing").is_none());

        // validate
        assert!(
            registry
                .validate_args("history.search", &json!({"query": "遵义"}))
                .is_ok()
        );
        assert!(
            registry
                .validate_args("history.search", &json!({}))
                .is_err()
        );
        assert!(
            registry
                .validate_args("history.search", &json!({"query": 42}))
                .is_err()
        );

        // execute
        let result = block_on(registry.execute(&ToolCallRequest {
            id: "call_1".into(),
            name: "history.search".into(),
            arguments: json!({"query": "遵义"}),
        }))
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.data["echo"]["query"], "遵义");
    }

    #[test]
    fn scoped_executor_receives_current_app_context_after_schema_validation() {
        struct ScopedTool(ToolSpec);
        #[async_trait::async_trait]
        impl ToolExecutor for ScopedTool {
            fn spec(&self) -> &ToolSpec {
                &self.0
            }
            async fn execute(
                &self,
                _arguments: serde_json::Value,
            ) -> Result<ToolResult, AgentError> {
                Ok(ToolResult::fail("context was not supplied"))
            }
            async fn execute_with_context(
                &self,
                _arguments: serde_json::Value,
                context: &AppContext,
            ) -> Result<ToolResult, AgentError> {
                Ok(ToolResult::ok(
                    json!({"entity_id": context.entity.as_ref().map(|entity| entity.id.as_str())}),
                ))
            }
        }

        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(ScopedTool(ToolSpec {
                name: "travel.propose_add_place".into(),
                description: "preview".into(),
                input_schema: json!({"type": "object"}),
                risk: ToolRisk::Read,
                module: "travel".into(),
            })))
            .unwrap();
        let result = block_on(registry.execute_with_context(
            &ToolCallRequest {
                id: "c1".into(),
                name: "travel.propose_add_place".into(),
                arguments: json!({}),
            },
            &AppContext {
                module: Some("travel".into()),
                entity: Some(devtoolbox_core::personal_ai::EntityRef {
                    kind: "destination".into(),
                    id: "Luoyang".into(),
                    label: None,
                }),
                ..AppContext::default()
            },
        ))
        .unwrap();
        assert_eq!(result.data["entity_id"], "Luoyang");
    }

    #[test]
    fn unknown_tool_is_controlled_error() {
        let mut registry = ToolRegistry::new();
        registry.register(search_tool()).unwrap();
        let error = block_on(registry.execute(&ToolCallRequest {
            id: "c".into(),
            name: "history.missing".into(),
            arguments: json!({}),
        }))
        .unwrap_err();
        assert_eq!(error.code(), "personal_ai_tool_not_found");
    }

    #[test]
    fn invalid_arguments_never_reach_executor() {
        let mut registry = ToolRegistry::new();
        let tool = Arc::new(FakeTool {
            spec: ToolSpec {
                name: "strict.tool".into(),
                description: "d".into(),
                input_schema: json!({"type": "object", "required": ["ok"], "properties": {"ok": {"type": "boolean"}}}),
                risk: ToolRisk::Read,
                module: "strict".into(),
            },
            fail: false,
        });
        registry.register(tool).unwrap();
        let error = block_on(registry.execute(&ToolCallRequest {
            id: "c".into(),
            name: "strict.tool".into(),
            arguments: json!({"ok": "yes"}),
        }))
        .unwrap_err();
        assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
    }

    #[test]
    fn tool_exception_is_controlled() {
        let mut registry = ToolRegistry::new();
        registry
            .register(Arc::new(FakeTool {
                spec: ToolSpec {
                    name: "boom.tool".into(),
                    description: "d".into(),
                    input_schema: json!({"type": "object"}),
                    risk: ToolRisk::Read,
                    module: "boom".into(),
                },
                fail: true,
            }))
            .unwrap();
        let error = block_on(registry.execute(&ToolCallRequest {
            id: "c".into(),
            name: "boom.tool".into(),
            arguments: json!({}),
        }))
        .unwrap_err();
        assert_eq!(error.code(), "personal_ai_tool_execution_failed");
    }

    #[test]
    fn duplicate_and_bad_names_rejected() {
        let mut registry = ToolRegistry::new();
        registry.register(search_tool()).unwrap();
        assert!(registry.register(search_tool()).is_err());
        assert!(
            registry
                .register(Arc::new(FakeTool {
                    spec: ToolSpec {
                        name: "nodot".into(),
                        description: "d".into(),
                        input_schema: json!({}),
                        risk: ToolRisk::Read,
                        module: "x".into(),
                    },
                    fail: false,
                }))
                .is_err()
        );
    }

    #[test]
    fn module_registry_register_and_resolve() {
        let mut modules = ModuleRegistry::new();
        let provider = NoopProvider;
        modules
            .register(ModuleRegistration {
                descriptor: ModuleDescriptor {
                    id: "history".into(),
                    display_name: "History".into(),
                    description: "history module".into(),
                    capabilities: vec!["search".into(), "entity".into()],
                    tools: vec!["history.search".into()],
                },
                context_provider: Some(Arc::new(provider)),
            })
            .unwrap();

        let descriptors = modules.descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].id, "history");
        assert!(modules.contains("history"));
        assert!(!modules.contains("travel"));

        let bundle = modules
            .resolve_context(
                &AppContext {
                    module: Some("history".into()),
                    ..AppContext::default()
                },
                &ContextBudget::default(),
            )
            .unwrap();
        assert_eq!(bundle.module, "history");

        // 未注册模块 → controlled context error
        let error = modules
            .resolve_context(
                &AppContext {
                    module: Some("nope".into()),
                    ..AppContext::default()
                },
                &ContextBudget::default(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "personal_ai_context_error");
    }

    #[test]
    fn risk_gate_only_permits_read() {
        assert!(allowed_risk(ToolRisk::Read));
        assert!(allowed_risk(ToolRisk::SafeWrite)); // V5：SafeWrite 允许（derived cache 类工具）
        assert!(!allowed_risk(ToolRisk::SensitiveWrite));
        assert!(!allowed_risk(ToolRisk::System));
    }

    #[test]
    fn restricted_tools_are_discoverable_but_never_auto_executed() {
        let mut registry = ToolRegistry::new();
        for (name, risk) in [
            ("memory.archive", ToolRisk::SensitiveWrite),
            ("server.restart", ToolRisk::System),
        ] {
            registry
                .register(Arc::new(FakeTool {
                    spec: ToolSpec {
                        name: name.into(),
                        description: "restricted action".into(),
                        input_schema: json!({"type": "object"}),
                        risk,
                        module: name.split('.').next().unwrap().into(),
                    },
                    fail: false,
                }))
                .unwrap();
        }

        let specs = registry.specs();
        assert_eq!(specs.len(), 2);
        assert!(specs.iter().any(|spec| spec.name == "memory.archive"));
        assert!(specs.iter().any(|spec| spec.name == "server.restart"));

        for name in ["memory.archive", "server.restart"] {
            let error = block_on(registry.execute(&ToolCallRequest {
                id: "c1".into(),
                name: name.into(),
                arguments: json!({}),
            }))
            .expect_err("restricted tools must not run without an approval flow");
            assert_eq!(error.code(), "personal_ai_tool_execution_failed");
            assert!(
                error
                    .message
                    .contains("not eligible for automatic execution")
            );

            let scoped_error = block_on(registry.execute_with_context(
                &ToolCallRequest {
                    id: "c2".into(),
                    name: name.into(),
                    arguments: json!({}),
                },
                &AppContext::default(),
            ))
            .expect_err("scoped execution must enforce the same risk gate");
            assert_eq!(scoped_error.code(), "personal_ai_tool_execution_failed");
        }
    }

    // ---- minimal provider for module registry test ----
    struct NoopProvider;
    impl ModuleContextProvider for NoopProvider {
        fn module_id(&self) -> &str {
            "history"
        }
        fn build_context(
            &self,
            _app_context: &AppContext,
            _budget: &ContextBudget,
        ) -> Result<ContextBundle, AgentError> {
            Ok(ContextBundle {
                module: "history".into(),
                headline: "History".into(),
                summary: json!({"ok": true}),
            })
        }
    }

    // ---- keep AppContext referenced (contract smoke) ----
}
