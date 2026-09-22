# SELF-TOOLS V9 · MULTI-AGENT ORCHESTRATION — PLAN

> Gate 0 审计结论 + 七 Track 实施计划。基线：V8 冻结 HEAD `955d5ce`
> （工作树干净，`cargo test --workspace` = **687 passed / 0 failed**，
> `cargo check --workspace --all-targets` = 0 warning）。

---

## 1. Gate 0 审计（逐项核实）

### 1.1 复用资产

| 面 | 位置 | 事实 | V9 决策 |
| --- | --- | --- | --- |
| 编排资产 | 全仓 grep `Task/Job/Run/Worker/Planner/Trace/CancellationToken` | **零命中**（仅 Markdown 的 `task_state`，无关域） | 全新域 |
| `PersonalAgent` 工具循环 | `personal_ai/agent.rs:132-175` | `loop { chat → tool_calls? → execute → 回喂 }`，`max_tool_rounds = 4`，usage 累计 | **抽取共享 runtime**（§40）：`AgentExecutor` 复用同一循环，不 copy-paste |
| `ChatModelProvider` | `core/src/personal_ai/provider.rs` | `name()` + `async chat(ChatRequest)`；usage 在响应内 | worker 直接用（§83-§85：model_profile 映射 + 回落默认） |
| `ToolRegistry` | `personal_ai/registry.rs` | `specs()` / `spec()` / `async execute()`；注册期 `allowed_risk` | worker 的**唯一** capability 源（§2） |
| `ToolRisk` 四级 | core | Read / SafeWrite / SensitiveWrite / System | worker capability 交集按 risk 过滤（§34-§36） |
| `SafeActionService` | `server/action.rs` | plan / confirm_and_execute / 票据 / cooldown / audit | 子 Agent 不碰；只返回 `ActionProposal`（§88-§91） |
| MCP exposure 表 | `core/src/mcp/exposure.rs` | tool → group + scope + remote_visible | worker 工具过滤的**现有先例**（同款交集思路） |
| 会话 | `personal_ai/session.rs` | 内存 store，上限 60 条 | worker 用独立短会话，不复用用户会话（§26） |

### 1.2 V8 未决前置（Gate -0.5）

V8 Final Report 标记两个前置：**真实 Remote Identity** 与 **MCP 完整 store 装配**。

本仓事实：
- `DenyAllIdentityProvider` 拒一切远程；MCP Phase-1 组合根是 fail-closed 空能力集
  （空 registry / 空服务表 / 内存审计）。
- **开发环境无可连接的 OIDC Provider**（无配置、无网络端点）。

V9 决策（§12 允许的路径）：
1. **Remote Identity Productionization → 部分**：抽象已就绪；新增
   `OidcIdentityProvider`（**配置驱动**：issuer / client_id / audience / jwks_url /
   scopes / clock_skew），默认未配置 → 保持 `DenyAll` 行为。**不自己实现
   Authorization Server**（§5）。用 `FakeOidcProvider` 测试完整链路（§128），
   远程写能力保持 disabled 直到生产配置存在。
2. **MCP Full Store Wiring → 部分**：`apps/mcp/src/compose.rs` 增加
   `--stores <config-dir>` 模式，装配**与 desktop 相同的** repository 抽象
   （`MemorySqliteStore` / `DocumentIndexSqliteStore` / `FileIndexSqliteStore` /
   `HistoryDuckDbRepository` / `ServerActionAuditSqlite`）→ **不建第二份 DB**（§10）。
   Runtime gate 测试（§11/§129）：经 MCP 调 `memory.search` 等必须读到真实 store。

### 1.3 依赖方向（不变式）

```text
core            agents/{descriptor,task,budget,result}.rs（纯契约）
application  →  core only（AgentRegistry / TaskRuntime / AgentExecutor /
                          OrchestrationService / worker profiles / OIDC provider）
infrastructure →  core（SQLite/JWKS 等实现）
apps/mcp       =  transport（+ store 装配模式）
apps/desktop   =  组合根 + UI（进度 / trace / stop）
```

`application → infrastructure` = 0 保持；`std::process` 在 core/application = 0 保持。

---

## 2. 目标架构

```text
                          User
                           │
                     PersonalAgent（唯一用户入口，§1）
                           │
                    OrchestrationService（§41：不塞进 agent.rs）
                           │
              plan → delegate → parallel → review → merge
                           │
        ┌──────────────────┼──────────────────┐
     Research            Planner            Reviewer
     (READ only)      (no write)        (no write)
        └──────────────────┼──────────────────┘
                           │
                       ToolRegistry（唯一 capability 源）
                           │
        Modules / Knowledge / Server / SafeAction / Audit
```

---

## 3. Track A · V8 Runtime Productionization（Gate -0.5）

见 §1.2。**降级策略**：若 OIDC 无法在开发环境验证，只交付
`FakeOidcProvider` + 保持远程写 disabled（§12/§152）。

## 4. Track B · Agent Contracts + Registry（Gate 1）

### 4.1 core 契约（`crates/core/src/agents/`）

| 类型 | 内容 |
| --- | --- |
| `AgentRole` | `Research / Planner / Reviewer / Synthesizer`（§19：第一版 3+1） |
| `AgentDescriptor` | id / role / description / model_profile / allowed_risk / allowed_modules / max_steps / max_tokens / timeout / can_delegate（§18） |
| `AgentRunState` | `Pending / Running / Completed / Failed / Cancelled / TimedOut`（§30） |
| `TaskEnvelope` | task_id / parent_task_id / objective / instructions / context_refs / allowed_tools / denied_tools / max_steps / max_tokens / timeout / deadline / output_schema / priority / trace_id（§25） |
| `AgentBudget` | max_agents / max_steps / max_tool_calls / max_tokens / max_duration_ms（§52-§53） |
| `DelegationResult` | task_id / status / structured_output / summary / sources / tool_calls / usage / duration_ms / errors（§29） |
| `ActionProposal` | action_type / target_id / summary / risk / rationale（§89：子 Agent 唯一写路径） |

### 4.2 application

- `AgentRegistry`（§17）：注册表（与 ModuleRegistry/ToolRegistry 同构）；
  profile **只能静态注册**，模型不可修改/创建（§107/§108）。
- `TaskRuntime`：创建 child task（结构化校验）、状态机、预算扣减。

## 5. Track C · AgentExecutor + 共享 Tool Loop（Gate 2-3）

- 从 `agent.rs` 抽取 **`run_tool_loop`** 共享函数（messages + tools + budget →
  最终 content + usage + tool trace）；`PersonalAgent` 与 `AgentExecutor` **共用**
  （§40：不 copy-paste；§116：agent.rs 不膨胀）。
- `AgentExecutor`（§39）：load profile → 构建 prompt（`AgentPromptBuilder`，
  §110 共享 core policy + role + envelope + context + allowed tools）→
  执行循环 → 返回 `DelegationResult`。
- **capability sandbox**（§34-§36）：`DelegatedCapabilitySet =
  parent ∩ profile ∩ task`；child ⊆ parent（**编译期 + 运行时双重断言**）。
- 子 Agent 默认 **READ only**（§4）；SYSTEM/SENSITIVE_WRITE 工具**不进入**
  worker 的 tool specs（§88）；`memory.save` 默认不给（§93）。

## 6. Track D · Orchestrator（Gate 4）

- `OrchestrationService`（§41/§42）：decide → plan → delegate → parallel →
  collect → review → merge。
- **Delegation 触发**（§44）：rule + 模型结构化决策。简单请求（如「珠峰多高」）
  直接走 PersonalAgent 自己的工具循环，**不启动 worker**（§43/§105）。
- `ExecutionPlan`（§46）：tasks / dependencies / parallel_groups /
  final_review_required（DAG 基础模型，§47）。
- **第一版限制**（§48/§49）：max agents per request = 4；**max depth = 1**
  （worker 不允许再 spawn worker）。
- 有界并发（§50/§51）：`max_parallel_agents` 2–4。
- 全局 `AgentBudget`（§52-§54）；child 总和 ≤ parent。
- 超时（§57）：per-agent + 整体。
- 取消（§58/§59）：`tokio::select!` / `CancellationToken`，不留 zombie。
- 失败处理（§60-§63）：worker 失败按 required/optional 决定；
  `PARTIAL` 明确说明哪个子任务失败；retry 仅对 timeout/transient，最多 1 次。

## 7. Track E · Specialist Workers（Gate 5）

| Worker | 职责 | 权限 |
|---|---|---|
| `research` | search / retrieve / compare / collect evidence（§20） | READ only |
| `planner` | decompose / sequence / dependencies（§21） | READ only（无业务写） |
| `reviewer` | check evidence / contradictions / completeness / unsupported claims（§22） | READ only；与 producer 分离 |
| `synthesizer` | merge worker outputs（§23，P1） | READ only |

## 8. Track F · Review / Quality Gate + Security（Gate 6-7）

- Reviewer 输出（§66）：`PASS / NEEDS_FIX / UNSUPPORTED_CLAIMS / MISSING_EVIDENCE /
  CONTRADICTION`；不直接改数据（§67）。
- **Repair 最多 1 次**（§68）：禁止 writer↔reviewer 无限循环。
- Grounding（§69/§70）：结论保留来源；trace 记录 agent/tool/source。
- 跨 Agent 注入（§96/§97）：worker 输出按 **untrusted worker result** 处理，
  除非过结构校验（§98：优先 structured output）。
- 内部 agent 间协议 = `TaskEnvelope`/`DelegationResult`，**不用 MCP A2A**（§99/§100）。
- 审计（§113/§114）：`AuditSource` 增加 `Agent`，区分 PersonalAgent / MCP / SubAgent。

## 9. Track G · Trace + UI（Gate 8）

- `OrchestrationTrace`（§71/§72）：trace_id / parent request / plan / runs /
  tool_calls / timings / usage / statuses / review result。
- **不记录**（§73）：API key、token、完整私有文件/记忆、完整 prompt。
- UI（§74-§78）：AI Panel 折叠「查看执行过程」；只展示 task/role/status/
  tools/sources/duration（**不展示 CoT**，§75）；Stop 按钮取消子 run。
- 设置（§79/§80）：AI → Advanced：`multi_agent_enabled` / `max_workers` /
  `max_duration` / `review_mode`。
- 用户显式控制（§81/§82）：「不要使用多 Agent」必须遵守；「深入研究」是强信号。

---

## 10. Security / Threat Model（Gate 9）

| 威胁 | 缓解 |
| --- | --- |
| capability escalation | `DelegatedCapabilitySet = parent ∩ profile ∩ task`；child ⊆ parent 双重断言 |
| parent→child scope leak | TaskEnvelope 只带必要 context refs（§26/§27），不带全会话 |
| SYSTEM tool leak | worker tool specs 过滤掉 risk ≥ SensitiveWrite；只返 `ActionProposal` |
| memory write leak | `memory.save` 不在 worker allowed 集合 |
| recursive delegation | depth = 1；worker 调 delegate → Denied |
| budget / timeout bypass | 全局 budget 在 runtime 扣减；per-agent + 整体超时 |
| cross-agent prompt injection | worker 输出标记 untrusted；schema 校验后才进 merge |
| worker output spoofing | structured output + provenance（agent_id/tool/source） |
| trace private-data leakage | trace 只存 id/计数/时长/状态 |
| 模型自建 agent | AgentRegistry 静态注册；模型只能选 id（§45） |

---

## 11. Test Matrix

| 面 | 用例 |
| --- | --- |
| Agent core（§117） | registry / descriptor / task envelope / budget / capability 交集 / result schema |
| Delegation（§118） | 简单请求→不委派；复杂→2 workers；capability ⊆ parent；denied tool → denied |
| Recursion（§119） | worker 尝试 delegate → denied |
| Budget（§120） | max_agents / max_steps / max_tokens / timeout 达到后安全停止 |
| Parallel（§121） | 两个独立 worker 并发且不超过上限 |
| Cancellation（§122） | 取消 parent → 所有 child 取消 |
| Timeout（§123） | 一个 worker 超时，其它结果仍收集 |
| Reviewer（§124） | unsupported claim 被抓；valid → PASS；repair 仅一次 |
| SafeAction（§125） | worker 无 SYSTEM 工具 |
| Memory（§126） | worker 无 `memory.save` |
| Trace（§127） | run/tool/usage/status 可追踪；无 secret |
| Remote Identity（§128） | FakeOidc：valid / expired / wrong issuer / wrong audience / unknown key / scope mismatch / provider unavailable |
| MCP stores（§129） | 经 MCP 调 memory/documents/history/server 读到真实 store |

---

## 12. Gate 顺序与 PASS 判据

| Gate | 内容 | PASS |
| --- | --- | --- |
| -1 | Freeze V8 | 工作树干净；687 基线绿 |
| -0.5 | Remote Identity + MCP stores | OIDC provider（配置驱动）或安全降级；MCP 读真实 store（Runtime gate） |
| 0 | Audit + Plan | 落盘；确认零编排资产 |
| 1 | Contracts + Registry | `AgentDescriptor/AgentRegistry/TaskEnvelope/AgentRun/DelegationResult/AgentBudget` 编译 + 单测 |
| 2 | Task runtime | Parent 可创建结构化 child task |
| 3 | AgentExecutor + shared loop | Worker 用 ChatModelProvider + 授权 ToolRegistry 完成任务 |
| 4 | Orchestrator | plan / parallel / collect / merge |
| 5 | Workers | research / planner / reviewer 可用 |
| 6 | Budget + Concurrency + Cancellation | 全部 enforced |
| 7 | SafeAction / Security | 子 Agent 无法提权 / 执行 SYSTEM / 静默写 memory |
| 8 | Trace + Frontend | 进度 / worker 状态 / 工具来源 / Stop |
| 9 | Security Review | 独立 reviewer；无 HIGH/CRITICAL 未修 |
| 10 | Full Regression | workspace + all-targets + tsc + build 全绿；V5-V8 不退化 |
| 11 | Docs | `MULTI_AGENT_V9.md` / `V9_OVERNIGHT_STATUS.md` / `V9_FINAL_REPORT.md` + `ADR-008` |

---

## 13. Scope 控制

**本轮不做**（§16）：Jev/Decision Model、自主无限 agent、agent marketplace、
agent 动态写 agent、agent 生成 shell、Computer Use、浏览器自动化、发邮件、支付、
智能家居、自动 OS 管理。Synthesizer、成本估算、外部 agent worker = P2。

**降级策略**（§159）：宁可只有 Research + Reviewer 两个 worker，也不为数量破坏
安全与可控。

## 14. Rollback

`core/src/agents` + `application/src/agents` + UI trace 区可独立删除；
`PersonalAgent` 保持「可选 OrchestrationService」注入（不装配即回到 V8 形态）；
OIDC provider 未配置时行为与 `DenyAll` 完全一致；MCP stores 装配失败即回退
fail-closed 空能力集。
