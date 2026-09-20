# SELF-TOOLS V4 · PERSONAL AI HUB — 终版架构

> 状态：✅ 已实施（Gates 0-9 PASS，见 [`V4_OVERNIGHT_STATUS.md`](V4_OVERNIGHT_STATUS.md)；
> 计划与审计见 [`PERSONAL_AI_HUB_V4_PLAN.md`](PERSONAL_AI_HUB_V4_PLAN.md)；
> 决策记录见 [`docs/architecture/ADR-003-personal-ai-hub.md`](../architecture/ADR-003-personal-ai-hub.md)）。

---

## 1. PersonalAgent 是什么

`PersonalAgent`（`crates/core/src/personal_ai/` 类型 + `crates/application/src/personal_ai/agent.rs` 循环）
是 self-tools **唯一**的 AI 核心：

```text
AgentRequest{message, session_id, app_context, capabilities, locale}
  → Prompt 组装（core system + 模块清单 + 当前上下文 + 工具 schema）
  → ChatModelProvider.chat()（messages + tools + usage）
       ├─ tool_calls → ToolRegistry.validate + execute → ToolResult 回喂 → 再调（≤ max_tool_rounds=4）
       └─ 无 tool_calls → 解析 envelope → AgentResponse{message, actions, ui_blocks, tool_trace, usage}
```

- 一个核心服务所有模块（V4 Principle 2）；不负责业务 DB、前端路由、key 存储。
- 模型只产出 Action **请求**，Frontend/Application 决定是否执行（V4 §51）。

## 2. Module Registry 是什么

`ModuleRegistry`（application/personal_ai/registry.rs）：模块描述注册表。

```rust
ModuleDescriptor { id, display_name, description, capabilities, tools }
```

当前已注册：`history`（描述 + 4 工具 + ContextProvider）。加入新模块 = `register_module()` +
注册其工具 + 提供 ContextProvider，**PersonalAgent 零改动、零 if/else**。

## 3. Tool Registry 是什么

`ToolRegistry`：discover / schema / validate / execute / 风险门禁。

```rust
ToolSpec  { name: "history.search", description, input_schema: JSON Schema, risk: Read, module: "history" }
ToolResult{ ok, data, error, metadata }
```

- 参数经 `json_schema.rs` 迷你校验器验证后才进业务层（V4 §114）；
- 命名统一 `module.action`；未知工具 / 参数错误 / 执行异常 → 受控 `ToolResult::fail`，不 panic；
- **风险门禁**：组合根只装配 `Read`（V4 §21/§14；`SafeWrite/SensitiveWrite/System`
  枚举存在但当前无注册路径）。

## 4. AppContext 是什么

`AppContext{module, page, entity, selection, view_state}` —— AI 必须知道用户当前在哪里、在看什么。

- **所有权**：Frontend 负责「我在哪」（App.tsx 桥 + HistoryPage 上报当前实体）；
- 业务模块的 `ModuleContextProvider` 负责「这个 entity 的业务上下文」→ `ContextBundle`
  （headline + 紧凑结构化 summary，受 `ContextBudget{max_items=30, max_chars=6000}` 截断）。

## 5. Action Protocol

```json
{"type":"navigate|open_entity|refresh_view|show_panel","module":"history","target":{...}}
```

V4 只做 4 种；前端渲染器把 `open_entity/navigate` 映射到既有导航（openHistory 等）。
高风险动作（删除/写库/Shell/发送）V4 不支持。

## 6. UI Block Protocol

`UiBlock{kind: entity_list|entity_card|source_list|key_value|timeline_preview, title, data}`。
V4 实现 `EntityList` 卡片渲染（kind/标题/副标题/年份 + 点击导航）；其余 kind 优雅降级。

## 7. 如何注册新 Module（V4 §118 V5 路径）

```rust
// 1) 在 application/personal_ai/<module>.rs 定义：描述 + 工具执行器（impl ToolExecutor）+ ContextProvider
// 2) 在组合根（apps/desktop/src/personal_ai.rs）注册：
register_x(&mut hub.modules, &mut hub.tools, port);   // 每个模块一个 register_* 函数
```

不需要改：PersonalAgent、ToolRegistry、prompt 组装、前端协议。

## 8. 如何增加 Tool

在模块的注册函数里 `tools.register(Arc::new(MyTool {...}))`，并提供
`ToolSpec{name: "module.action", description, input_schema, risk: Read}`。
Agent 自动在下一轮对话中把它暴露给模型（工具列表由注册表注入，非手写 if/else）。

## 9. History 如何接入

`crates/application/src/personal_ai/history.rs` —— 复用 V3 `HistoryService`（端口 + 只读 DuckDB）：

| 工具 | 数据源 | 说明 |
| --- | --- | --- |
| `history.search(query, entity_type?, limit?)` | `HistoryService::search` | 精简命中（id/kind/标题/年份） |
| `history.get_event(id)` | `event_detail` | canonical + relations + people/places + evidence + `enrichment_state`（如实=库内 quality_status/evidence 概况） |
| `history.get_person(id)` | `person_detail` | canonical + relations + events + stories |
| `history.get_context(app_context)` | ContextProvider | 当前实体紧凑上下文（指代解析） |

V3 无 on-demand AI enrichment（审计结论，见 PLAN §1），故不注册 `ensure_enrichment`；
V4 不重建 V3 数据模型，全部 Read 工具，零写 canonical。

## 10. 如何配置模型

设置 → Ask AI：`base_url`（OpenAI Compatible）+ `model` + 可选 `api_key` + 超时。
存于 gitignored `config/settings.json` 的 `ai` 字段（`serde(default)` 向后兼容）；
OpenAI / DeepSeek / OpenRouter / LiteLLM / 自建 gateway / 本地 Ollama `/v1` 皆可。

## 11. 没有模型时会怎样

- `personal_ai_status.configured=false` → AI 面板显示「AI provider 未配置」+ 打开设置按钮；
- Agent 收到 `model_unavailable` 受控错误（code: `personal_ai_model_unavailable`）；
- History / Travel / Language / 其他页面与启动流程完全正常（V4 §63/§112）。

## 12. 依赖方向（不变式）

```text
core/personal_ai（无内部依赖）
  ↑
application/personal_ai（零 infra 引用；经 application::history 端口）
  ↑
infrastructure/personal_ai（OpenAI-Compatible 实现）
  ↑
apps/desktop（组合根：装配 + commands）
```

## 13. 安全边界

- Key 只进 Provider 配置；不进前端、不进日志、不提交 git（settings.json 于 gitignored config/）。
- 工具风险门禁 + 注册表只装配 Read。
- 模型输出仅作为 UI 注解；canonical（duckdb）只读。
- Agent 错误模型统一 `AgentErrorKind`（9 个稳定 code，前端可分类展示）。

## 14. 会话与记忆边界

- Session Memory（内存，上限 60 条/会话；持久化 P1）；Conversation ≠ Memory。
- `PersonalMemoryStore` 只保留为未来边界（V4 不实现，V6 候选）。
