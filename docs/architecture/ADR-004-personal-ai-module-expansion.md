# ADR-004 · Personal AI Module Expansion：Provider 统一 + 按需富化 + 多模块注册

- 状态：**Accepted**（2026-09-21，V5 Gates 0-9 PASS）
- 领域：`crates/core|application|infrastructure` + `apps/desktop`
- 关联：[ADR-003-personal-ai-hub.md](ADR-003-personal-ai-hub.md)（V4）、
  [V5_PROVIDER_CONSOLIDATION.md](../personal-ai/V5_PROVIDER_CONSOLIDATION.md)、
  [V5 计划](../personal-ai/PERSONAL_AI_HUB_V5_PLAN.md)

## 背景

V4 留下两个模型 Provider（Personal AI `ChatModelProvider` 与 travel `LlmProvider`），
以及「History 是 AI 唯一模块、无按需数据」的平台局限。V5 需要：①统一模型接入点；
②把 History 升级为 Canonical + Evidence + On-demand Enrichment；③证明模块机制可承载
Travel/Geography/Language，且 PersonalAgent（V4 核心）不改业务。

## 决策

### 1. `ChatModelProvider` 成为唯一模型抽象

以 V4 的 `ChatModelProvider`（多消息 + 工具 + usage，超集）为最终入口；删除
`core::travel::LlmProvider` 与 `infra/travel/llm.rs`。travel 消费方经 `travel_complete`
薄适配（system+user → content），错误 Display 前缀 `travel llm request failed: …`、
`travel_llm_failed` code、未配置降级「来源列表」全部冻结（行为不变，26 个既有测试原样通过）。
配置统一为 `AiModelConfig`（构造层从 `TravelSettings.llm_*` / `AiSettings` 映射，settings schema 不迁移）。
理由：一套客户端/解析/错误模型；travel 的 `LlmProvider` 是历史包袱（V5_PROVIDER_CONSOLIDATION ≥0 条重复）。

### 2. ToolExecutor 升级为 async（平台演进，非业务分支）

V5 需要慢/IO 工具（`history.ensure_enrichment` 要联网检索 + 生成）。`ToolExecutor::execute`
由同步改 `#[async_trait] async`，PersonalAgent 循环 await —— 这是注册表平台的通用能力，
不改变「零业务 if/else」原则；`ToolRegistry::execute` 同步改 async。

### 3. 风险门禁启用 SafeWrite（仅 derived 写入）

`ToolRegistry::register` 允许 `Read + SafeWrite`（SensitiveWrite/System 仍禁）。
V5 唯一 SafeWrite 工具 = `history.ensure_enrichment`（只写独立富化缓存，绝不写 Canonical）。

### 4. History V3.1：Canonical = Truth Layer + On-demand Enrichment

- Canonical（duckdb 只读）不变；富化为 **derived 用户数据**（`config/history_enrichment.db`，gitignored）。
- 完整按需管线：状态检查 →（单飞）搜索 → 来源排序（去重/域名归一/类型分类/权威加权）→
  结构化生成（JSON envelope：section/content/claims[text,source_ids]/uncertainties/controversies）
  → 校验门（entity 存在 / content 非空 / 引用解析 / section 一致；失败 FAILED 不存 READY）。
- 状态机 MISSING/GENERATING/READY/STALE/FAILED/REVIEWED；STALE 由 TTL(30d) / canonical
  revision 指纹 / schema_version 三路派生；REVIEWED 禁止自动刷新，手动重生成 = 新 revision 候选。
- 单飞 + stale-while-revalidate + 首次渲染不阻塞（Canonical 照常显示）。
- 未配置搜索/模型 → FAILED(unavailable)，其余功能零影响（§63/§64）。

### 5. 模块接入模式（V5 验收：PersonalAgent 无业务 hardcode）

Travel / Geography / Language 均按同一模式接入：

```text
ModuleDescriptor （id/display_name/capabilities/tools）
+ tools（ToolExecutor 实现，module.action 命名，JSON Schema 参数，风险分级）
+ ModuleContextProvider（AppContext → 紧凑业务上下文，ContextBudget 截断）
+ composition root registration（desktop build_hub 一处调用 register_*）
```

`PersonalAgent` / `ToolRegistry` / `ModuleRegistry` 核心零业务修改（reviewer 核验 + agent 路由测试）。
新增模块的论据：V4 平台可扩展性被实测证明；History/Travel/Geography/Language 呈现同构接入。

## 后果

**正面**：单一模型接入点；History 获得按需富化（第一条把手）；四个标准模块 + 5(N)+4+4+4 个工具；
V5 验收指标（§110/§114）全部落地。

**代价/约束**：

- language.explain 的 LLM 插槽 setup 时读取配置（重启生效；主 agent 路径仍每调用读取）；
- 富化检索仅用 snippet + 短摘要（不存整页，§24）；来源权威加权为规则式（V6 可升级为模型评审）；
- async ToolExecutor 让所有工具实现必须 async（同步工具包一层 async 即可，成本低）；
- 前端无自动测试 runner（仓库既有事实）。

## 替代方案（已否决）

| 方案 | 否决理由 |
| --- | --- |
| travel 保留独立 LlmProvider | 双客户端/双配置/双错误模型，V5_PROVIDER_CONSOLIDATION 已列出重复点 |
| 富化直接写 canonical | 违反 Truth Layer；REVIEWED/版本化无法实现 |
| 每模块一个 Agent / if-module 分派 | V4 ADR-003 已否决；V5 最终验收明确要求零业务 hardcode |
| 富化 UI 阻塞首屏 | §28：首次渲染不阻塞是硬性要求 |
