//! 系统指标契约（V7 §14-§17）。
//!
//! 纯数据形状：platform 语义（`Platform`）、CPU / 内存、存储卷。
//! **任何字段 unavailable 都是合法状态**（Gate 2：部分指标缺失不 panic），
//! 由 [`SystemMetrics::unavailable`] 标记，健康评估据此降级为 `Unknown`。

use serde::{Deserialize, Serialize};

/// 目标平台（§23：macOS 12 是目标，CI/开发机可能是其它）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    #[default]
    Unknown,
    MacOs,
    Linux,
    Windows,
}

impl Platform {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Unknown => "unknown",
            Platform::MacOs => "macos",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Platform::Unknown => "未知平台",
            Platform::MacOs => "macOS",
            Platform::Linux => "Linux",
            Platform::Windows => "Windows",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "macos" | "mac_os" | "darwin" => Some(Platform::MacOs),
            "linux" => Some(Platform::Linux),
            "windows" => Some(Platform::Windows),
            _ => None,
        }
    }
}

/// CPU 快照（§15）。`usage_ratio` 缺失 → `None`（不猜测 0.0）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// 0.0–1.0；`None` = 采样不可用。
    pub usage_ratio: Option<f32>,
    /// 逻辑核心数；0 = 未知。
    pub logical_cores: u32,
    /// 1 / 5 / 15 分钟负载（平台不支持 → 全 `None`）。
    pub load_average: Option<(f32, f32, f32)>,
}

/// 内存快照（§15）。字段均不可用时 `available_bytes = None`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: Option<u64>,
}

impl MemoryMetrics {
    /// 使用率（0.0–1.0）；总数未知 → `None`。
    #[must_use]
    pub fn usage_ratio(&self) -> Option<f32> {
        if self.total_bytes == 0 {
            return None;
        }
        Some((self.used_bytes as f64 / self.total_bytes as f64) as f32)
    }
}

/// 存储卷类别（§17：过滤虚拟/临时卷的策略输入）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeKind {
    /// 主系统盘。
    #[default]
    SystemDisk,
    /// 用户数据盘。
    DataDisk,
    Removable,
    Network,
    Temporary,
    Virtual,
    Unknown,
}

impl VolumeKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            VolumeKind::SystemDisk => "system_disk",
            VolumeKind::DataDisk => "data_disk",
            VolumeKind::Removable => "removable",
            VolumeKind::Network => "network",
            VolumeKind::Temporary => "temporary",
            VolumeKind::Virtual => "virtual",
            VolumeKind::Unknown => "unknown",
        }
    }

    /// 默认是否展示（§17：隐藏临时 / 虚拟卷）。
    #[must_use]
    pub fn is_user_visible(self) -> bool {
        !matches!(self, VolumeKind::Temporary | VolumeKind::Virtual)
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "system_disk" | "systemdisk" => Some(VolumeKind::SystemDisk),
            "data_disk" | "datadisk" => Some(VolumeKind::DataDisk),
            "removable" => Some(VolumeKind::Removable),
            "network" => Some(VolumeKind::Network),
            "temporary" => Some(VolumeKind::Temporary),
            "virtual" => Some(VolumeKind::Virtual),
            _ => None,
        }
    }
}

/// 单个卷的指标（§16）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StorageMetrics {
    /// 挂载点 / 卷名（展示用；如 `/`、`/System/Volumes/Data`）。
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub kind: VolumeKind,
}

impl StorageMetrics {
    /// 使用率（0.0–1.0）；总量未知 → `None`。
    #[must_use]
    pub fn usage_ratio(&self) -> Option<f32> {
        if self.total_bytes == 0 {
            return None;
        }
        Some((self.used_bytes as f64 / self.total_bytes as f64) as f32)
    }
}

/// 系统指标总览（§15/§19）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub hostname: String,
    pub platform: Platform,
    pub os_version: String,
    pub architecture: String,
    /// 运行秒数；0 = 未知。
    pub uptime_secs: u64,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    /// 存储卷（已按 §17 过滤后的集合由调用方决定；这里是全量）。
    pub storage: Vec<StorageMetrics>,
    /// 采样时间（Unix 秒）。
    pub sampled_at: i64,
}

impl SystemMetrics {
    /// 关键字段全部缺失时的「未知」标记（Gate 2 Case B）。
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        self.hostname.is_empty()
            && self.uptime_secs == 0
            && self.cpu.usage_ratio.is_none()
            && self.memory.total_bytes == 0
            && self.storage.is_empty()
    }

    /// 默认展示的卷（§17）。
    #[must_use]
    pub fn visible_storage(&self) -> Vec<&StorageMetrics> {
        self.storage
            .iter()
            .filter(|volume| volume.kind.is_user_visible())
            .collect()
    }

    /// 使用率最高的可见卷（§149「哪个盘快满了」）。
    #[must_use]
    pub fn tightest_storage(&self) -> Option<&StorageMetrics> {
        self.visible_storage()
            .into_iter()
            .filter_map(|volume| volume.usage_ratio().map(|ratio| (volume, ratio)))
            .max_by(|(_, left), (_, right)| {
                left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(volume, _)| volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(mount: &str, used: u64, total: u64, kind: VolumeKind) -> StorageMetrics {
        StorageMetrics {
            mount: mount.to_string(),
            used_bytes: used,
            total_bytes: total,
            available_bytes: total - used,
            kind,
        }
    }

    #[test]
    fn memory_ratio_requires_total() {
        assert_eq!(MemoryMetrics::default().usage_ratio(), None);
        let metrics = MemoryMetrics {
            total_bytes: 100,
            used_bytes: 42,
            available_bytes: Some(58),
        };
        assert_eq!(metrics.usage_ratio(), Some(0.42));
    }

    #[test]
    fn storage_filters_temporary_and_virtual_by_default() {
        let metrics = SystemMetrics {
            storage: vec![
                volume("/", 50, 100, VolumeKind::SystemDisk),
                volume("/Volumes/ram", 90, 100, VolumeKind::Temporary),
                volume("map auto_home", 1, 1, VolumeKind::Virtual),
                volume("/Volumes/Data", 80, 100, VolumeKind::DataDisk),
            ],
            ..SystemMetrics::default()
        };
        let visible: Vec<&str> = metrics
            .visible_storage()
            .into_iter()
            .map(|entry| entry.mount.as_str())
            .collect();
        assert_eq!(visible, vec!["/", "/Volumes/Data"]);
        let tightest = metrics.tightest_storage().expect("tightest");
        assert_eq!(tightest.mount, "/Volumes/Data", "80% 高于 50%");
    }

    #[test]
    fn unknown_metrics_flag_when_everything_missing() {
        assert!(SystemMetrics::default().is_unknown());
        let partial = SystemMetrics {
            hostname: "mac-studio".into(),
            ..SystemMetrics::default()
        };
        assert!(!partial.is_unknown(), "任一字段可用即非全未知");
    }

    #[test]
    fn volume_kind_round_trips() {
        for kind in [
            VolumeKind::SystemDisk,
            VolumeKind::DataDisk,
            VolumeKind::Temporary,
            VolumeKind::Virtual,
        ] {
            assert_eq!(VolumeKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(VolumeKind::parse("nonsense"), None);
    }
}
