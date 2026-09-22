# V9 · Multi-Agent Orchestration — 实施状态

> 实时状态（Gates -1–11）。基线：V8 冻结 HEAD `955d5ce`
> （工作树干净，687 passed / 0 failed，`--all-targets` 0 warning）。

## 测试总览（真实执行）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace` | **749 passed / 0 failed**（V8 基线 687，+62） |
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
| 9 | Security Review | ✅ | 独立 reviewer A-G；**11 项发现全部修复**（2 高 3 中 6 低/信息） |
| 10 | Full Regression | ✅ | 745 全绿；V5/V6/V7/V8 无退化 |
| 11 | Docs | ✅ | `MULTI_AGENT_V9.md` / `ADR-008` / 本文件 / `V9_FINAL_REPORT.md` |

## Gate 9 安全审查修复（独立 reviewer，11 项）

| # | 严重度 | 发现 | 修复 |
| --- | --- | --- | --- |
| V9-B1 | **高** | `run_tool_loop` 执行模型点名的**任意**已注册工具 → descriptor/capability 交集只过滤了 discovery，执行侧无强制 | 每个 call 先过授权列表，未命中 → `tool_not_authorized`（记 trace）；parent 与 worker 共用同一强制点 |
| V9-D1 | **高** | budget 的 tokens / tool_calls / duration 三维只算不强制执行（无超时、provider max_tokens 恒 None、deadline 恒 0） | executor 用 `tokio::time::timeout`（→ `TimedOut`）；token 上限传入 provider；工具调用硬上限 |
| V9-E1 | 中 | worker 输出以「任务指令」身份进 reviewer / parent prompt | 显式不可信围栏 + 结构投影 + 截断；reviewer drafts 与 parent 注入都走它 |
| V9-D2 | 中 | reviewer run 绕过 child_budget / 名额预扣 / 预算检查 | 与 worker 同路径 |
| V9-G1 | 中 | MCP `--stores` 可指向桌面配置目录 → 多进程共开 SQLite（无 busy_timeout/WAL）立即 `SQLITE_BUSY` | 拒绝已含业务库的目录（`allow_existing` 显式豁免仅测试/迁移） |
| V9-D3 | 低 | AI 票据 `session_id="ai"` vs 桌面确认 `"desktop"` → 票据永无法确认（UI 死卡） | 统一 `"ai-desktop"`；MCP client_id 仍每请求唯一 |
| V9-A2 | 低 | `TaskEnvelope::validate` 生产从未调用（且自比较恒真） | 编排器对每个 envelope validate，失败即任务失败 |
| V9-C1 | 低 | `worker_context_messages` 死代码（agent.rs 另有注入） | 删除 |
| V9-A1 | 信息 | `required_tools` 空 = 全集；research 默认含 restart/open 入口 | profile 默认只读模块白名单 + 显式 deny `memory.*`/`*.open`/`services.restart` |
| V9-D4 | 信息 | `child_budget` 的 `.max(now_ms.min(1))` 把 0 抬到 1 | 删除后缀，恢复「父耗尽 → 子 0」 |
| V9-F1 | 信息 | trace_id 直接用前端可控 session_id | `is_valid_task_id` 校验，不合法回落 `req-{uid16}` |

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
