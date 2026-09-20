# ADR-003 · Personal AI Hub：单一 Personal Agent + Registry 接入 + Tool 抽象

- 状态：**Accepted**（2026-09-16，V4 Gates 0-8 PASS）
- 领域：`crates/core/personal_ai` · `crates/application/personal_ai` · `apps/desktop`
- 关联：[ADR-001 依赖倒置与组合根](../../../ARCHITECTURE_BACKLOG.md)（Gate 5.5）、
  [V4 计划](../personal-ai/PERSONAL_AI_HUB_V4_PLAN.md)、[V4 终版架构](../personal-ai/PERSONAL_AI_HUB_V4.md)、
  [V2 Cutover 审计](../migration/09-history-v2-cutover-audit-2026-09-10.md)（V3 依赖基线）

## 背景

self-tools 已有 History / Travel / Geography / Language / Documents 等相对独立的模块。
社区常见的「为每个模块做一个 Agent」会导致 N 个模型/提示词/编排各自为政；反之，
单一聊天外壳 + if/else 分派又会把 AI 层写死。V4 需要在「独立产品」与「ChatGPT 克隆」
之间建立第三条路：**一个 PersonalAgent + 注册表驱动的模块接入**。

## 决策

### 1. 先做单一 PersonalAgent（不做 Multi-Agent）

一个 `PersonalAgent`（工具循环 + 统一 Provider + 统一错误模型）服务所有模块。
Multi-Agent / Router / Jev 推迟到未来版本，但架构（注册表）不阻塞。

理由：V4 的首要价值是验证「任意模块用同一机制接入 AI」，而非编排竞争。
单一 Agent 让 Prompt/上下文/工具注入只有一条路径，错误面小、可确定性测试。

### 2. Registry-based 模块（不做 if/else）

`ModuleRegistry`（描述 + ContextProvider）+ `ToolRegistry`（schema 校验 + 执行 + 风险门禁）。
新增模块 = 注册描述 + 注册工具 + 提供 ContextProvider；**PersonalAgent 零改动**。
模型可发现的工具列表由注册表注入，杜绝 `if module == "history"` 式分派。

### 3. Tool 抽象（不做裸函数 / 随意字符串）

`ToolSpec{name, description, input_schema, risk, module}` + `ToolResult{ok, data, error, metadata}`：

- 参数经确定性 schema 校验，杜绝「任意 JSON 直接进业务层」；
- 未知工具 / 参数错误 / 执行异常全部收敛为受控结果，不 panic；
- `module.action` 命名、风险分级为未来 SafeWrite（如 enrichment cache）预留边界。

### 4. Context 优先于 Memory

AppContext（Frontend 负责「我在哪」）+ 模块 ContextProvider（业务上下文）+ ContextBudget
（当前实体优先，硬截断）先落地；长期 Memory 只定义边界（PersonalMemoryStore），不实现。
会话记忆（Session Memory，内存 + 上限裁剪）与未来长期记忆**分表分界**。

### 5. 暂不引入 MCP；Provider 走 OpenAI-Compatible

本轮不 MCP 化：本地应用 + 有限工具体系下，注册表 + 显式 schema 已满足，
且避免协议层复杂化。Provider 抽象（`ChatModelProvider`）以 OpenAI-Compatible
为默认路线：OpenAI / DeepSeek / OpenRouter / LiteLLM / 本地 Ollama `/v1` 均可接，
travel 的 `LlmProvider` 保持独立（V5 候选统一为一个 chat provider）。

### 6. 无 API Key 门槛 / 离线安全

无 key 时：`configured=false` + 面板未配置提示 + Agent 受控 `model_unavailable`；
普通模块与启动流程零影响。AI 是 enhancement，不是运行依赖。

## 后果

**正面**：History 成为第一个标准模块（4 个 Read 工具 + 上下文），Travel/Geography/
Language 可按同一 `register_module → 工具 → ContextProvider` 路径接入，PersonalAgent 核心无需重写。

**代价/约束**：

- 工具执行当前为同步、快速查询（不在循环内跨 await 持锁）；未来慢工具需模块自预算；
- 迷你 JSON Schema 校验器只覆盖本项目工具子集（type/required/properties/items/enum）；
- 会话存储为内存实现，持久化列为 P1；
- prompt 组装为代码拼接（core system + 模块 + 上下文 + 工具），非长 Prompt 文件。

## 替代方案（已否决）

| 方案 | 否决理由 |
| --- | --- |
| 每模块一个 Agent | N 套编排无法统一测试与错误模型；违背 Principle 2 |
| 前端直连 LLM / 自己组 Prompt | 违反 V4 §56：前端只消费 AgentResponse；key 会泄漏到前端 |
| 全项目 RAG / Vector DB | 属于 V6+ 范围；V4 是 Foundation 而非 Everything AI |
| 引入 MCP | 当前工具面小，注册表 + schema 足够；MCP 列入 V7 候选 |
