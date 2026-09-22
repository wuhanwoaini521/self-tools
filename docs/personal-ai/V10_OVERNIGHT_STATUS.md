# V10 Overnight Status

## 已完成 Gates

| Gate | 内容 | 状态 |
| --- | --- | --- |
| -1 | Freeze V9（749 tests / check 0 warning / tsc+build / history pipeline） | PASS |
| 0 | Audit + `DECISION_LAYER_V10_PLAN.md` | PASS |
| 1 | Decision contracts（engine/provider/context/result/confidence） | PASS |
| 2 | Rule Baseline（V9 规则逐字冻结） | PASS |
| 3 | Golden Decision Dataset（15 cases，12+ 类别） | PASS |
| 4 | Decision Telemetry（trace + view + UI） | PASS |
| 5 | Metrics（direct/orchestration/worker/latency/fallback/failure） | PASS |
| 6 | Jev Provider（HTTP adapter，官方 API 形状核对） | PASS |
| 7 | Fake Jev（脚本化 transport） | PASS |
| 8 | Shadow Mode + Fallback + Settings | PASS |
| 9 | Security review findings fix + regression | PASS |

## 实施位置

| 层 | 文件 | 内容 |
| --- | --- | --- |
| core | `crates/core/src/agents/decision.rs` | 契约 + `DecisionEngine` + 16 tests |
| application | `crates/application/src/agents/decision_rule.rs` | RuleDecisionProvider（V9 冻结）+ 7 tests |
| application | `crates/application/src/agents/decision_engine.rs` | `AgentDecisionEngine` 装配 + 3 tests |
| application | `crates/application/src/agents/decision_eval.rs` | golden dataset + eval harness + 5 tests |
| application | `crates/application/src/agents/decision_security_tests.rs` | 7 security integration tests |
| application | `crates/application/src/agents/orchestrator.rs` | `decide_v10` / `plan_for_strategy` / trace 扩展 |
| application | `crates/application/src/personal_ai/agent.rs` | 编排路径改走 DecisionEngine |
| infrastructure | `crates/infrastructure/src/agents/jev_decision.rs` | Jev adapter + Fake + 10 tests |
| core | `crates/core/src/settings.rs` | `DecisionSettings` + 4 tests |
| desktop | `apps/desktop/src/personal_ai.rs` | hub 装配 decision engine + orchestration |
| frontend | `apps/desktop/ui/src/features/ai/{aiTypes,AIPanel}.tsx` | 决策遥测展示 |

## 测试计数（真实运行）

| 范围 | 结果 |
| --- | --- |
| V9 基线（冻结时） | 749 passed / 0 failed |
| core（agents::decision） | 16 passed |
| application（agents::*，含 rule/eval/engine/security） | 63 passed |
| infrastructure（agents::jev_decision） | 10 passed |
| core（settings decision） | 4 passed |

## 外部阻塞

- `REAL_JEV = BLOCKED_EXTERNAL`：无 TypeSafe Jev API key。
  已实现：abstraction + Fake + serialization + validation + timeout + error handling
  + shadow mode + settings + fallback + eval harness + tests。
  真实 active 不可用；默认模式 `RULE`。

## 已知限制（诚实记录）

1. Jev active 未做真实端到端（无 key）；Fake 覆盖了请求/响应/错误全形状。
2. Eval harness 的 golden labels 中 4 个为 `Heuristic`（关键词明确但未逐条人工复核），
   已在 `LabelSource` 里显式标注，不与 reviewed 混淆。
3. 决策遥测目前挂到 `OrchestrationTrace`，尚未接入应用级 metrics 聚合端点
   （V11 health/metrics gate 统一做）。
