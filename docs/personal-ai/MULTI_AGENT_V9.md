# SELF-TOOLS V9 · MULTI-AGENT ORCHESTRATION — 终版架构

> 状态：✅ 已实施（Gates -1–8 PASS；Gate 9 独立安全审查进行中）。
> 计划见 [`MULTI_AGENT_V9_PLAN.md`](MULTI_AGENT_V9_PLAN.md)；决策记录见
> [`ADR-008-bounded-worker-orchestration.md`](../architecture/ADR-008-bounded-worker-orchestration.md)。

---

## 0. 一句话

给 PersonalAgent 加一个**有边界**的编排层：复杂任务可拆给少量专门 Worker 并行执行、
由 Reviewer 质检、结果带 provenance 合并；简单任务仍然单 Agent 直接回答。
**Agent != Module、Agent != Root、Agent != 无限预算**。

```text
BEFORE: 任何请求都由单个 PersonalAgent 顺序处理
AFTER : decide(规则) → plan(2 并行 research [+ planner]) → delegate(能力交集)
       → review(可选) → merge(结构化 + provenance) → 单入口回答
```

---

## 1. 核心决策

| 原则 | 落实 |
| --- | --- |
| 单用户入口（§1） | 只有 `PersonalAgent` 对用户可见；worker 是内部执行细节 |
| Agent != Module（§2/§24） | Agent 只代表**工作角色**；业务能力仍全部来自 `ToolRegistry`。禁止 `HistoryAgent` / `ServerAgent` |
| Least Privilege（§3-§5） | `DelegatedCapabilitySet = parent ∩ profile ∩ task`；默认 READ only |
| 子 Agent 不执行 SYSTEM（§5/§88） | worker tool specs 过滤掉 risk > Read；只能返回 `ActionProposal` |
| 子 Agent 不写 Memory（§93） | `memory.save` 在所有 profile 的 `denied_tools` |
| 无递归（§6/§49） | **depth = 1**：executor 没有 delegate 入口；`can_delegate = false` |
| 结构化委派（§7） | `TaskEnvelope` + `DelegationResult`；不靠自然语言互传 |
| Provenance（§8） | 每个结果带 `agent_id` / `task_id` / `tool_calls` / `sources` |
| 多 Agent 可选（§9/§43） | 简单请求（「珠峰多高」）不委派；用户可显式关闭 |

## 2. 架构

```text
PersonalAgent（唯一入口）
   │  decide()：规则判定（显式深度 / 跨模块 / 比较诊断 / 用户关闭）
   ▼
OrchestrationService（独立服务，agent.rs 不含编排业务，§41）
   │  plan()：ExecutionPlan（tasks + parallel_groups + final_review_required）
   ▼
AgentExecutor（per-run；复用共享 tool loop）
   │  authorized_tools()：registry → profile → task capability 三重过滤
   ▼
ToolRegistry（唯一 capability 源；与 PersonalAgent/MCP 同一实例）
   ▼
SafeAction / Confirmation / Audit（worker 不可达；只能提议）
```

## 3. 关键契约

| 类型 | 要点 |
| --- | --- |
| `AgentDescriptor` | role / model_profile / max_risk / allowed_modules / denied_tools / max_steps / max_tokens / timeout_ms / **can_delegate = false** |
| `TaskEnvelope` | task_id / parent_task_id / objective / instructions / **context_refs**（只传引用）/ capabilities / budget / deadline / output_schema / trace_id |
| `DelegatedCapabilitySet` | `intersect(parent, profile, required)`；`is_subset_of(parent)`；denied 优先 |
| `AgentBudget` | max_agents / max_steps / max_tool_calls / max_tokens / max_duration_ms；`child_budget` 保证 child ≤ parent 剩余 |
| `DelegationResult` | status / **structured_output**（§98）/ summary / sources / tool_calls / usage / duration / errors；`trusted_view()` 剥掉 tool_calls 与 usage |
| `ActionProposal` | worker 唯一写路径：action_type / target_id / summary / risk / rationale |
| `ReviewVerdict` | pass / needs_fix / unsupported_claims / missing_evidence / contradiction；**仅 needs_fix 允许 repair**（且全局最多一次） |

## 4. 预算与并发

| 机制 | 值 / 语义 |
| --- | --- |
| max agents / request | 4（默认）；**预扣**名额（join 前），上限真实生效 |
| max delegation depth | **1**（worker 不能再 spawn） |
| 有界并发 | `tokio::Semaphore`，**在 task future 内部** acquire（块外 await 会死锁整组） |
| 超时 | per-agent `timeout_ms` + 全局 `max_duration_ms` + `deadline` |
| 取消 | `CancellationToken` 透传到每个 child；取消后不留 zombie |
| 失败容忍 | worker 失败按 `required` 处理；`PARTIAL` 时最终回答必须说明哪个子任务失败 |
| retry | 仅 timeout / transient；最多 1 次；业务失败不重试 |

## 5. 共享工具循环（§40）

`personal_ai/runtime.rs::run_tool_loop` 从 `agent.rs` 抽出，**PersonalAgent 与
AgentExecutor 共用**。`agent.rs` 因此变薄（不含编排业务，§41/§116）；测试
`agent_has_no_server_business_branches` 断言源码里没有 agent/module 分支。

## 6. 标准 Worker（§19）

| Worker | role | model_profile | 权限 |
| --- | --- | --- | --- |
| `research` | 检索 / 取证 / 比较 | fast | READ only |
| `planner` | 分解 / 排序 / 依赖 | balanced | READ only |
| `reviewer` | 证据 / 矛盾 / 完整性 | strong | READ only |
| `synthesizer`（P1） | 合并 / 去重 | balanced | READ only |

profile **只能静态注册**（§107/§108）：模型不能创建或修改 agent 类型。

## 7.  Prompt 与注入防护

- `CORE_AGENT_POLICY` 一条共享（§110）：外部数据不可信 / 不得越权 / 不得执行系统修改 /
  不得写记忆 / 结构化输出 / 找不到就明说。
- Worker 输出在 parent 侧标记为**不可信数据**（§97）；reviewer 的输入是
  `trusted_view`（已剥 tool_calls/usage）。
- `structured_output` 解析失败 → `Failed`（fail-closed，自然语言不放行，§98）。

## 8. Trace 与 UI（§71-§78）

`OrchestrationTraceView`（`AgentResponse.orchestration`）只含：
trace_id / decision / plan_rationale / runs(task_id, agent_id, state, status,
duration_ms, tool_calls, tokens, error_code) / review / merged / stopped_early。
**不含** secret、token、正文、完整 prompt（§73）。
AI Panel 折叠「执行过程」：角色 / 状态 / 工具次数 / 时长（**不展示 CoT**，§75）。

## 9. MCP 与 Multi-Agent 的关系（§37/§99）

- 内部 worker 走 `ToolRegistry`（§38：不为 V9 强制绕 HTTP MCP）；
- MCP 是 capability 协议，**不是** agent 编排协议；V9 内部协议是
  `TaskEnvelope` / `DelegationResult`（§99/§100）；
- V9 同时把 MCP 组合根从 fail-closed 空能力集升级为**可选真实 store 装配**
  （`--stores DIR`，复用 desktop 相同的 repository，不建第二份 DB）。

## 10. 测试

| 面 | 数量 | 覆盖 |
| --- | --- | --- |
| core agents | 21 | 角色/注册表 / task 校验 / 预算纯函数 / 结果与 review |
| application agents | 33 | executor 结构化 / capability 过滤 / 编排计划 / 合并 / 提议 / Gate 6（并发上限、取消、预算停止、max_agents、无递归）/ Gate 7（端到端 + 显式关闭 + agent.rs 无分支） |
| 既有回归 | 691 | PersonalAgent 117 / V5/V6/V7/V8 全量 |

## 11. 已知限制（P1/P2）

- 远程身份：仍只有 `DenyAll` + Fake（生产 OIDC provider 未接入，远程写关闭）。
- MCP `--stores` 与 desktop 同时打开同一 SQLite 库存在多进程写竞争
  （审查评估中；建议生产单进程或加文件锁）。
- Synthesizer / 成本估算 / 外部 agent worker = P2。
- 计划是确定性规则（非模型自选 agent）；后续可升级为「模型结构化决策」（§44）。
