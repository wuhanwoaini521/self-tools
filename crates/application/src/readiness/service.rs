//! 系统就绪度探测（V11 §123-§127）。
//!
//! 依赖方向：只依赖 `devtoolbox-core::readiness` 契约；不感知 Tauri / HTTP /
//! SQLite 具体实现。`ReadinessProbe` 由组合根（apps/desktop 或 apps/server）
//! 装配，`ReadinessService` 只做**只读聚合**。
//!
//! 契约：
//! - `report()` 依次执行全部探测，按 `ReadinessReport::aggregate` 聚合；
//! - `diagnostics()` 复用同一批探测结果（不重复执行），逐项转诊断视图；
//! - 探测 panic / 降级为受控 `Failed` 项，**绝不让单个探测拖垮整份报告**；
//! - 输出不含 secret（§126）：detail 只允许「已配置 / 未配置 / 能力布尔」。

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use devtoolbox_core::readiness::{
    DiagnosticCheck, ReadinessCheck, ReadinessCheckId, ReadinessReport, ReadinessStatus,
    aggregate_status,
};

use crate::time::now_unix;

/// 只读就绪度探测端口：一个子系统一个实现，`probe()` 必须自限时间。
pub trait ReadinessProbe: Send + Sync {
    /// 执行一次探测，返回受控的检查项。
    ///
    /// 实现方必须保证：`detail` 不含 API key / base URL / 模型名（§126）。
    fn probe(&self) -> ReadinessCheck;
}

/// 就绪度服务：持有全部探测，产出聚合报告与逐项诊断。
pub struct ReadinessService {
    probes: Vec<Arc<dyn ReadinessProbe>>,
}

/// panic 兜底检查项的稳定 id（探测自身未按 id 契约返回时）。
const FALLBACK_CHECK_ID: &str = "backend";

impl ReadinessService {
    #[must_use]
    pub fn new(probes: Vec<Arc<dyn ReadinessProbe>>) -> Self {
        Self { probes }
    }

    /// 追加探测（组合根按条件装配时使用）。
    #[must_use]
    pub fn with_probe(mut self, probe: Arc<dyn ReadinessProbe>) -> Self {
        self.probes.push(probe);
        self
    }

    /// 已注册的探测数量（诊断面板 / 测试用）。
    #[must_use]
    pub fn probe_count(&self) -> usize {
        self.probes.len()
    }

    /// 执行全部探测并聚合（§123/§125）。
    ///
    /// 探测顺序不影响结果：输出按 `ReadinessCheckId::ALL` 稳定排序。
    #[must_use]
    pub fn report(&self) -> ReadinessReport {
        let mut report = ReadinessReport::aggregate(self.probe_all());
        report.generated_at = now_unix();
        report
    }

    /// 逐项诊断（§127）：与 `report()` 共用一次探测结果，仍为只读。
    #[must_use]
    pub fn diagnostics(&self) -> Vec<DiagnosticCheck> {
        self.probe_all().into_iter().map(DiagnosticCheck::from).collect()
    }

    /// 执行全部探测。单个探测 panic 视为该项 `Failed`，其余探测照常执行
    /// （优雅降级：诊断面板永远拿得到完整结果）。
    fn probe_all(&self) -> Vec<ReadinessCheck> {
        let mut checks: Vec<ReadinessCheck> = self
            .probes
            .iter()
            .map(|probe| {
                std::panic::catch_unwind(AssertUnwindSafe(|| probe.probe())).unwrap_or_else(
                    |_| {
                        ReadinessCheck::with_label(
                            FALLBACK_CHECK_ID.to_string(),
                            "后端探测".to_string(),
                            ReadinessStatus::Failed,
                            "探测执行失败",
                        )
                    },
                )
            })
            .collect();
        sort_stable(&mut checks);
        checks
    }
}

/// 按 `ReadinessCheckId::ALL` 的契约顺序稳定排序；未知 id 保持相对顺序置尾。
fn sort_stable(checks: &mut [ReadinessCheck]) {
    let rank = |check: &ReadinessCheck| {
        ReadinessCheckId::parse(&check.id)
            .map_or(ReadinessCheckId::ALL.len(), |id| {
                ReadinessCheckId::ALL
                    .iter()
                    .position(|candidate| *candidate == id)
                    .unwrap_or(ReadinessCheckId::ALL.len())
            })
    };
    checks.sort_by_key(rank);
}

/// 聚合规则再导出（调用方无需 import core 的辅助函数）。
#[must_use]
pub fn aggregate(checks: &[ReadinessCheck]) -> ReadinessStatus {
    aggregate_status(checks)
}
