# SELF-TOOLS V4 · PERSONAL AI HUB — PLAN

> 定位：`Personal Digital Hub` 的地基。本轮产出**一个 PersonalAgent**、
> Registry-based 模块接入机制、App Context、Tool Calling 循环、History 标准模块
> 接入与前端 AI Panel。不实现 Multi-Agent / MCP / 长期记忆。
>
> 计划日期：2026-09-16（本文件为 Gate 0 交付物；实施状态见
> [`V4_OVERNIGHT_STATUS.md`](V4_OVERNIGHT_STATUS.md)，最终架构见
> [`PERSONAL_AI_HUB_V4.md`](PERSONAL_AI_HUB_V4.md)）。

---

## 1. V3 现状核对（审计结论，非假设）

以当前代码为准（`git status` 干净，HEAD `b45b517`，submodule `history-data-pipeline@8c6cf71`）：

| V3 组成 | 实际状态 | 证据 |
| --- | --- | --- |
| Canonical Knowledge Graph | ✅ 存在 | `history-data-pipeline/data/curated/history_backbone/` + `dist/history.duckdb`（只读事实源） |
| Evidence | ✅ 存在 | `backbone/evidence_link.py`、quality 门禁 evidence_precision 维度；DuckDB `event_evidence` 表（work/term/chapter_hint/historical_text_id） |
| 批量 Enhancement（写时补全正文） | ✅ 存在 | `backbone/quality.py`（100 分确定性评分，content_completeness 10 分奖励正文）+ `enrichment.py` qa-run；事件带 `summary/background/process/result/impact_zh_cn` 长文本 |
| **On-demand AI Enrichment（Enrichment Cache / ensure_enrichment）** | ❌ **未实现** | 全仓 grep `ensure_enrichment | enrichment_cache | on_demand | HistoryContextBuilder` = 0（仅 node_modules 噪音）；Rust/TS 无联网富化 |
| Reviewed protection | ✅ 存在 | `quality_status ∈ {verified,reviewed,accepted}` 计分；canonical 不写 AI 输出（AGENTS.md 反伪造不变量） |
| Repository（只读 DuckDB） | ✅ 存在 | `crates/infrastructure/src/history/duckdb.rs` 只读 SELECT；`apps/desktop/src/history_query.rs` adapter |
| Migration | ✅ 已完成 | `docs/migration/09-history-v2-cutover-audit-2026-09-10.md`；V1 legacy 代码已删 |

**结论**：V3 = 「Canonical + Evidence + 写时批量补全」已完成；「On-demand AI Enrichment」
（上一阶段设计讨论的 旧模式→新模式）**未落代码**。

**V4 处置（遵守「不要趁 V4 重写 V3」）**：

1. V4 把 V3 当作**只读依赖**，不重建数据模型、不重拆 Canonical、不做 Enrichment 改造。
2. V4 只向模型暴露 V3 **已真实存在**的能力：`search` / `get_event` / `get_person` /
   `get_context`。**不暴露 `ensure_enrichment`**（V3 无此能力，暴露=范围蔓延）。
3. `get_event` / `get_person` 的「available enrichment state」= 库内已有的
   `quality_status` + evidence 摘要，如实透传，不伪造。
4. V3 的 on-demand enrichment 另行规划（V5 候选），V4 的 Context Provider 预留
   「existing enrichment」字段位，未来接上即可，不改协议。

---

## 2. Current Architecture（本轮基线）

```text
Frontend (apps/desktop/ui, React 19 + Vite 7 + TS)
  └─ 8 个 feature client → transport.ts → tauriTransport.invoke
apps/desktop (唯一组合根)
  └─ lib.rs 46 个 command + AppState + setup + composition.rs + travel_providers.rs
       │
       ├─ crates/application  use cases + 端口（History/Geography/RSS/Language/Travel/Workflows）
       ├─ crates/core         纯域 + 契约（history_records / settings / travel::provider）
       └─ crates/infrastructure  实现（DuckDB / SQLite / HTTP）
apps/server  只读 History 试点（Gate 9），本轮不扩展
```

已有可复用的 AI 「硬件」：

- `crates/core/src/travel/provider.rs`：`SearchProvider` / `WebFetcher` /
  `LlmProvider` / `TravelDataProvider`（travel 口味，本轮不动）。
- `crates/infrastructure/src/travel/llm.rs`：`OpenAiCompatibleLlmProvider` —
  OpenAI-Compatible `/chat/completions`，`LlmConfig{base_url,api_key,model}` +
  `is_configured()` + 120s timeout。**作为 ChatModelProvider 实现的模板**。
- `crates/core/src/settings.rs`：`AppSettings`（`serde(default)` 每字段结构）—
  travel.llm_* 已有 key 配置先例（存 gitignored `config/settings.json`）。

---

## 3. Target Architecture（V4）

```text
                      Frontend
                          │
                ┌─────────┴─────────┐
                │                   │
           Normal Modules       AI Interface (AI Panel)
                │                   │
                │                   ▼
                │            PersonalAgent (application)
                │                   │
                │           ┌───────┼────────┐
                │           │       │        │
                │        Context   Tools   Provider
                │           │       │
                │           │       ▼
                │           │  ToolRegistry
                │           │       │
                │           │       ▼
                │           │ ModuleRegistry（含 ContextProvider、Tool 描述）
                │           │       │
                └───────────┼───────┘
                            ▼
                 Application Services（HistoryService 等，经端口）
```

**依赖方向（遵守项目 clean architecture / ADR-001）**：

```text
crates/core/personal_ai     纯类型 + 错误 + ChatModelProvider 契约（无内部依赖）
        ↑
crates/application/personal_ai   PersonalAgent / ModuleRegistry / ToolRegistry /
        │                         ContextProvider / Session / Prompt 组装 / History 适配器
        │                         （只依赖 core + application::history 等 use case，零 infra 引用）
        ↑
crates/infrastructure/personal_ai  OpenAiCompatibleChatModelProvider（reqwest 实现）
        ↑
apps/desktop（组合根）          装配（personal_ai_service 构造器）+ Tauri 命令
```

禁止：`application → desktop`、`domain → infrastructure`。

---

## 4. Module Registry

```rust
// core/personal_ai
pub struct ModuleDescriptor {
    pub id: String,            // "history"
    pub display_name: String,  // "History"
    pub description: String,
    pub capabilities: Vec<String>, // ["search","entity","enrichment_state"]
    pub tools: Vec<String>,        // ["history.search", ...]
}
```

- `ModuleRegistry`（application）持有 `ModuleDescriptor` 集合 + 各模块的工具执行器
  与 Context Provider；注册 API：`register_module(ModuleRegistration)`。
- 新增模块 = 注册一个 descriptor + 工具 + context provider，**不改 PersonalAgent**。

## 5. Tool Registry

```rust
pub struct ToolSpec {
    pub name: String,          // "history.search"
    pub description: String,
    pub input_schema: serde_json::Value, // JSON Schema
    pub risk: ToolRisk,        // Read / SafeWrite / SensitiveWrite / System
    pub module: String,
}
pub enum ToolRisk { Read, SafeWrite, SensitiveWrite, System }
pub struct ToolResult { pub ok: bool, pub data: serde_json::Value,
                        pub error: Option<String>, pub metadata: serde_json::Value }
```

- `ToolRegistry`：`discover()` / `schema(name)` / `validate_args(name, value)` /
  `execute(name, value)`；执行器为 `Box<dyn ToolExecutor>`（application 内注册，
  组合根只做装配）。
- V4 History tools 全部 `Read`。无 `SensitiveWrite/System` 注册能力（API 预留，
  组合根恒拒绝注册 —— 安全边界在组合根强制）。

## 6. App Context（P0）

```rust
pub struct AppContext { pub module: Option<String>, pub page: Option<String>,
                        pub entity: Option<EntityRef>, pub selection: Option<SelectionRef>,
                        pub view_state: serde_json::Value }
pub struct EntityRef { pub kind: String, pub id: String, pub label: Option<String> }
```

- **Context 所有权**：Frontend 负责「我在哪」（module/page/entity），
  模块的 ContextProvider 负责「这个 entity 的业务上下文」。
- `ModuleContextProvider`（application）：`build_context(&AppContext, &ContextBudget) -> ContextBundle`。
- **Context Budget**：current entity first → page context → direct relations → extras；
  `ContextBudget { max_items, max_chars }` 硬截断（不实现 token optimizer）。

## 7. Action Protocol

```rust
pub enum ActionKind { Navigate, OpenEntity, RefreshView, ShowPanel }
pub struct Action { pub kind: ActionKind, pub module: String,
                    pub target: serde_json::Value }
// 示例: {"kind":"Navigate","module":"history","target":{"type":"event","id":"..."} }
```

- **执行边界**：AI 只产出 `Action Request`；**Frontend 决定是否执行**
  （renders 后由 `onNavigate` 回调驱动，模型无 router 引用）。
- 高风险 Action（删除/写库/Shell/发送）V4 不支持；ToolRisk 门禁 + 组合根装配时
  注册表只允许 Read（本轮）。

## 8. UI Block Protocol

```rust
pub enum UiBlockKind { EntityList, EntityCard, SourceList, KeyValue, TimelinePreview }
pub struct UiBlock { pub kind: UiBlockKind, pub title: String, pub data: serde_json::Value }
```

- V4 实现 `EntityList`（含 Kind/Title/Subtitle/Years 的实体卡片数据）渲染；
  其余 kind 预留、前端优雅降级为列表展示。

## 9. PersonalAgent（application/personal_ai/agent.rs）

循环：

```text
User → AgentRequest{messages, session_id, app_context, capabilities, locale}
  └─ Prompt 组装（core system + context + tool 描述 + 会话历史）
  └─ ChatModelProvider.chat(req) → content / tool_calls / usage
       ├─ tool_calls? → ToolRegistry.validate + execute（每步 timeout）
       │     ├─ ToolResult 追加为消息 → 再调模型（round += 1，≤ max_tool_rounds）
       │     └─ 超限 → AgentError::MaxToolRounds
       └─ 无 tool_calls → 解析最终结构化 envelope（message + actions + ui_blocks）
  └─ AgentResponse{message, actions[], ui_blocks[], tool_trace[], usage}
```

- 不负责：History/Travel DB 实现、前端路由、API key 存储（key 只进 Provider 配置）。
- `max_tool_rounds` 默认 4，可配置；工具/模型调用均有 timeout。

## 10. AI Provider

- 复用「OpenAI-Compatible」路线：新建 `ChatModelProvider`（core 契约：
  messages + tools + usage）与 infra 实现 `OpenAiCompatibleChatModelProvider`。
- travel 的 `LlmProvider` 保持不动（V5 候选：统一到 ChatModelProvider，本轮禁止
  改 travel —— §117 范围约束）。
- 配置：`AppSettings` 新增 `ai: AiSettings{ provider, model, base_url, api_key, timeout_secs }`
  （`serde(default)` 向后兼容旧 settings.json），现实化规则同 travel：cache 在
  gitignored `config/settings.json`；env 覆盖 `SELF_TOOLS_AI_*` 可选。
- **No API Key Gate**：无 key → `configured=false` → AI Panel 显示未配置，
  普通模块与 History 完全正常。

## 11. History 模块（第一个标准模块）

- `ModuleDescriptor{ id:"history", display_name:"History", tools:["history.search",
  "history.get_event","history.get_person","history.get_context"] }`
- 工具实现复用 `HistoryService`（application use case，经 `HistoryQueryPort` 假件可测）：
  - `history.search(query, entity_type?, period?, limit?)` → 精炼命中列表（id/type/name/years/摘要）
  - `history.get_event(id)` → canonical + relations + enrichment_state(quality_status/evidence 摘要)
  - `history.get_person(id)` → canonical + relations + events + timeline
  - `history.get_context(app_context)` → 当前实体 compact context（budget 截断）
- `HistoryContextProvider` 实现 `ModuleContextProvider`；复用 V3 详细数据（service
  方法直取），不新建 V3 数据面。
- **长期不做**：把 History 变成 AI 中心（§122：PersonalAgent 必须可服务任意模块）。

## 12. Frontend Entry

- 顶栏「Ask AI」按钮（仿 Settings 齿轮）+ 右侧滑出 **AI Panel**（侧栏，非全屏）；
  窄屏由 CSS 降级（panel 宽度自适应/全宽覆盖）。
- `aiClient.ts`（沿用 feature client 模式）：
  `personal_ai_status()` / `personal_ai_chat(request)`。
- AppContext 桥：App.tsx 维护 `{module,page,entity}`（History 页上报当前 detail
  entity），每次发消息随请求携带；Panel 顶部显示「当前上下文：History · 毛泽东」chip，
  可清除（General）。
- 状态机：idle / thinking / using_tool / done / error / unconfigured。
- Action 渲染：Navigate/OpenEntity → 调用既有 `openHistory(id)` 等回调；
  UI Block EntityList → History 卡片列表。

## 13. Session（V4 = Session Memory，非长期记忆）

- `SessionStore`（application 抽象）：`session_id → Vec<ChatMessage>`，内存实现、
  上限裁剪（每会话保留最近 N 条）。持久化 = P1（不混入长期记忆）。
- Conversation ≠ Memory；History 知识 ≠ Personal Memory（未来 PersonalMemoryStore
  只定义边界，不实现）。

## 14. Security Boundary

- API key 不进前端、不进日志、不进 git（沿用 settings.json 于 gitignored config/）。
- 模型输出只在 UI 渲染为**注解**；不写 canonical（duckdb 只读）。
- 工具风险门禁：注册表只装配 Read 工具；`SensitiveWrite/System` 枚举存在但组合根
  无注册路径（编译期 + 装配期双重防线）。
- Prompt 组装拼装 tool schema 时：只注入已注册模块的工具。

## 15. 非目标（本轮不做）

Multi-Agent / MCP / Jev / 长期记忆 / Vector DB / 全项目 RAG / Documents·Travel·
Geography·Language AI 改造 / 家庭自动化 / 服务器控制 / 高风险自动执行 / 语音 /
原生 App / 流式（P1） / 取消生成（P1） / 快捷键（P1）。

## 16. 测试策略

- core：类型 serde 往返、ToolRisk/ToolSpec 校验、Action/UiBlock 序列化。
- application：FakeChatModelProvider 五场景（text-only / tool call → result → final /
  unknown tool / tool exception / max rounds）；ToolRegistry 参数校验；ModuleRegistry
  注册与发现；History 模块四工具（Fake HistoryQueryPort 复用 history/tests.rs 模式）。
- infra：`extract_chat_content` 类纯解析函数单测（含 tool_calls 解析）。
- 前端：无测试 runner（既有事实），以 `npm run build`（tsc + vite）为准 +
  reviewer 人工核验。
- 回归：`cargo test --workspace`、`cargo check --workspace --all-targets`、
  `npm --prefix apps/desktop/ui run build`、History validation/测试（submodule）。

## 17. Gate 序列（严格顺序）

| Gate | 内容 | PASS 判据 |
| --- | --- | --- |
| 0 | 审计 + V3 核对 + 本计划 | 本文档落盘；V3 状态结论 |
| 1 | Core contracts | AgentRequest/AgentResponse/Tool/ToolResult/ModuleDescriptor/AppContext/Action 稳定 |
| 2 | ModuleRegistry + ToolRegistry | 新 Tool 不需改 PersonalAgent；新 Module 无 if/else |
| 3 | AppContext + ContextProvider | 前端可传 AppContext；HistoryProvider 可解析 |
| 4 | PersonalAgent + Provider + loop | Fake 五场景全过 |
| 5 | History 集成 | 4 工具可被 Agent 调 |
| 6 | 前端 Ask AI + Panel + Context 桥 | UI 可开面板、见 context、发问、得答 |
| 7 | Actions + UI Blocks | Navigate + EntityList 可渲染执行 |
| 8 | 测试 + 回归 | 全绿 + History V3 无退化 |
| 9 | 文档 + ADR + 架构 review | V4 文档/ADR/status 落盘；reviewer 通过 |

## 18. 接口速查（命名锚）

- Core：`crates/core/src/personal_ai/{mod,types,error,provider}.rs`
- Application：`crates/application/src/personal_ai/{mod,agent,registry,context,session,prompt,history}.rs`
- Infrastructure：`crates/infrastructure/src/personal_ai/{mod,llm.rs}`
- Desktop：`apps/desktop/src/personal_ai.rs`（装配）+ lib.rs 命令
- Frontend：`apps/desktop/ui/src/features/ai/{aiClient.ts,AIPanel.tsx,…}`
- Docs：`docs/personal-ai/PERSONAL_AI_HUB_V4.md`（最终架构）、
  `V4_OVERNIGHT_STATUS.md`（进度）、`docs/architecture/ADR-003-personal-ai-hub.md`
