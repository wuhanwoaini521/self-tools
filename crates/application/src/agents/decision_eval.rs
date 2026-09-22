//! Golden Decision Dataset + Decision Eval Harness（V10 §19/§31-§33）。
//!
//! 三个铁律：
//! 1. **不伪造 ground truth（§33）**：每个 case 显式标注标签来源
//!    （`Reviewed` = 人工审 / `Heuristic` = 规则推断 / `Baseline` = 规则决策对照）。
//!    `Baseline` 标签**不**当作正确性标准，只用于测量 agreement。
//! 2. **agreement ≠ correctness**：routing agreement 只衡量 rule 与 jev 的一致度。
//! 3. **可判定**：case 带期望策略（ reviewed label 时）与期望 reason code。

use std::time::Instant;

use devtoolbox_core::agents::{
    BudgetTier, DecisionProvider, DecisionRequest, DecisionResult, DecisionStrategy,
};

/// 标签来源（§33）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelSource {
    /// 人工审过的期望值。
    Reviewed,
    /// 启发式推断（关键词明确、规则稳定）。
    Heuristic,
    /// 仅作对照：等于 rule 基线决策，不是正确性标准。
    Baseline,
}

impl LabelSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LabelSource::Reviewed => "reviewed",
            LabelSource::Heuristic => "heuristic",
            LabelSource::Baseline => "baseline",
        }
    }
}

/// 单个 golden case。
#[derive(Clone, Debug)]
pub struct GoldenCase {
    pub id: &'static str,
    /// 请求类别（UI/报告用）。
    pub category: &'static str,
    pub message: &'static str,
    pub module: Option<&'static str>,
    pub entity_kind: Option<&'static str>,
    pub budget_tier: BudgetTier,
    /// 期望策略（None = 不约束，只度量）。
    pub expected_strategy: Option<DecisionStrategy>,
    /// 期望 reason code（None = 不约束）。
    pub expected_reason_code: Option<&'static str>,
    pub label: LabelSource,
}

impl GoldenCase {
    fn to_request(&self, available_workers: &[String]) -> DecisionRequest {
        DecisionRequest {
            request_id: self.id.into(),
            message: self.message.chars().take(256).collect(),
            module: self.module.map(str::to_string),
            page: None,
            entity_kind: self.entity_kind.map(str::to_string),
            cross_module_hint: false,
            external_evidence_needed: false,
            explicit_deep: false,
            multi_agent_off: false,
            available_workers: available_workers.to_vec(),
            available_tool_groups: Vec::new(),
            budget_tier: self.budget_tier,
            capability_constraints: vec![
                "read_only".into(),
                "no_memory_write".into(),
                "no_system".into(),
            ],
        }
    }
}

fn default_workers() -> Vec<String> {
    vec!["research".into(), "planner".into(), "reviewer".into()]
}

/// Golden 数据集（§19：≥ 12 类，覆盖产品真实请求形态）。
#[must_use]
pub fn golden_decision_cases() -> Vec<GoldenCase> {
    use BudgetTier::{Full, None as NoBudget};
    use DecisionStrategy::*;
    use LabelSource::*;
    vec![
        // 1. simple factual
        GoldenCase {
            id: "simple-factual",
            category: "simple_factual",
            message: "中华人民共和国是哪一年成立的",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: None,
            label: Reviewed,
        },
        // 2. single module
        GoldenCase {
            id: "single-module-history",
            category: "single_module",
            message: "查一下遵义会议的参与人员",
            module: Some("history"),
            entity_kind: Some("event"),
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: None,
            label: Reviewed,
        },
        // 3. cross module
        GoldenCase {
            id: "cross-module",
            category: "cross_module",
            message: "结合文档和服务器日志看看最近的问题",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Reviewed,
        },
        // 4. research
        GoldenCase {
            id: "research-comparison",
            category: "research",
            message: "比较这两份文档的观点差异",
            module: Some("documents"),
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Reviewed,
        },
        // 5. deep research
        GoldenCase {
            id: "deep-research",
            category: "deep_research",
            message: "请深入研究这次服务器故障与文档记录的关系",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(BoundedMultiAgent),
            expected_reason_code: None,
            label: Reviewed,
        },
        // 6. server read
        GoldenCase {
            id: "server-read",
            category: "server_read",
            message: "服务器现在状态怎么样，CPU 高吗",
            module: Some("server"),
            entity_kind: Some("service"),
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 7. safe action request（仍只读路由；执行边界不在决策层）
        GoldenCase {
            id: "safe-action-request",
            category: "safe_action",
            message: "帮我看下服务状态然后准备重启 media 服务",
            module: Some("server"),
            entity_kind: Some("service"),
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 8. travel planning
        GoldenCase {
            id: "travel-planning",
            category: "travel_planning",
            message: "帮我规划东京五天的行程，比较两种路线方案",
            module: Some("travel"),
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 9. document comparison（与 4 同类不同措辞，检查稳定性）
        GoldenCase {
            id: "document-comparison-why",
            category: "document_comparison",
            message: "为什么这两份合同对违约责任的描述不一致，请详细说明原因",
            module: Some("documents"),
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 10. knowledge synthesis
        GoldenCase {
            id: "knowledge-synthesis",
            category: "knowledge_synthesis",
            message: "把我记忆里的偏好和文档里的要求汇总一下",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 11. history context
        GoldenCase {
            id: "history-context",
            category: "history_context",
            message: "这个人物在历史上的地位如何",
            module: Some("history"),
            entity_kind: Some("person"),
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 12. language context（"为什么"+长句 → 冻结规则路由到 ResearchOnly）
        GoldenCase {
            id: "language-context",
            category: "language_context",
            message: "这句话为什么要这样变形，请解释语法原因",
            module: Some("language"),
            entity_kind: Some("sentence"),
            budget_tier: Full,
            expected_strategy: Some(ResearchOnly),
            expected_reason_code: None,
            label: Heuristic,
        },
        // 13. user off switch（最高优先级）
        GoldenCase {
            id: "user-off-switch",
            category: "off_switch",
            message: "不要使用多 agent，直接比较文档和日志",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: Some(Direct),
            expected_reason_code: Some("off_by_user"),
            label: Reviewed,
        },
        // 14. budget exhausted
        GoldenCase {
            id: "budget-exhausted",
            category: "budget",
            message: "请深入研究文档与服务器日志的关系",
            module: None,
            entity_kind: None,
            budget_tier: NoBudget,
            expected_strategy: Some(Direct),
            expected_reason_code: Some("budget_exhausted"),
            label: Reviewed,
        },
        // 15. baseline-only case（度量 agreement，不作为正确性标准）
        GoldenCase {
            id: "baseline-greeting",
            category: "baseline",
            message: "你好",
            module: None,
            entity_kind: None,
            budget_tier: Full,
            expected_strategy: None,
            expected_reason_code: None,
            label: Baseline,
        },
    ]
}

/// 单个 case 的评测结果。
#[derive(Clone, Debug)]
pub struct CaseOutcome {
    pub id: String,
    pub category: String,
    pub label: LabelSource,
    pub expected: Option<DecisionStrategy>,
    pub actual: DecisionStrategy,
    pub expected_reason_code: Option<&'static str>,
    pub actual_reason_code: &'static str,
    pub latency_ms: u64,
    /// 期望与 actual 是否一致（expected 为 None 时 = true，未约束）。
    pub strategy_match: bool,
    pub reason_match: bool,
}

/// 汇总报告（§32 指标）。
#[derive(Clone, Debug, Default)]
pub struct DecisionEvalReport {
    pub provider: String,
    pub total: usize,
    /// 与 reviewed/heuristic 期望一致的 case 数。
    pub matched: usize,
    /// 有期望但不一致的 case 数（missed = 该编排没编排；unnecessary = 不该编排却编排）。
    pub unmatched: usize,
    /// 该编排（非 Direct）但期望 Direct 的次数。
    pub unnecessary_orchestration: usize,
    /// 期望编排但实际 Direct 的次数。
    pub missed_orchestration: usize,
    /// 平均决策延迟（ms，向上取整）。
    pub avg_latency_ms: u64,
    /// 最大决策延迟（ms）。
    pub max_latency_ms: u64,
    /// reason code 不匹配次数（仅统计有期望的 case）。
    pub reason_mismatches: usize,
    pub outcomes: Vec<CaseOutcome>,
}

impl DecisionEvalReport {
    /// 有期望的 case 的匹配率（0..1；无期望 case 不计入分母）。
    #[must_use]
    pub fn accuracy(&self) -> f64 {
        let judged = self.matched + self.unmatched;
        if judged == 0 {
            return 1.0;
        }
        self.matched as f64 / judged as f64
    }
}

/// Eval harness：对任意 `DecisionProvider` 跑 golden dataset（§31）。
pub struct DecisionEvalHarness;

impl DecisionEvalHarness {
    /// 跑单个 provider。
    pub async fn run(provider: &dyn DecisionProvider) -> DecisionEvalReport {
        Self::run_with_workers(provider, &default_workers()).await
    }

    /// 跑单个 provider（指定可用 worker 集）。
    pub async fn run_with_workers(
        provider: &dyn DecisionProvider,
        available_workers: &[String],
    ) -> DecisionEvalReport {
        let cases = golden_decision_cases();
        let mut report = DecisionEvalReport {
            provider: provider.name().to_string(),
            total: cases.len(),
            ..DecisionEvalReport::default()
        };
        for case in &cases {
            let request = case.to_request(available_workers);
            let started = Instant::now();
            let result: DecisionResult = match provider.decide(&request).await {
                Ok(result) => result,
                // provider 失败视作 Direct（与 engine fallback 语义一致）。
                Err(_) => DecisionResult::direct(provider.name(), "provider_failed"),
            };
            let latency_ms = started.elapsed().as_millis() as u64;
            let strategy_match = case
                .expected_strategy
                .is_none_or(|expected| expected == result.strategy);
            let reason_match = case
                .expected_reason_code
                .is_none_or(|expected| expected == result.reason_code);
            if let Some(expected) = case.expected_strategy {
                if expected == result.strategy {
                    report.matched += 1;
                } else {
                    report.unmatched += 1;
                    if expected == DecisionStrategy::Direct {
                        report.unnecessary_orchestration += 1;
                    } else if result.strategy == DecisionStrategy::Direct {
                        report.missed_orchestration += 1;
                    }
                }
            }
            if case.expected_reason_code.is_some() && !reason_match {
                report.reason_mismatches += 1;
            }
            report.max_latency_ms = report.max_latency_ms.max(latency_ms);
            report.outcomes.push(CaseOutcome {
                id: case.id.to_string(),
                category: case.category.to_string(),
                label: case.label,
                expected: case.expected_strategy,
                actual: result.strategy,
                expected_reason_code: case.expected_reason_code,
                actual_reason_code: result.reason_code,
                latency_ms,
                strategy_match,
                reason_match,
            });
        }
        let latency_sum: u64 = report.outcomes.iter().map(|o| o.latency_ms).sum();
        report.avg_latency_ms = latency_sum.div_ceil(report.total.max(1) as u64);
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::decision_rule::RuleDecisionProvider;
    use devtoolbox_core::personal_ai::ProviderError;

    struct ChoiceProvider(&'static str);

    #[async_trait::async_trait]
    impl DecisionProvider for ChoiceProvider {
        fn name(&self) -> &'static str {
            self.0
        }
        async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, ProviderError> {
            // 恒定选 research_only：用于度量 unnecessary/missed orchestration。
            Ok(DecisionResult {
                strategy: DecisionStrategy::ResearchOnly,
                workers: vec!["research".into()],
                parallelism: 1,
                review_required: false,
                confidence: devtoolbox_core::agents::DecisionConfidence::High,
                reason_code: "constant",
                provider: self.0,
            })
        }
    }

    struct DirectProvider;

    #[async_trait::async_trait]
    impl DecisionProvider for DirectProvider {
        fn name(&self) -> &'static str {
            "always-direct"
        }
        async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, ProviderError> {
            Ok(DecisionResult::direct("always-direct", "constant_direct"))
        }
    }

    #[tokio::test]
    async fn rule_provider_meets_golden_expectations() {
        let report = DecisionEvalHarness::run(&RuleDecisionProvider::new()).await;
        assert_eq!(report.total, 15);
        // 有期望的 case 全部一致（rule 是这些期望的来源）。
        assert_eq!(report.unmatched, 0, "rule 未命中: {:?}", report.outcomes.iter().filter(|o| !o.strategy_match).map(|o| o.id.clone()).collect::<Vec<_>>());
        assert_eq!(report.reason_mismatches, 0);
        assert_eq!(report.accuracy(), 1.0);
        // 标签分布包含三类（§33）。
        assert!(report.outcomes.iter().any(|o| o.label == LabelSource::Reviewed));
        assert!(report.outcomes.iter().any(|o| o.label == LabelSource::Heuristic));
        assert!(report.outcomes.iter().any(|o| o.label == LabelSource::Baseline));
    }

    #[tokio::test]
    async fn constant_orchestrator_reports_unnecessary_orchestration() {
        let report = DecisionEvalHarness::run(&ChoiceProvider("constant")).await;
        // 期望 Direct 的 case 被编排 → unnecessary。
        assert!(report.unnecessary_orchestration > 0);
        assert!(report.accuracy() < 1.0);
    }

    #[tokio::test]
    async fn constant_direct_provider_reports_missed_orchestration() {
        let report = DecisionEvalHarness::run(&DirectProvider).await;
        // 期望编排（ResearchOnly / BoundedMultiAgent）但实际 Direct → missed。
        assert!(report.missed_orchestration > 0);
        assert_eq!(report.unnecessary_orchestration, 0);
    }

    #[tokio::test]
    async fn provider_failure_counts_as_direct() {
        struct Failing;
        #[async_trait::async_trait]
        impl DecisionProvider for Failing {
            fn name(&self) -> &'static str {
                "failing"
            }
            async fn decide(&self, _request: &DecisionRequest) -> Result<DecisionResult, ProviderError> {
                Err(ProviderError::unavailable("down"))
            }
        }
        let report = DecisionEvalHarness::run(&Failing).await;
        assert!(report.outcomes.iter().all(|o| o.actual == DecisionStrategy::Direct));
        assert!(report.outcomes.iter().any(|o| o.actual_reason_code == "provider_failed"));
    }

    #[tokio::test]
    async fn budget_case_is_judged_direct() {
        let report = DecisionEvalHarness::run(&RuleDecisionProvider::new()).await;
        let budget = report
            .outcomes
            .iter()
            .find(|o| o.id == "budget-exhausted")
            .expect("case");
        assert_eq!(budget.actual, DecisionStrategy::Direct);
        assert!(budget.reason_match);
    }

    #[test]
    fn golden_cases_cover_required_categories() {
        let cases = golden_decision_cases();
        let categories: Vec<&str> = cases.iter().map(|case| case.category).collect();
        for required in [
            "simple_factual",
            "single_module",
            "cross_module",
            "research",
            "deep_research",
            "server_read",
            "safe_action",
            "travel_planning",
            "document_comparison",
            "knowledge_synthesis",
            "history_context",
            "language_context",
        ] {
            assert!(
                categories.contains(&required),
                "golden dataset 缺少类别: {required}"
            );
        }
    }
}
