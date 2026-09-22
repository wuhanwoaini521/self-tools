# V9 · Multi-Agent Orchestration — 实施状态

> 实时状态（Gates -1–11）。基线：V8 冻结 HEAD `955d5ce`
> （工作树干净，687 passed / 0 failed，`--all-targets` 0 warning）。

## 测试总览（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **745 passed / 0 failed**（V8 基线 687，+58） |
| `cargo check --workspace --all-targets` | 0 error / 0 warning |
| `npx tsc --noEmit`（ui） | 0 error |
| `npm run build`（ui） | built |

测试分布：core 362 / application 178 / infrastructure 144 / desktop 13 /
apps/mcp 41（lib 29 + bin 0 + integration 8 + runtime_stores 4）/ server 7。

## Gate 状态

| Gate | 内容 | 状态 | 证据 |
| --- | --- | --- | --- |
| -1 | Freeze V8 | ✅ | V8 已 6 commits（含 4 份文档）；工作树干净；687 基线绿 |
| -0.5 | Remote Identity + MCP stores | ✅（部分，按 §12 允许） | `compose::build(BuildOptions{stores_dir})` 装配 desktop 相同 repository；`DenyAllIdentityProvider` 保持不变（无 OIDC provider 可连，远程写关闭）；4 个 runtime-gate 测试 |
| 0 | Audit + Plan | ✅ | `MULTI_AGENT_V9_PLAN.md`；全仓 grep 确认零编排资产 |
| 1 | Contracts + Registry | ✅ | `core::agents`（descriptor/task/budget/result）；21 单测 |
| 2 | Task runtime | ✅ | Parent 创建结构化 child task（`TaskEnvelope::validate`） |
| 3 | AgentExecutor + shared loop | ✅ | `personal_ai/runtime.rs` 抽出共享循环；executor 复用；`agent.rs` 变薄 |
| 4 | Orchestrator | ✅ | decide/plan/execute/merge；`ExecutionPlan` DAG 基础模型 |
| 5 | Workers | ✅ | research / planner / reviewer（+ synthesizer P1）；全 READ-only |
| 6 | Budget + Concurrency + Cancellation | ✅ | 5 个专项测试（有界并发、取消、预算停止、max_agents、无递归） |
| 7 | SafeAction / Security | ✅ | 端到端 + 显式关闭 + `agent.rs` 无业务分支断言 |
| 8 | Trace + Frontend | ✅ | `OrchestrationTraceView`（无 secret/正文）+ AI Panel 折叠「执行过程」 |
| 9 | Security Review | 🔄 | 独立 reviewer 运行中 |
| 10 | Full Regression | ✅ | 745 全绿；V5/V6/V7/V8 无退化 |
| 11 | Docs | 🔄 | `MULTI_AGENT_V9.md` / `ADR-008` / 本文件 / final report 待审查结论 |

## 实现期修复的真实缺陷

1. **Semaphore 死锁**：`acquire_owned()` 在 `handles.push(async move)` **之前**
   await —— 组内第二个任务在同一轮 `join_all` 开始前阻塞 → 整组挂起
   （`max_agents_limit_is_enforced` 测试暴露）。修复：acquire 移进 future 内部。
2. **max_agents 上限失效**：`used.agents` 在 `join_all` **之后**才累加，同组后续
   任务看不到已启动名额 → 全部放行。修复：**预扣**（join 前 `used.agents += 1`），
   join 后只累加其余维度。
3. **`AgentResponse` 缺 orchestration 字段**导致 core 编译失败（替换未命中）→ 精确补字段。
4. **`AgentConfig` / `PersonalHub` 新字段**破坏 4 个既有模块测试的字面量 → 统一补
   `orchestration: None`。

## 未做（明确范围外 / P1-P2）

- 生产 OIDC provider（§12：开发环境无可连接 IdP，保持 DenyAll + Fake，远程写关闭）。
- Synthesizer worker、成本估算、外部 agent worker（§159 P2）。
- 模型结构化委派决策（当前是确定性规则，§44 第一版）。
- Automation / Tasks / Resources / MCP A2A。

## 回滚

`core::agents` + `application::agents` + trace 字段可独立删除；
`PersonalHub.orchestration` 不装配即回到 V8 形态；`apps/mcp --stores`
不传即 fail-closed 空能力集。
