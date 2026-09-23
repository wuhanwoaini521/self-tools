//! Rule Decision Provider（V10 §18）：逐行冻结 V9 `decide` 规则。
//!
//! **行为冻结原则**：V9 的 `DelegationDecision::{Direct, Delegate{reason}}` 判定
//! 顺序、关键字与 reason code 一个不差地搬到这里；只是把输出从「是否委派」
//! 升级成「选哪种策略 + 哪些 worker」。
//!
//! reason code 冻结表（UI / 遥测 / golden dataset 共用）：
//! - `off_by_user`                用户显式关闭多 Agent
//! - `multi_agent_disabled`       设置关闭
//! - `explicit_deep_request`      显式深度模式
//! - `cross_module_request`       跨模块
//! - `comparison_or_diagnosis`    比较 / 诊断
//! - `simple_direct`              简单请求直接回答
//! - `rule_provider_failed`       provider 自身失败（engine fallback 记录）

use devtoolbox_core::agents::{
    DecisionConfidence, DecisionProvider, DecisionProviderError, DecisionRequest, DecisionResult,
    DecisionStrategy,
};

/// 规则 provider 标识。
pub const PROVIDER_NAME: &str = "rule";

/// 标准 worker 常量（与 `profiles.rs` 的注册 id 一致）。
pub const WORKER_RESEARCH: &str = "research";
pub const WORKER_PLANNER: &str = "planner";
pub const WORKER_REVIEWER: &str = "reviewer";

/// 规则决策 provider：纯函数，无状态、无 I/O。
#[derive(Clone, Copy, Debug, Default)]
pub struct RuleDecisionProvider;

impl RuleDecisionProvider {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DecisionProvider for RuleDecisionProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    async fn decide(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResult, DecisionProviderError> {
        Ok(decide_by_rule(request))
    }
}

/// 规则本体（导出为纯函数：golden dataset / eval harness 直接调用，不经 async）。
#[must_use]
pub fn decide_by_rule(request: &DecisionRequest) -> DecisionResult {
    let lowered = request.message.to_lowercase();

    // §81：用户显式关闭（最高优先，冻结自 V9）。
    if request.multi_agent_off
        || lowered.contains("不要使用多 agent")
        || lowered.contains("别用多 agent")
        || lowered.contains("不用多智能体")
    {
        return DecisionResult::direct(PROVIDER_NAME, "off_by_user");
    }

    // 预算不允许编排（V10 新增：来自 DecisionRequest.budget_tier）。
    if !request.allows_orchestration() {
        return DecisionResult::direct(PROVIDER_NAME, "budget_exhausted");
    }

    // §82：显式深度模式 → 有界多 Agent。
    if request.explicit_deep
        || lowered.contains("深入研究")
        || lowered.contains("深入分析")
        || lowered.contains("详细分析")
        || lowered.contains("全面比较")
        || lowered.contains("系统性")
    {
        return orchestrated(
            DecisionStrategy::BoundedMultiAgent,
            vec![WORKER_RESEARCH, WORKER_PLANNER, WORKER_REVIEWER],
            3,
            true,
            "explicit_deep_request",
            DecisionConfidence::High,
        );
    }

    // 跨模块 / 多来源。
    let cross_module = [
        "日志",
        "文档",
        "记忆",
        "服务器",
        "history",
        "documents",
        "memory",
        "server",
    ]
    .iter()
    .filter(|needle| lowered.contains(**needle))
    .count()
        >= 2
        || request.cross_module_hint;
    if cross_module {
        return orchestrated(
            DecisionStrategy::ResearchOnly,
            vec![WORKER_RESEARCH, WORKER_RESEARCH],
            2,
            false,
            "cross_module_request",
            DecisionConfidence::High,
        );
    }

    // 比较 / 诊断（冻结 V9 的优先级：`比较` OR `为什么` + 长消息）。
    let comparison = lowered.contains("比较")
        || lowered.contains("对比")
        || (lowered.contains("为什么") && lowered.len() > 30);
    if comparison {
        return orchestrated(
            DecisionStrategy::ResearchOnly,
            vec![WORKER_RESEARCH, WORKER_RESEARCH],
            2,
            false,
            "comparison_or_diagnosis",
            DecisionConfidence::High,
        );
    }

    DecisionResult::direct(PROVIDER_NAME, "simple_direct")
}

/// 构造一个编排决策（worker 集随后会被 engine clamp 到可用集合）。
fn orchestrated(
    strategy: DecisionStrategy,
    workers: Vec<&'static str>,
    parallelism: usize,
    review_required: bool,
    reason_code: &'static str,
    confidence: DecisionConfidence,
) -> DecisionResult {
    DecisionResult {
        strategy,
        workers: workers.into_iter().map(str::to_string).collect(),
        parallelism,
        review_required,
        confidence,
        reason_code,
        provider: PROVIDER_NAME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(message: &str) -> DecisionRequest {
        DecisionRequest {
            message: message.to_string(),
            available_workers: vec![
                WORKER_RESEARCH.to_string(),
                WORKER_PLANNER.to_string(),
                WORKER_REVIEWER.to_string(),
            ],
            ..DecisionRequest::default()
        }
    }

    #[test]
    fn user_off_switch_wins_over_everything() {
        for message in [
            "不要使用多 agent 深入分析服务器日志和文档记忆",
            "别用多 agent",
            "不用多智能体",
        ] {
            let result = decide_by_rule(&request(message));
            assert_eq!(result.strategy, DecisionStrategy::Direct);
            assert_eq!(result.reason_code, "off_by_user");
        }
        let off = DecisionRequest {
            multi_agent_off: true,
            ..request("深入研究文档")
        };
        assert_eq!(decide_by_rule(&off).reason_code, "off_by_user");
    }

    #[test]
    fn explicit_deep_request_routes_bounded_multi_agent() {
        // V9 顺序冻结：explicit deep **优先于** cross_module / comparison，
        // 且关键词逐字冻结（`详细分析`，不是 `详细比较`）。
        for message in [
            "请深入研究这次服务器故障",
            "深入分析文档与日志关系",
            "详细分析两种方案的差异",
            "全面比较服务器与文档",
            "系统性梳理记忆",
        ] {
            let result = decide_by_rule(&request(message));
            assert_eq!(
                result.strategy,
                DecisionStrategy::BoundedMultiAgent,
                "{message}"
            );
            assert_eq!(result.reason_code, "explicit_deep_request");
            assert!(result.review_required);
        }
    }

    #[test]
    fn cross_module_keyword_count_matches_v9() {
        // 单个关键词 → 不触发跨模块。
        let single = decide_by_rule(&request("服务器状态怎么样"));
        assert_eq!(single.strategy, DecisionStrategy::Direct);
        // 两个关键词 → 触发（V9 规则冻结）。
        let two = decide_by_rule(&request("日志和文档都说"));
        assert_eq!(two.reason_code, "cross_module_request");
        assert_eq!(two.strategy, DecisionStrategy::ResearchOnly);
    }

    #[test]
    fn comparison_rules_match_v9() {
        // V9 顺序冻结：cross_module 优先于 comparison —— 含两个模块词的比较句
        // 记 cross_module_request（与 V9 逐字一致）。
        assert_eq!(
            decide_by_rule(&request("比较这两个文档")).reason_code,
            "comparison_or_diagnosis"
        );
        assert_eq!(
            decide_by_rule(&request("对比服务器与文档")).reason_code,
            "cross_module_request"
        );
        // 为什么 + len > 30 才触发（冻结 V9 的 `lowered.len()` 字节数判断）。
        assert_eq!(
            decide_by_rule(&request("为什么")).strategy,
            DecisionStrategy::Direct
        );
        // 该串字节长度 > 30 但不含第二模块词 → comparison_or_diagnosis。
        let long_why = "为什么最近系统总是在夜间出现内存占用异常升高的现象";
        assert!(long_why.to_lowercase().len() > 30);
        assert_eq!(
            decide_by_rule(&request(long_why)).reason_code,
            "comparison_or_diagnosis"
        );
    }

    #[test]
    fn budget_tier_none_forces_direct() {
        let request = DecisionRequest {
            budget_tier: devtoolbox_core::agents::BudgetTier::None,
            ..request("深入研究文档和日志")
        };
        let result = decide_by_rule(&request);
        assert_eq!(result.strategy, DecisionStrategy::Direct);
        assert_eq!(result.reason_code, "budget_exhausted");
    }

    #[test]
    fn simple_requests_stay_direct() {
        for message in ["你好", "现在几点", "毛泽东是谁"] {
            let result = decide_by_rule(&request(message));
            assert_eq!(result.strategy, DecisionStrategy::Direct);
            assert_eq!(result.reason_code, "simple_direct");
        }
    }

    #[test]
    fn provider_name_is_stable() {
        assert_eq!(RuleDecisionProvider::new().name(), "rule");
    }
}
