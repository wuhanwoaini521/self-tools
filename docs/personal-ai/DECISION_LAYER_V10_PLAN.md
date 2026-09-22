# V10 · Decision Intelligence Layer — Plan

Gate 0 审计结论与实施计划（对应最终 Goal §14–§41）。

## 1. 现状审计（V9 真实代码）

| 事实 | 位置 |
| --- | --- |
| 决策点是 `OrchestrationService::decide(&str, bool) -> DelegationDecision`（纯规则：关闭词 / `multi_agent_enabled` / 深度词 / 跨模块词计数 / 比较词） | `crates/application/src/agents/orchestrator.rs:161-193` |
| `DelegationDecision` 只有两种形态：`Direct` / `Delegate { reason }` | 同上 |
| 计划生成 `plan(&self, request_id, objective, deep: bool)`：2 个 research 并行 + 可选 planner；`final_review_required = deep` | `orchestrator.rs:195-243` |
| 追踪结构 `OrchestrationTrace { trace_id, decision, plan_rationale, runs, review, merged, stopped_early }` → 视图 `OrchestrationTraceView` | `orchestrator.rs:104-131`；`crates/core/src/personal_ai/types.rs:77-116` |
| agent 侧在 `decision.is_delegating()` 时用固定 `plan(&request_id, msg, true)` 执行编排 | `crates/application/src/personal_ai/agent.rs:135-172` |
| 设置 `AppSettings`（`crates/core/src/settings.rs`）无 decision/multi-agent 段；前端 `AiSettings` 只有 provider/model/base_url/api_key/timeout_secs |
| Provider 契约 `ChatModelProvider { name(), chat(ChatRequest) }`；`ChatRequest { messages, tools, temperature, max_tokens }` | `crates/core/src/personal_ai/provider.rs` |
| worker 能力由 cap intersect + `envelope.validate()` 强制（V9 已冻结） | `orchestrator.rs` execute 路径 |

**边界确认（§12）**：Decision Layer 只产出 *orchestration strategy*（direct / workers / parallelism / review）。
Authorization、tool permission、SafeAction、confirmation、SYSTEM capability、Memory write 全部继续由
确定性代码（capability intersect、`envelope.validate()`、child_budget、SafeActionService）控制。

## 2. 目标契约（core 新增 `agents::decision`）

```
DecisionStrategy   DIRECT | RESEARCH_ONLY | PLAN_AND_RESEARCH | RESEARCH_AND_REVIEW | BOUNDED_MULTI_AGENT
DecisionMode       RULE | JEV_SHADOW | JEV_ACTIVE
DecisionConfidence HIGH | UNCERTAIN | LOW        (conflict_counter 计数驱动，不散落 if > 0.5)
WorkerSet          explicit | CAPABILITY_BOUNDED
DecisionRequest    request_id, message(截断), module, page, entity_kind, cross_module_hint,
                   external_evidence_needed, explicit_deep, available_workers,
                   available_tool_groups, budget_summary, capability_constraints
DecisionResult     strategy, workers, parallelism, review_required, confidence, reason_code, provider, latency_ms
DecisionPlan       DecisionResult + ExecutionPlan（由 DecisionEngine 组合，provider 不产 plan）
DecisionProvider   async fn decide(&self, ctx: &DecisionRequest) -> Result<DecisionResult, ProviderError>
DecisionEngine     provider 链 + shadow provider + fallback；只返回 DecisionResult
```

**数据最小化（§16）**：`DecisionRequest` 不携带 Memory/Documents/聊天正文/文件内容；
`message` 截断到 256 字符且只含分类所需信息。

**策略映射到 V9 既有 plan 形态（行为冻结）**：

| Strategy | plan 形态 |
| --- | --- |
| `DIRECT` | 空 plan（agent 单轮直接回答） |
| `RESEARCH_ONLY` | 2 个并行 research |
| `PLAN_AND_RESEARCH` | 2 research + planner（可选） |
| `RESEARCH_AND_REVIEW` | 2 research + reviewer |
| `BOUNDED_MULTI_AGENT` | 2 research + planner + reviewer |

## 3. Provider 实现

- `RuleDecisionProvider`：逐行搬迁 V9 `decide` 规则，产出 `DecisionResult`；`reason_code` 沿用
  V9 常量（`off_by_user` / `multi_agent_disabled` / `explicit_deep_request` /
  `cross_module_request` / `comparison_or_diagnosis` / `simple_direct`）。
- `JevDecisionProvider`（infrastructure adapter）：TypeSafe Jev HTTP API，只做窄决策
  （Choice：strategy / review_required）。**不**执行工具、不改权限、不写 Memory。
  core/application 只依赖 `DecisionProvider`。
- `FakeJevTransport` / script provider：无 key 时也能测 request serialization、response validation、
  timeout、invalid response、rate-limited 响应。

## 4. Shadow / Fallback（§26–§30）

`DecisionEngine`：`mode = RULE` 直接用 rule；`JEV_SHADOW` 用 rule 执行 + 并行调 Jev 记录差异
（不影响执行）；`JEV_ACTIVE` 用 Jev，任何错误（timeout / invalid / low confidence / rate limit /
unavailable）**自动 fallback 到 rule** 并记录 `fallback = true`。

本仓库无 Jev key + 无外网确认 → `REAL_JEV = BLOCKED_EXTERNAL`；默认模式 `RULE`，
shadow 用 FakeJev 全部测通。

## 5. 遥测（§20–§21）

扩展 `OrchestrationTrace` / `OrchestrationTraceView`：

```
decision_provider: Option<String>
decision_confidence: Option<String>   // high/uncertain/low
decision_strategy: Option<String>
decision_reason_code: Option<String>
decision_latency_ms: Option<u64>
decision_fallback: bool
shadow_decision: Option<String>       // JEV_SHADOW 下 Jev 的判断（仅 label，无 CoT）
worker_count: usize
```

指标聚合在新 `DecisionMetrics`（进程内 + 可选导出接口）：direct rate / orchestration rate /
worker count / decision latency / fallback rate。

## 6. Golden dataset + Eval harness（§19/§31–§33）

`golden_decision_cases`（application 内静态表，≥12 类：simple factual、single module、
cross module、research、deep research、server read、safe action、travel planning、
document comparison、knowledge synthesis、history context、language context）。
`DecisionEvalHarness` 对 Rule / Jev(fake) 运行，输出：
routing agreement、unnecessary orchestration、missed orchestration、worker selection、
review decision、latency。标签来源分三类并显式标注：`reviewed`（人工审）/ `heuristic` /
`baseline`（= rule 决策，仅作对照，不当作 ground truth）。

## 7. 测试清单

- `DecisionRequest` 截断 / 最小化（不携带私人内容）。
- Rule provider 对 V9 每个 reason code 行为冻结（旧规则 → 新策略 1:1）。
- Engine fallback：provider error / timeout / low confidence / invalid response 全部回 rule 且成功。
- Shadow：执行取 rule，shadow 记录存在，且失败不影响。
- Confidence policy 边界（阈值表集中在一处）。
- Jev adapter：serialization 形状、validation 拒非法响应、timeout 映射。
- Capability 未退化：decision 只能选 `available_workers` 内的 worker；无法选未注册 agent。
- OrchestrationService 经 `DecisionEngine.decide()` 路由的集成测试；V9 全部既有测试不回归。

## 8. DoD 映射

见 `V10_FINAL_REPORT.md`（Gate 9 后生成）。
