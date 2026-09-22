# SELF-TOOLS V9 · MULTI-AGENT ORCHESTRATION — FINAL REPORT

- 日期：2026-09-22
- 基线：V8 冻结 HEAD `955d5ce`（687 passed / 0 failed，工作树干净，`--all-targets` 0 warning）
- 已提交 5 个 checkpoint；文档在工作树（保留未提交，与 V6/V7/V8 一致）。

## Status

**PASS**（Gates -1–11；独立安全审查 11 项发现全部修复）

---

## V8 Freeze

| 项 | 值 |
| --- | --- |
| V8 commits | `23803b5` / `f1c4a4b` / `96b7b51` / `61c7eaa` / `9e6f450` / `955d5ce`（6 个，含 4 份文档） |
| V8 regression（冻结时） | `cargo test --workspace` = **687 / 0**；`--all-targets` 0 warning |
| 起始 V9 git status | **clean** |

---

## Remote Runtime（Gate -0.5）

| 项 | 状态 |
| --- | --- |
| 生产 OIDC provider | **未接入**（本地无可连接 IdP）→ `DenyAllIdentityProvider` 保持，`identity_configured()` 恒 false，远程写关闭（§12 允许路径） |
| Fake identity | `StaticTokenIdentityProvider`（CI / 本地）；`AuthFailure` 四态 + provider_unavailable |
| MCP 生产 stores | **已接线**：`self-tools mcp --stores DIR` 装配与 desktop **相同**的 repository（memory/documents/files/server_actions 四个 SQLite），不建 `mcp_*.db`；4 个 runtime-gate 测试证明 MCP 读到真实数据 |
| 并发防护 | 拒绝指向已含业务库的目录（默认），防多进程 `SQLITE_BUSY`；显式 `allow_existing` 仅测试 / 一次性迁移 |

---

## Agent Matrix

| Agent | Role | Allowed Tools | Can Delegate | Max Steps | Status |
| --- | --- | --- | --- | --- | --- |
| `research` | 检索 / 取证 / 比较 | 只读模块白名单（memory/documents/files/knowledge/history/travel/geography/language/server 的 Read 级）；deny `memory.*`写 / `*.open` / `services.restart` | false | 4 | IMPLEMENTED |
| `planner` | 分解 / 排序 / 依赖 | 同上（无业务写） | false | 2 | IMPLEMENTED |
| `reviewer` | 证据 / 矛盾 / 完整性 | 同上 | false | 3 | IMPLEMENTED |
| `synthesizer` | 合并 / 去重 | 同上 | false | 2 | P1（profile 已注册，未进默认计划） |

## Orchestration Matrix

| 环节 | 实现 |
| --- | --- |
| Planning | 确定性规则：2 并行 research（服务器侧 / 知识侧）+ 可选 planner；`ExecutionPlan{tasks, parallel_groups, final_review_required}` |
| Delegation | `TaskEnvelope`（context_refs 只传引用）+ `DelegatedCapabilitySet = parent ∩ profile ∩ task` + `is_subset_of(parent)` |
| Parallelism | `tokio::Semaphore`（acquire 在 future 内部，避免组死锁）；组大小 ≤ `max_agents` |
| Budgets | `AgentBudget`（agents/steps/tool_calls/tokens/duration）；名额**预扣**；`child_budget` ≤ 父剩余；四维全部强制执行 |
| Timeout | per-agent `tokio::time::timeout`（min(profile, 父剩余)）→ `TimedOut`；全局 `max_duration_ms`；`deadline` |
| Cancellation | `CancellationToken` 透传每个 child；取消不留 zombie |
| Review | reviewer 五态裁决；仅 `needs_fix` 可 repair，且全局一次 |
| Repair | 一次（由 plan 的单一 reviewer 任务保证） |

---

## Capability Security

| 问题 | 答案 |
| --- | --- |
| Child 能否获得 Parent 没有的 Tool？ | **NO**（`intersect` + `is_subset_of(parent)` + 执行侧 allowlist） |
| Child 能否调用 SYSTEM？ | **NO**（risk 过滤 + denied + allowlist；`services.restart` 在 profile denied 列表） |
| Child 能否保存 Personal Memory？ | **NO**（`memory.save/archive/update` 在所有 profile denied） |
| Child 能否继续 Spawn Agent？ | **NO**（depth = 1；executor 无 delegate 入口） |
| Child 能否绕过 ToolRegistry？ | **NO**（唯一执行路径；allowlist 在 `run_tool_loop` 强制） |
| Child 能否绕过 MCP/SafeAction？ | **NO**（worker 无 SafeAction 入口；只能返回 `ActionProposal`） |
| Worker 输出是否当成可信指令？ | **NO**（untrusted 围栏 + 结构投影 + 截断；policy 明确围栏内非指令） |

## PersonalAgent Core Diff

**NO** —— 未变成巨型 Orchestrator。`agent.rs` 只多一个可选 stage
（`hub.orchestration`，与 V6 retrieval 同形态；未装配 = V8 行为）；
编排逻辑在独立的 `OrchestrationService`；测试断言 `agent.rs` 源码不含
agent/module 分支。

---

## Tests

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **749 passed / 0 failed**（V8 基线 687，+62） |
| `cargo check --workspace --all-targets` | 0 error / 0 warning |
| `npx tsc --noEmit`（ui） | 0 error |
| `npm run build`（ui） | built |

新增 62：core agents 21 / application agents 37（executor 6 + orchestrator 22 +
Gate 6 五个专项 + Gate 7 三个 + Gate 9 四个回归）/ MCP runtime gate 4 / trace 契约与其它。

---

## Regression

- V4（Registry / Tool / Action / UiBlock / Provider）：通过。
- V5（History / Travel / Geography / Language / ChatModelProvider）：通过。
- V6（Memory / Documents / Files / Allowed Roots / Secret Protection）：通过。
- V7（Server / Services / Logs / Apps / SafeAction / Confirmation / Audit /
  Rate Limit）：通过（本轮只统一 AI 票据 session 常量，行为向「可确认」修复）。
- V8（MCP STDIO / HTTP / Authorization / Exposure / SafeAction）：通过
  （41 个 MCP 测试全绿；`DenyAll` 默认不变）。
- History pipeline：`apps/server` 未改动，7 测试通过。

---

## Known Issues

1. 生产 OIDC provider 未接入（开发环境无 IdP）：远程 MCP 写能力保持关闭。
2. `apps/mcp --stores` 与 desktop 同时打开同一 SQLite 仍可能 `SQLITE_BUSY`
   （infra 未设 busy_timeout / WAL）；当前以「拒绝共开」缓解，属已知限制。
3. AI 会话的 cooldown / session_limit 仍按常量 `"ai-desktop"` 共享（跨 AI 会话
   不隔离）；MCP 侧每请求唯一。
4. Synthesizer / 成本估算 / 外部 agent worker 未实现（P2）。
5. 委派触发是确定性规则；「模型结构化决策」留待后续（§44）。

---

## Git Status

5 个 V9 checkpoint：
```
567f829 agents: orchestration contracts, bounded task runtime, executor and standard workers (Gates 1-6)
77261a9 mcp: wire production stores into the MCP runtime (Gate -0.5)
f110d19 agents: optional orchestration stage in PersonalAgent, capability isolation tests (Gate 7)
55317b7 core+app: orchestration trace view on AgentResponse (Gate 8 backend)
ee551f2 ui: orchestration progress and trace panel (Gate 8)
ce075fb security: fix all V9 review findings (Gate 9)
```
文档（`MULTI_AGENT_V9.md` / `ADR-008` / `V9_OVERNIGHT_STATUS.md` / 本报告）
在工作树。

---

## V10 Readiness

| 问题 | 答案 |
| --- | --- |
| 是否已有足够的真实 Agent run 数据？ | **部分**。trace 已记录每 run 的 agent/tool/status/duration/tokens，但尚无持久化运行库（生产编排未装配，数据只在测试中）。 |
| 是否已有任务/Agent/Tool usage metrics？ | **有结构**（`OrchestrationTraceView.runs` + `AgentBudget` 用量）；缺持久化与聚合。 |
| 是否存在可供 Decision Model 学习/评估的 routing features？ | **尚不充分**：需要先落库 + 采集线上委派决策与结果（当前 decide 是规则）。 |
| 当前 routing 是否还是 rule + LLM？ | **rule**（确定性规则 + 计划）；LLM 只做 worker 内推理。 |
| 是否可以开始 Decision Layer / Jev？ | **可以启动 P0 研究**，但建议先把「编排 trace 持久化 + 委派决策日志」作为 V10 第一个 Gate。 |
| Jev 是否能够只替换 routing decision，而不影响 Agent Runtime？ | **是**。`OrchestrationService::decide` 是唯一路由决策点，替换它不影响 executor / budget / SafeAction。 |
| PersonalAgent core 是否仍无需重构？ | **是**（四代零业务分支）。 |
