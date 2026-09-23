//! 系统指标适配器（V7 §14-§17/§23）。
//!
//! 平台差异全部收敛在这里：macOS 用 `sysctl` / `df`（固定参数），
//! Linux 读 `/proc`，Windows 返回 `Unknown`。**没有**任意命令执行。

use std::process::Command;

use devtoolbox_core::server::{
    CpuMetrics, MemoryMetrics, Platform, StorageMetrics, SystemMetrics, VolumeKind,
};

/// 本地平台采样（macOS / Linux）。返回 core 的 `SystemMetrics`；
///「是否可用」由 application 的端口适配器判定。
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalSystemMetrics;

impl LocalSystemMetrics {
    /// 采样一份完整指标；任何平台不可得的字段保持默认（`Unknown` 语义）。
    #[must_use]
    pub fn sample(&self) -> SystemMetrics {
        SystemMetrics {
            hostname: hostname(),
            platform: detect_platform(),
            os_version: os_version(),
            architecture: std::env::consts::ARCH.to_string(),
            uptime_secs: uptime_secs(),
            cpu: cpu_metrics(),
            memory: memory_metrics(),
            storage: storage_volumes(),
            sampled_at: unix_now(),
        }
    }
}

/// 只上报平台身份、不上报指标（未启用采样的平台）。
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformSystemMetrics;

impl PlatformSystemMetrics {
    #[must_use]
    pub fn sample(&self) -> SystemMetrics {
        SystemMetrics {
            hostname: hostname(),
            platform: detect_platform(),
            os_version: String::new(),
            architecture: std::env::consts::ARCH.to_string(),
            uptime_secs: 0,
            cpu: CpuMetrics::default(),
            memory: MemoryMetrics::default(),
            storage: Vec::new(),
            sampled_at: unix_now(),
        }
    }
}

fn detect_platform() -> Platform {
    match std::env::consts::OS {
        "macos" => Platform::MacOs,
        "linux" => Platform::Linux,
        "windows" => Platform::Windows,
        _ => Platform::Unknown,
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// hostname：读环境变量或 `/etc/hostname`；失败 → 空串（上层据此降级）。
fn hostname() -> String {
    if let Ok(value) = std::env::var("HOSTNAME")
        && !value.trim().is_empty()
    {
        return value.trim().to_string();
    }
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

/// OS 版本：macOS 用固定 `sw_vers -productVersion`，Linux 读 os-release。
fn os_version() -> String {
    match std::env::consts::OS {
        "macos" => run_capture("sw_vers", &["-productVersion"])
            .unwrap_or_default()
            .trim()
            .to_string(),
        "linux" => std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("VERSION_ID="))
                    .map(|line| {
                        line.trim_start_matches("VERSION_ID=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// 运行秒数：macOS `sysctl -n kern.boottime`，Linux `/proc/uptime`。
fn uptime_secs() -> u64 {
    let now = unix_now();
    match std::env::consts::OS {
        "macos" => run_capture("sysctl", &["-n", "kern.boottime"])
            .and_then(|output| {
                // "sec = 1690000000, usec = 0" → 取 sec。
                output
                    .split("sec =")
                    .nth(1)
                    .and_then(|rest| rest.split(',').next())
                    .and_then(|value| value.trim().parse::<i64>().ok())
            })
            .map(|boot| now.saturating_sub(boot) as u64)
            .unwrap_or(0),
        "linux" => std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|content| {
                content
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<f64>().ok())
            })
            .map(|seconds| seconds as u64)
            .unwrap_or(0),
        _ => 0,
    }
}

fn cpu_metrics() -> CpuMetrics {
    let logical_cores = std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(0);
    let usage_ratio = match std::env::consts::OS {
        "linux" => linux_cpu_usage(),
        // macOS 需要 host_statistics（C API）；V7 不引入额外依赖，
        // 相关字段保持 `None` → 健康评估按 Unknown 降级（§23「必要时才调用」的反面：
        // 非必要不引入）。
        _ => None,
    };
    let load_average = match std::env::consts::OS {
        "linux" => linux_load_average(),
        "macos" => {
            run_capture("sysctl", &["-n", "vm.loadavg"]).and_then(|output| parse_loadavg(&output))
        }
        _ => None,
    };
    CpuMetrics {
        usage_ratio,
        logical_cores,
        load_average,
    }
}

fn linux_cpu_usage() -> Option<f32> {
    // 单次采样不可得 → None（不编造 0.0）。需要差值采样时由后续 Gate 扩展。
    let _ = std::fs::read_to_string("/proc/stat").ok()?;
    None
}

fn linux_load_average() -> Option<(f32, f32, f32)> {
    let content = std::fs::read_to_string("/proc/loadavg").ok()?;
    let mut parts = content.split_whitespace();
    let one = parts.next()?.parse::<f32>().ok()?;
    let five = parts.next()?.parse::<f32>().ok()?;
    let fifteen = parts.next()?.parse::<f32>().ok()?;
    Some((one, five, fifteen))
}

/// macOS `vm.loadavg` 形如 `{ 1.23 1.45 1.67 }`。
fn parse_loadavg(raw: &str) -> Option<(f32, f32, f32)> {
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    let mut parts = trimmed.split_whitespace();
    let one = parts.next()?.parse::<f32>().ok()?;
    let five = parts.next()?.parse::<f32>().ok()?;
    let fifteen = parts.next()?.parse::<f32>().ok()?;
    Some((one, five, fifteen))
}

fn memory_metrics() -> MemoryMetrics {
    match std::env::consts::OS {
        "macos" => macos_memory(),
        "linux" => linux_memory(),
        _ => MemoryMetrics::default(),
    }
}

fn macos_memory() -> MemoryMetrics {
    let total = run_capture("sysctl", &["-n", "hw.memsize"])
        .and_then(|output| output.trim().parse::<u64>().ok())
        .unwrap_or(0);
    // used/available 需要 host_statistics；V7 只报 total（其余 None → Unknown）。
    MemoryMetrics {
        total_bytes: total,
        used_bytes: 0,
        available_bytes: None,
    }
}

fn linux_memory() -> MemoryMetrics {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total = 0u64;
    let mut available = 0u64;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = parse_kib(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = parse_kib(rest);
        }
    }
    MemoryMetrics {
        total_bytes: total,
        used_bytes: total.saturating_sub(available),
        available_bytes: (total > 0).then_some(available),
    }
}

fn parse_kib(raw: &str) -> u64 {
    raw.split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib * 1_024)
        .unwrap_or(0)
}

fn storage_volumes() -> Vec<StorageMetrics> {
    match std::env::consts::OS {
        "macos" => df_volumes(),
        "linux" => df_volumes(),
        _ => Vec::new(),
    }
}

/// `df -k -P` 固定参数（§24：固定 executable + 固定 argument template）。
fn df_volumes() -> Vec<StorageMetrics> {
    let Some(output) = run_capture("df", &["-k", "-P"]) else {
        return Vec::new();
    };
    output.lines().skip(1).filter_map(parse_df_line).collect()
}

/// 解析一行 `df -k -P`：`Filesystem 1K-blocks Used Available Capacity Mounted on`。
fn parse_df_line(line: &str) -> Option<StorageMetrics> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    // macOS `df -P` 的 Filesystem 列可能含空格（`map auto_home` / `devfs` 虚拟卷）：
    // 从左侧吃掉字段直到遇见第一个纯数字（1K-blocks）。
    let mut index = 0usize;
    while index < fields.len() && fields[index].parse::<u64>().is_err() {
        index += 1;
    }
    if index == 0 || index + 3 >= fields.len() {
        return None;
    }
    let total_kib = fields[index].parse::<u64>().ok()?;
    let used_kib = fields[index + 1].parse::<u64>().ok()?;
    let available_kib = fields[index + 2].parse::<u64>().ok()?;
    // Mounted on 之后的 `(autofs, noowners)` 是挂载描述，不属于路径。
    let mount = fields[(index + 4)..]
        .iter()
        .take_while(|field| !field.starts_with('('))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if mount.is_empty() {
        return None;
    }
    if mount.is_empty() {
        return None;
    }
    // `total_kib == 0` 是合法形态（auto_home / devfs 等虚拟卷）——保留，
    // 由 `usage_ratio() == None` 表达「无法评估」，而不是丢弃整行。
    let device = fields[..index].join(" ");
    Some(StorageMetrics {
        kind: classify_volume_with_device(&mount, &device),
        mount,
        total_bytes: total_kib * 1_024,
        used_bytes: used_kib * 1_024,
        available_bytes: available_kib * 1_024,
    })
}

/// 卷分类（§17：纯函数规则，平台无关、可单测）。
pub fn classify_volume(mount: &str) -> VolumeKind {
    classify_volume_with_device(mount, "")
}

/// 卷分类（可带 filesystem 设备名辅助判定虚拟卷，如 `map auto_home`）。
#[must_use]
pub fn classify_volume_with_device(mount: &str, device: &str) -> VolumeKind {
    let lower = mount.to_ascii_lowercase();
    let device = device.to_ascii_lowercase();
    if device.starts_with("map ") || device.starts_with("devfs") {
        return VolumeKind::Virtual;
    }
    if lower == "/" {
        return VolumeKind::SystemDisk;
    }
    if lower.starts_with("/system/volumes/data") {
        return VolumeKind::DataDisk;
    }
    if lower.starts_with("/volumes/") {
        return VolumeKind::Removable;
    }
    if lower.starts_with("/private/var/folders") || lower.starts_with("/tmp") || lower == "/dev" {
        return VolumeKind::Temporary;
    }
    if lower.starts_with("/network") || lower.starts_with("//") || lower.starts_with("smb:") {
        return VolumeKind::Network;
    }
    if lower.starts_with("map ") || lower.starts_with("devfs") || lower.starts_with("/dev/") {
        return VolumeKind::Virtual;
    }
    VolumeKind::Unknown
}

/// 固定 executable + 固定 args 的命令执行（**唯一**允许的进程调用形态）。
///
/// argv 全部由本文件的字面量提供；调用方无法注入参数。失败（不存在 / 非零
/// 退出）→ `None`，上层降级为 `Unknown`（§8 fail-closed 的反面：不假装成功）。
pub(crate) fn run_capture(executable: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(executable).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_never_panics_on_any_platform() {
        // CI 可能跑在 Linux / Windows：只要不 panic 且形状合法即通过。
        let metrics = LocalSystemMetrics.sample();
        assert!(!metrics.architecture.is_empty());
        assert!(metrics.sampled_at > 0);
    }

    #[test]
    fn platform_metrics_report_unknown_without_lying() {
        let metrics = PlatformSystemMetrics.sample();
        assert!(metrics.is_unknown(), "未配置采样 → 全 Unknown");
        assert_eq!(metrics.cpu.usage_ratio, None);
        assert_eq!(metrics.memory.total_bytes, 0);
    }

    #[test]
    fn df_line_parses_into_storage_metrics() {
        let parsed =
            parse_df_line("/dev/disk3s1s1 976490568 610000000 340000000    65% /").expect("parse");
        assert_eq!(parsed.mount, "/");
        assert_eq!(parsed.total_bytes, 976_490_568 * 1_024);
        assert_eq!(parsed.used_bytes, 610_000_000 * 1_024);
        assert!((parsed.usage_ratio().unwrap_or(0.0) - 0.6245).abs() < 0.01);
    }

    #[test]
    fn df_line_with_spaces_in_mount_keeps_full_path() {
        let parsed = parse_df_line(
            "map auto_home 0 0 0 100% /System/Volumes/Data/Users/me (autofs, noowners)",
        )
        .expect("parse");
        assert_eq!(parsed.kind, VolumeKind::Virtual, "map auto_home → 虚拟卷");
        assert_eq!(
            parsed.mount, "/System/Volumes/Data/Users/me",
            "挂载描述 (autofs, noowners) 必须剥离"
        );
    }

    #[test]
    fn volume_classification_matches_policy() {
        assert_eq!(classify_volume("/"), VolumeKind::SystemDisk);
        assert_eq!(
            classify_volume_with_device("/System/Volumes/Data", "map auto_home"),
            VolumeKind::Virtual,
            "设备名优先于挂载点"
        );
        assert_eq!(
            classify_volume("/System/Volumes/Data"),
            VolumeKind::DataDisk
        );
        assert_eq!(classify_volume("/Volumes/Backup"), VolumeKind::Removable);
        assert_eq!(
            classify_volume("/private/var/folders/ab/cd"),
            VolumeKind::Temporary
        );
        assert_eq!(classify_volume("map auto_home"), VolumeKind::Virtual);
        assert_eq!(classify_volume("/Volumes/weird"), VolumeKind::Removable);
    }

    #[test]
    fn loadavg_parsing_handles_bsd_shape() {
        assert_eq!(
            parse_loadavg("{ 1.23 1.45 1.67 }"),
            Some((1.23, 1.45, 1.67))
        );
        assert_eq!(parse_loadavg("garbage"), None);
    }
}
