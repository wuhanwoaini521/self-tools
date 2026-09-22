# Decision Intelligence Layer（V10）

> 对应 Goal §11–§42。V9 的孤立 rule 决策点升级为可替换的 `DecisionEngine`，
> 但**安全边界不变**：决策层只选 orchestration strategy，一切权限仍由确定性代码控制。

## 1. 架构

```text
                     PersonalAgent（唯一用户入口）
                           │
                           ▼
                 OrchestrationService（唯一执行方）
                           │
                           ▼
                     DecisionEngine
                      │           │
             RuleDecisionProvider  JevDecisionProvider（adapter）
             （V9 规则冻结）       （HTTP；只做窄决策）
                      │           │
                      └─────┬─────┘
                            ▼
                      DecisionResult
              { strategy, workers, parallelism,
                review_required, confidence, reason_code }
                            │
              plan_for_strategy() → ExecutionPlan
                            │
             delegate / parallel / review / merge（V9 不变）
```

## 2. 契约（`crates/core/src/agents/decision.rs`）

| 类型 | 作用 |
| --- | --- |
| `DecisionStrategy` | `direct` / `research_only` / `plan_and_research` / `research_and_review` / `bounded_multi_agent` |
| `DecisionMode` | `rule` / `jev_shadow` / `jev_active`（未配置 Jev key → 强制 `rule`） |
| `DecisionConfidence` | `high` / `uncertain` / `low`；阈值集中在 `from_probability`（≥0.75 / ≥0.45 / else） |
| `DecisionRequest` | **最小化路由信号**：module/page/entity_kind/4 个布尔信号/available_workers/available_tool_groups/budget_tier/capability_constraints；message 截断 256 字符；无 Memory/Documents/文件内容 |
| `DecisionResult` | strategy + workers + parallelism + review_required + confidence + reason_code + provider |
| `DecisionProvider` | 端口：`name()` / `is_available()` / `decide()` |
| `DecisionEngine` | mode + shadow + fallback + clamp；只返回 `DecisionResult` |
| `DecisionTelemetry` | provider/strategy/confidence/reason_code/latency/fallback/shadow/workers（无正文、无 CoT、无 secret） |

**`DecisionResult::clamp`（硬边界）**：worker 集裁剪到 `available_workers`；
全被裁掉 → 强制 `Direct`。providers 永远无法虚构编排。

## 3. Rule Baseline（行为冻结）

`RuleDecisionProvider`（`crates/application/src/agents/decision_rule.rs`）逐字搬迁 V9
`OrchestrationService::decide` 规则，顺序与关键帧全部一致：

| 顺序 | 条件 | reason_code | 策略 |
| --- | --- | --- | --- |
| 1 | `multi_agent_off` / "不要使用多 agent" / "别用多 agent" / "不用多智能体" | `off_by_user` | direct |
| 2 | 预算档 `None` / 无可用 worker | `budget_exhausted` | direct |
| 3 | 显式深度词（深入研究/深入分析/详细分析/全面比较/系统性） | `explicit_deep_request` | bounded_multi_agent |
| 4 | 跨模块词命中 ≥2（日志/文档/记忆/服务器/history/documents/memory/server）或 `cross_module_hint` | `cross_module_request` | research_only |
| 5 | 比较/对比 或（为什么且 `len()>30`） | `comparison_or_diagnosis` | research_only |
| 6 | 其他 | `simple_direct` | direct |

## 4. Jev Provider（隔离在 infrastructure）

- 文件：`crates/infrastructure/src/agents/jev_decision.rs`。
- API（2026-09 核对官方文档 https://docs.typesafe.ai/api）：
  - `POST {base}/v1/systemone`，`Authorization: Bearer <key>`
  - body `{ state, model: "jev-latest", questions: { strategy: {type:"choice",…}, review_required: {type:"noul",…} } }`
  - 响应 `{ answers: { strategy: {choice, probabilities, confidence}, review_required: {noul} }, usage: {input_tokens, output_tokens} }`
  - 错误码映射：401/403→`unavailable`，429→`unavailable`（rate limited），529/5xx→`unavailable`，422/解析失败→`invalid_response`
- 只问两个窄问题（§24）：选策略 + 是否 review。
- **不做**：执行工具、改权限、选 SYSTEM 权限、写 Memory、授权文件、批准 SafeAction。
- core/application **零** Jev 知识；只有 `DecisionProvider` trait。
- `FakeJevTransport`：脚本化 choice/noul/错误响应，无 key 也能测全链路。

## 5. Shadow / Fallback

| 模式 | 实际路由 | 影子 | 失败行为 |
| --- | --- | --- | --- |
| `RULE` | Rule | 无 | Rule 自身失败 → `direct`（`rule_provider_failed`） |
| `JEV_SHADOW` | **Rule** | 并行调 Jev，记录 `DecisionShadowRecord{rule_strategy, shadow_strategy, agreed, shadow_confidence, shadow_latency_ms}` | 影子失败只丢记录，执行不变 |
| `JEV_ACTIVE` | Jev | 无 | timeout / 任何 `Err` / `low confidence` / clamp 后空 worker → **回落 Rule** 且 `telemetry.fallback = true` |

`jev_failure_fallback` 恒 true（不可关闭）：fallback 是安全属性，不是可选项。

## 6. Golden Dataset + Eval Harness

- `golden_decision_cases()`：15 个 case，覆盖 §19 要求的 12 类（simple factual /
  single module / cross module / research / deep research / server read / safe action /
  travel planning / document comparison / knowledge synthesis / history context /
  language context）+ off switch + budget + baseline。
- 标签来源三分类且显式标注（§33）：`Reviewed`（人工审）/ `Heuristic`（规则推断）/
  `Baseline`（= rule 决策，**只度量 agreement，不当 ground truth**）。
- `DecisionEvalHarness::run(provider)` 产出 `DecisionEvalReport`：
  matched/unmatched、`unnecessary_orchestration`、`missed_orchestration`、
  reason mismatches、avg/max latency、per-case outcomes。

## 7. 遥测（§20/§38）

`OrchestrationTrace.decision_telemetry: Option<DecisionTelemetry>` →
`OrchestrationTraceView` 新增 `decision_provider` / `decision_strategy` /
`decision_confidence` / `decision_reason_code` / `decision_latency_ms` /
`decision_fallback` / `shadow_decision` / `worker_count`。

Trace UI 展示（`AIPanel.tsx`）：策略中文 label + provider label + 置信档 +
延迟 + fallback 标记 + shadow label。**禁止**展示隐藏 chain-of-thought。

## 8. 边界（§12/§39：安全不委托）

Decision Layer 只能决定 orchestration strategy。以下全部继续由确定性代码控制，
并有测试逐条证明（`decision_security_tests.rs`）：

| 属性 | 强制点 | 测试 |
| --- | --- | --- |
| 不能绕过 auth | 决策层无 auth 概念；worker capability 由 intersect 决定 | `decision_cannot_grant_workers_denied_tools` |
| 不能绕过 tool capability | `DelegatedCapabilitySet::intersect` + `is_subset_of` | `real_workers_still_never_get_write_tools` |
| 不能绕过 SafeAction | SYSTEM 语义只在 SafeActionService 票据 | `decision_cannot_elevate_worker_risk` |
| 不能提升 worker | profiles 静态、READ-only、`can_delegate=false` | `decision_cannot_elevate_worker_risk` |
| 低置信安全回落 | `DecisionConfidence::forces_fallback` | `low_confidence_never_reaches_execution` |
| provider 超时/失败 | engine `tokio::time::timeout` + Err → rule | core engine tests（4 个 fallback 测试） |
| 私有数据最小化 | `DecisionRequest` 无正文字段；截断 256 | `decision_request_never_carries_private_content` |
| depth=1 / max workers / budget | V9 不变（execute 路径） | V9 gate6/gate9 测试全保留 |
| 不能选未注册 agent | registry 静态 + clamp | `decision_does_not_select_unregistered_agents` |

## 9. 设置（§37）

`DecisionSettings`（`crates/core/src/settings.rs`）：

```json
{ "mode": "rule | jev_shadow | jev_active",
  "jev_api_key": "<secret；skip_serializing_if empty>",
  "jev_base_url": "https://api.typesafe.ai",
  "jev_model": "jev-latest",
  "jev_timeout_secs": 5,
  "jev_failure_fallback": true,
  "multi_agent_enabled": true,
  "max_workers": 4 }
```

`effective_mode()`：未配置 key → 强制 `rule`。`jev_configured()` 只暴露布尔；
key 只进 gitignored `config/settings.json`，不进日志/前端/git。

## 10. 当前激活状态

| 项 | 状态 |
| --- | --- |
| DecisionEngine | PASS |
| Rule Provider（V9 冻结） | PASS |
| Decision Telemetry | PASS |
| Golden Cases（15） | PASS |
| Eval Harness | PASS |
| Jev Provider + FakeJev | PASS |
| Shadow Mode | PASS |
| Fallback（error/timeout/low-confidence/invalid） | PASS |
| No Security Delegation | PASS |
| **Real Jev** | **BLOCKED_EXTERNAL**（无 API key；`JEV_ACTIVE` 不可用） |
| V9 Regression | PASS |
| Docs / ADR | PASS |
