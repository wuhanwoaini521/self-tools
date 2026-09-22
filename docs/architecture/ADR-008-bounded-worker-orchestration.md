# ADR-008 · Bounded Worker Orchestration：单用户入口 + 最小权限委派

- 状态：**Accepted**（2026-09-22，V9 Gates -1–8 PASS，Gate 9 审查中）
- 领域：`crates/core` / `crates/application` / `apps/mcp` / `apps/desktop`
- 关联：[ADR-005](ADR-005-personal-knowledge-layer.md)（V6）、
  [ADR-006](ADR-006-safe-actions.md)（V7）、
  [ADR-007](ADR-007-mcp-as-adapter.md)（V8）、
  [V9 计划](../personal-ai/MULTI_AGENT_V9_PLAN.md)

## 背景

V4–V8 已形成：Module/Tool Registry（typed capability）→ Risk 门禁 →
SafeAction（确认 + 审计）→ MCP（外部入口）。缺口是**复杂任务**：一次
PersonalAgent 调用要同时看服务器日志、个人文档和历史记录时，单线程顺序
处理的上下文与质量都受限。

诱惑是「上 Multi-Agent」；风险是它很容易变成第二条权限通道、递归失控、
上下文爆炸。V9 必须在**不新增任何能力**的前提下提升复杂任务质量。

## 决策

### 1. Agent 是 Worker，不是 Domain

`AgentRegistry` 表达**工作角色**（research / planner / reviewer /
synthesizer），业务能力仍全部来自 `ToolRegistry`。禁止 HistoryAgent /
TravelAgent / ServerAgent（§24）。profile 只能静态注册，模型不能创建或
修改（§107/§108）。

### 2. 单用户入口

只有 `PersonalAgent` 对用户可见。编排是 `hub.orchestration` 上的**可选**
stage（与 V6 retrieval / V8 orchestration 同形态）：未装配 = V8 行为。

### 3. Capability 交集，子集不可放大

`DelegatedCapabilitySet::intersect(parent, profile, task)`；`is_subset_of(parent)`
在编排期校验。worker 的 tool specs 经过三重过滤（registry → profile 静态
规则 → task capability），默认 READ only；SYSTEM/SENSITIVE_WRITE 不进入
worker（§88）；`memory.save` 在所有 profile 的 denied 列表（§93）。
需要系统修改时 worker 只能返回 `ActionProposal`，由 parent 转
SafeAction 确认（§89-§91）。

### 4. 共享工具循环，PersonalAgent 不变厚

`personal_ai/runtime.rs::run_tool_loop` 从 `agent.rs` 抽出，两者共用
（§40）。`agent.rs` 只多一个可选 stage；测试断言其源码不含 agent/module
分支（§41/§116）。

### 5. Bounded：depth 1 + 预算 + 并发 + 取消

- **depth = 1**（§49）：executor 没有 delegate 入口，`can_delegate = false`；
- 全局 `AgentBudget`（agents/steps/tool_calls/tokens/duration），
  `child_budget` 保证 child ≤ parent 剩余，**名额预扣**（join 之前）；
- `tokio::Semaphore` 有界并发，**在 task future 内部** acquire
  （块外 await 会把整组死锁在 join 之前——实现期真实踩到）；
- `CancellationToken` 透传到每个 child；
- 失败容忍：required/optional + `PARTIAL` 明示；retry 仅 timeout/transient
  且最多一次。

### 6. 结构化 + Provenance + Untrusted

`TaskEnvelope` / `DelegationResult` 是内部协议（不用 MCP A2A，§99/§100）。
`structured_output` 解析失败 → Failed（fail-closed，§98）。worker 输出在
parent 侧标记不可信（§97）。`OrchestrationTraceView` 只含结构与计数
（§73：无 secret / token / 正文 / 完整 prompt）。

### 7. 触发是规则，不是模型自选

第一版 `decide()` 是确定性规则：显式关闭 > 显式深度 > 跨模块 > 比较/诊断
（§44）。简单请求（「珠峰多高」）不委派（§43/§105）。计划也是确定性
（2 并行 research + 可选 planner/reviewer），不让模型自选 agent 类型（§45）。

## 备选方案（拒绝）

| 方案 | 拒绝理由 |
| --- | --- |
| 每个业务域一个 Agent | 能力重复两份，必然漂移（§24） |
| worker 继承 parent 全部工具 | 违反 Least Privilege；一次注入即全权限 |
| worker 可直接执行 SYSTEM | 绕过 SafeAction 确认（§87/§90） |
| 允许 worker 再 spawn | 递归失控（§6/§49） |
| 用 MCP A2A 做内部编排 | MCP 是 capability 协议；内部用 TaskEnvelope 更简单且无网络面（§99） |
| 模型自由决定是否委派 | 第一版不可预测；规则 + 结构化决策更可控（§44） |
| 让 agent.rs 承载编排 | agent.rs 变巨型；独立 OrchestrationService（§41） |

## 影响

- 新增 `core::agents`（纯契约）与 `application::agents`
  （executor / orchestrator / profiles / prompt）。
- `PersonalHub.orchestration`、`AgentConfig.multi_agent_enabled`、
  `AgentResponse.orchestration`（trace 视图）；`personal_ai/runtime.rs`。
- `apps/mcp` 组合根升级为可选真实 store 装配（`--stores`）。
- 测试 +55（core 21 / application 33 / MCP runtime gate 4 等）；
  `cargo check --workspace --all-targets` 0 warning。
