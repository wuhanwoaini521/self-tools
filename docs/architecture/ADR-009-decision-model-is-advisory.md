# ADR-009 · Decision Model Is Advisory; Security Remains Deterministic

- 状态：Accepted（V10）
- 日期：2026-09-22

## 背景

V9 的 `OrchestrationService::decide` 是孤立 rule-based 决策点。V10 引入
`DecisionEngine` + 可插拔 provider（Rule / Jev），用于决定 orchestration strategy
（direct / workers / parallelism / review）。

风险：模型化决策一旦被允许影响权限，就会打开 capability escalation、
SafeAction bypass、隐藏数据外泄等攻击面。

## 决策

**Decision Model Is Advisory; Security Remains Deterministic.**

1. 决策层只回答「用哪种编排策略」，输出 `DecisionResult`（纯数据）。
2. 决策层**不能**决定：authorization、tool permission、MCP scope、文件安全、
   secret 检测、SafeAction、confirmation、SYSTEM 权限、Memory 写入权限。
3. 上述每一项继续由确定性代码强制：
   - 工具能力：`ToolRegistry` risk gate + `DelegatedCapabilitySet::intersect`
     + `is_subset_of`
   - worker 权限：静态 `AgentDescriptor`（READ-only、`can_delegate=false`、denied_tools）
   - 执行边界：`TaskEnvelope::validate` + `child_budget` + 有界并发
   - SYSTEM 写入：`SafeActionService` 票据 + confirmation + audit（V7/V8）
3. `DecisionResult::clamp` 是最后一道关：worker 集必须 ⊆ `available_workers`，
   否则强制 Direct。
4. Confidence 策略集中在一处（`DecisionConfidence::from_probability`）；
   低置信 = 自动回落 Rule。
5. provider 失败（timeout / invalid / rate-limited / unavailable）一律回落 Rule，
   且 fallback 不可关闭。
6. `DecisionRequest` 最小化：无 Memory/Documents/文件内容，message 截断 256 字符。
7. 遥测只暴露结构（provider/strategy/confidence/reason_code/latency/workers），
   禁止 hidden chain-of-thought 与 secret。

## 备选方案

- **让模型直接选工具/权限**：否决——决策与执行混淆，无法审计，违背 V9 的
  `Agent != Unlimited Capability` 铁律。
- **决策失败即报错**：否决——家庭服务器场景可用性优先，必须 fail-safe 到 Rule。
- **Shadow 模式可选**：否决——`JEV_SHADOW` 是 `JEV_ACTIVE` 的前置条件（Goal §26）。

## 后果

- 正面：provider 可替换（Rule ⇄ Jev ⇄ 未来 LLM reference），PersonalAgent 保持单入口；
  安全边界可逐条测试证明（`decision_security_tests.rs`）。
- 负面：决策质量受 Rule 基线限制；Jev 未配置时只能 Shadow/Rule
  （当前 `REAL_JEV = BLOCKED_EXTERNAL`）。
- 中性：`OrchestrationService` 仍是唯一执行所有者；`plan_for_strategy` 只依赖
  策略枚举，不依赖 provider。
