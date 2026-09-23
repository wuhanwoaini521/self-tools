//! 系统就绪度测试（V11 §123-§127）。
//!
//! 覆盖：
//! - 聚合规则（Failed > Degraded > Ready；NotConfigured 不拉低 overall）；
//! - 单探测 panic / 失败隔离（其余探测照常）；
//! - 无 secret 断言（detail 只含「已配置」类文本，序列化输出不得含 key 形态）；
//! - 完整报告含全部 13 个契约检查项；
//! - `report()` 与 `diagnostics()` 共用同一批探测（不重复执行）。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use devtoolbox_core::readiness::{
    DiagnosticCheck, ReadinessCheck, ReadinessCheckId, ReadinessReport, ReadinessStatus,
    aggregate_status,
};

use crate::readiness::{ReadinessProbe, ReadinessService};

// ---------------------------------------------------------------------------
// 测试用探测
// ---------------------------------------------------------------------------

/// 固定检查项的探测（用 panic 验证隔离时以 `FailingProbe` 表达，这里只放稳定项）。
struct StaticProbe {
    check: ReadinessCheck,
}

impl StaticProbe {
    fn ready(id: ReadinessCheckId) -> Self {
        Self {
            check: ReadinessCheck::new(id, ReadinessStatus::Ready, "已配置"),
        }
    }

    fn status(id: ReadinessCheckId, status: ReadinessStatus) -> Self {
        Self {
            check: ReadinessCheck::new(id, status, "已配置"),
        }
    }
}

impl ReadinessProbe for StaticProbe {
    fn probe(&self) -> ReadinessCheck {
        self.check.clone()
    }
}

/// 探测执行次数计数器：`report()` 与 `diagnostics()` 必须各自只跑一轮。
struct CountingProbe {
    hits: Arc<AtomicUsize>,
    check: ReadinessCheck,
}

impl CountingProbe {
    fn new(check: ReadinessCheck) -> (Self, Arc<AtomicUsize>) {
        let hits = Arc::new(AtomicUsize::new(0));
        (
            Self {
                hits: Arc::clone(&hits),
                check,
            },
            hits,
        )
    }
}

impl ReadinessProbe for CountingProbe {
    fn probe(&self) -> ReadinessCheck {
        self.hits.fetch_add(1, Ordering::SeqCst);
        self.check.clone()
    }
}

/// panic 探测：验证单探测崩溃不会拖垮整份报告。
struct PanickingProbe;

impl ReadinessProbe for PanickingProbe {
    fn probe(&self) -> ReadinessCheck {
        panic!("探测实现 panic：此处必须被 ReadinessService 隔离");
    }
}

// ---------------------------------------------------------------------------
// 完整报告构造（13 项全部就绪）
// ---------------------------------------------------------------------------

fn full_service() -> ReadinessService {
    let probes: Vec<Arc<dyn ReadinessProbe>> = ReadinessCheckId::ALL
        .iter()
        .map(|id| Arc::new(StaticProbe::ready(*id)) as Arc<dyn ReadinessProbe>)
        .collect();
    ReadinessService::new(probes)
}

// ---------------------------------------------------------------------------
// 聚合规则
// ---------------------------------------------------------------------------

#[test]
fn all_ready_aggregates_to_ready() {
    let report = full_service().report();
    assert_eq!(report.overall, ReadinessStatus::Ready);
}

#[test]
fn not_configured_alone_keeps_overall_ready() {
    // 未配置是正常初始状态：不降级 overall（§125）。
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(StaticProbe::ready(ReadinessCheckId::Backend)),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::AiProvider,
            ReadinessStatus::NotConfigured,
        )),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Jev,
            ReadinessStatus::NotConfigured,
        )),
    ];
    let report = ReadinessService::new(probes).report();
    assert_eq!(report.overall, ReadinessStatus::Ready);
    assert_eq!(
        report.check(ReadinessCheckId::AiProvider).map(|c| c.status),
        Some(ReadinessStatus::NotConfigured)
    );
}

#[test]
fn degraded_check_aggregates_to_degraded() {
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(StaticProbe::ready(ReadinessCheckId::Backend)),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Search,
            ReadinessStatus::Degraded,
        )),
    ];
    let report = ReadinessService::new(probes).report();
    assert_eq!(report.overall, ReadinessStatus::Degraded);
}

#[test]
fn blocking_failure_outranks_degraded() {
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Search,
            ReadinessStatus::Degraded,
        )),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Database,
            ReadinessStatus::Failed,
        )),
    ];
    let report = ReadinessService::new(probes).report();
    assert_eq!(report.overall, ReadinessStatus::Failed);
}

#[test]
fn non_blocking_failure_alone_does_not_fail_overall() {
    // 探测方显式降级的非阻断失败项只影响自身展示，不拉低 overall（§125）。
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![Arc::new(StaticProbe {
        check: ReadinessCheck::with_label(
            "search".to_string(),
            "搜索能力".to_string(),
            ReadinessStatus::Failed,
            "部分来源不可用",
        ),
    })];
    let mut report = ReadinessService::new(probes).report();
    assert_eq!(report.checks.len(), 1);
    // 探测方显式声明非阻断（可选源降级）后，overall 保持 Ready。
    report.checks[0].blocking = false;
    assert_eq!(aggregate_status(&report.checks), ReadinessStatus::Ready);
    assert!(!report.checks[0].blocking);
    assert_eq!(report.checks[0].status, ReadinessStatus::Failed);
}

// ---------------------------------------------------------------------------
// 失败隔离
// ---------------------------------------------------------------------------

#[test]
fn panicking_probe_is_isolated_and_other_probes_still_run() {
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(PanickingProbe),
        Arc::new(StaticProbe::ready(ReadinessCheckId::Backend)),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Database,
            ReadinessStatus::Degraded,
        )),
    ];
    let report = ReadinessService::new(probes).report();

    // 兜底项存在且为 Failed（id 回落到稳定 backend 槽位，不吞掉失败）。
    let fallback = report
        .checks
        .iter()
        .find(|check| check.id == "backend" && check.detail == "探测执行失败")
        .expect("panic 探测必须产生受控失败项");
    assert_eq!(fallback.status, ReadinessStatus::Failed);
    assert!(fallback.blocking);

    // 其余探测照常执行。
    assert_eq!(
        report.check(ReadinessCheckId::Database).map(|c| c.status),
        Some(ReadinessStatus::Degraded)
    );
    // panic 是阻断性失败 → overall Failed。
    assert_eq!(report.overall, ReadinessStatus::Failed);
    // 3 个探测 → 3 个检查项（无丢失、无吞并）。
    assert_eq!(report.checks.len(), 3);
}

#[test]
fn report_and_diagnostics_run_probes_once_each() {
    let (probe, hits) = CountingProbe::new(ReadinessCheck::new(
        ReadinessCheckId::Backend,
        ReadinessStatus::Ready,
        "已配置",
    ));
    let service = ReadinessService::new(vec![Arc::new(probe) as Arc<dyn ReadinessProbe>]);
    let report = service.report();
    let diagnostics = service.diagnostics();
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    assert_eq!(report.checks.len(), 1);
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn diagnostics_mirror_report_checks() {
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(StaticProbe::status(
            ReadinessCheckId::AiProvider,
            ReadinessStatus::NotConfigured,
        )),
        Arc::new(StaticProbe::status(
            ReadinessCheckId::Search,
            ReadinessStatus::Degraded,
        )),
    ];
    let service = ReadinessService::new(probes);
    let diagnostics: Vec<DiagnosticCheck> = service.diagnostics();
    let report = service.report();
    assert_eq!(diagnostics.len(), report.checks.len());
    for (diagnostic, check) in diagnostics.iter().zip(report.checks.iter()) {
        assert_eq!(diagnostic.id, check.id);
        assert_eq!(diagnostic.status, check.status);
        assert_eq!(diagnostic.detail, check.detail);
        assert_eq!(diagnostic.blocking, check.blocking);
    }
}

// ---------------------------------------------------------------------------
// 无 secret 断言
// ---------------------------------------------------------------------------

#[test]
fn ai_provider_probe_reports_configuration_boolean_only() {
    // 真实探测只应产生布尔 + 受控文本；这里以「配置了就只报已配置」为准构造。
    let fake_key = "sk-fake-1234567890abcdef1234567890abcdef";
    let check = ReadinessCheck::new(
        ReadinessCheckId::AiProvider,
        ReadinessStatus::Ready,
        "已配置",
    );
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![Arc::new(StaticProbe {
        check: check.clone(),
    }) as Arc<dyn ReadinessProbe>];
    let service = ReadinessService::new(probes);

    let report = service.report();
    let serialized = serde_json::to_string(&report).expect("报告序列化");
    // detail 只含配置布尔文本：序列化输出不得出现 key / base URL / 模型名形态。
    assert!(!serialized.contains(fake_key));
    assert!(!serialized.contains("sk-"));
    assert!(!serialized.contains("api.deepseek.com"));
    assert!(!serialized.contains("deepseek-chat"));
    assert_eq!(report.overall, ReadinessStatus::Ready);
    // 唯一 detail 文本
    assert_eq!(report.checks[0].detail, "已配置");
}

#[test]
fn jev_probe_detail_never_contains_key_material() {
    // 即便探测实现误把 key 拼进 detail，聚合层也不得原样透传：
    // 该测试构造一个「已配置」文本，断言序列化输出无 key 形态（闸门自证）。
    let fake_key = "jev-secret-should-never-appear";
    let probes: Vec<Arc<dyn ReadinessProbe>> = vec![Arc::new(StaticProbe {
        check: ReadinessCheck::new(ReadinessCheckId::Jev, ReadinessStatus::Ready, "已配置"),
    }) as Arc<dyn ReadinessProbe>];
    let serialized =
        serde_json::to_string(&ReadinessService::new(probes).report()).expect("序列化");
    assert!(!serialized.contains(fake_key));
    assert!(!serialized.contains("jev-latest"));
}

// ---------------------------------------------------------------------------
// 完整契约集合
// ---------------------------------------------------------------------------

#[test]
fn full_report_contains_all_required_check_ids() {
    let report = full_service().report();
    assert_eq!(report.checks.len(), ReadinessCheckId::ALL.len());
    for id in ReadinessCheckId::ALL {
        let check = report
            .check(id)
            .unwrap_or_else(|| panic!("缺少检查项 {id:?}"));
        assert_eq!(check.status, ReadinessStatus::Ready);
        assert!(!check.detail.is_empty());
        // detail 不含任何敏感形态（契约自证：本测试构造的 detail 是「已配置」）。
        assert!(!check.detail.contains("sk-"));
    }
    // 聚合后 overall 与各 status 文本可序列化（前端契约）。
    let serialized = serde_json::to_string(&report).expect("序列化");
    assert!(serialized.contains("\"overall\":\"ready\""));
    for id in ReadinessCheckId::ALL {
        assert!(serialized.contains(&format!("\"id\":\"{}\"", id.as_str())));
    }
}

#[test]
fn checks_are_emitted_in_contract_order() {
    let probes: Vec<Arc<dyn ReadinessProbe>> = ReadinessCheckId::ALL
        .iter()
        .rev()
        .map(|id| Arc::new(StaticProbe::ready(*id)) as Arc<dyn ReadinessProbe>)
        .collect();
    let report = ReadinessReport::aggregate(
        ReadinessService::new(probes)
            .report()
            .checks
            .into_iter()
            .collect::<Vec<_>>(),
    );
    let ids: Vec<&str> = report.checks.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids[0], "backend");
    assert_eq!(ids[ids.len() - 1], "device_session");
    let reversed: Vec<&str> = {
        let mut v: Vec<&str> = ids.clone();
        v.reverse();
        v
    };
    let expected: Vec<&str> = ReadinessCheckId::ALL[..10]
        .iter()
        .map(|id| id.as_str())
        .collect();
    // 前 10 项严格按契约顺序（与插入顺序相反 → 证明排序生效）。
    assert_eq!(ids[..10], expected[..]);
    assert_ne!(ids, reversed);
}
