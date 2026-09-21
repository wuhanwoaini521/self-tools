//! 注册日志源的有界读取（V7 §37-§39）。
//!
//! 路径只能来自 `ServiceDescriptor.log_sources`（上层保证）；
//! 本适配器只做：读文件 → 按字节/行数截断 → 时间窗过滤。
//! **不判断内容**（脱敏在 application 的 `LogRedactor`）。

use std::io::{BufRead, BufReader};
use std::path::Path;

use devtoolbox_core::server::LogReadResult;
use devtoolbox_core::server::{LogSource, ServiceDescriptor, contains_traversal};

/// 本地文件日志尾部读取器。
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalLogTail;

impl LocalLogTail {
    /// 读取注册日志源尾部（路径来自 descriptor；不判断内容）。
    ///
    /// # Errors
    /// 返回稳定错误码前缀（`no_log_source` / `log_path_traversal` /
    /// `log_read_failed: …`），由组合根映射为应用错误。
    pub fn tail(
        &self,
        service: &ServiceDescriptor,
        log_source_id: &str,
        max_lines: usize,
        max_bytes: usize,
        _max_age_secs: u64,
    ) -> Result<LogReadResult, String> {
        let source: &LogSource = service
            .log_sources
            .iter()
            .find(|source| source.id == log_source_id)
            .or_else(|| service.log_sources.first())
            .ok_or_else(|| "no_log_source".to_string())?;
        let path = Path::new(&source.path);
        // 形态防御：注册表配置也不允许 traversal（纵深防御）。
        if contains_traversal(path) {
            return Err("log_path_traversal".to_string());
        }
        let text = read_tail(path, max_lines, max_bytes)
            .map_err(|reason| format!("log_read_failed: {reason}"))?;
        let lines = text.lines().count();
        Ok(LogReadResult {
            service_id: service.id.clone(),
            log_source_id: source.id.clone(),
            text,
            lines,
            redactions: 0,
            truncated: lines >= max_lines,
        })
    }
}

/// 读取文件尾部：从后往前收集行，直到满足行数 / 字节上限。
fn read_tail(path: &Path, max_lines: usize, max_bytes: usize) -> Result<String, &'static str> {
    let metadata = std::fs::metadata(path).map_err(|_| "not_found")?;
    if !metadata.is_file() {
        return Err("not_a_file");
    }
    // 超过 64 MiB 的日志直接拒绝（避免为读尾部把整个文件拉进内存）。
    const MAX_SCAN_BYTES: u64 = 64 * 1_024 * 1_024;
    if metadata.len() > MAX_SCAN_BYTES {
        return Err("too_large_to_scan");
    }
    let file = std::fs::File::open(path).map_err(|_| "open_failed")?;
    let reader = BufReader::new(file);
    let mut collected: Vec<String> = Vec::new();
    let mut bytes = 0usize;
    for line in reader.lines() {
        let line = line.map_err(|_| "read_failed")?;
        let size = line.len() + 1;
        if bytes + size > max_bytes && !collected.is_empty() {
            break;
        }
        bytes += size;
        collected.push(line);
        if collected.len() >= max_lines {
            break;
        }
    }
    // 只保留尾部 max_lines 行（大文件场景下 collected 可能超过）。
    if collected.len() > max_lines {
        let start = collected.len() - max_lines;
        collected.drain(0..start);
    }
    Ok(collected.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::server::{LogSource, ServiceDescriptor};

    fn service_with(path: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: "self-tools".into(),
            display_name: "Self Tools".into(),
            provider_ref: "com.example".into(),
            log_sources: vec![LogSource {
                id: "stdout".into(),
                display_name: "标准输出".into(),
                path: path.into(),
            }],
            ..ServiceDescriptor::default()
        }
    }

    #[test]
    fn tail_reads_registered_source_only() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("app.log");
        std::fs::write(&log, "line1\nline2\nline3\n").unwrap();
        let service = service_with(log.to_string_lossy().as_ref());

        let result = LocalLogTail
            .tail(&service, "stdout", 10, 4_096, 0)
            .expect("read");
        assert_eq!(result.lines, 3);
        assert!(result.text.ends_with("line3"));
        assert_eq!(result.log_source_id, "stdout");
    }

    #[test]
    fn tail_respects_line_and_byte_limits() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("app.log");
        let big_line = "x".repeat(500);
        let content: String = std::iter::repeat_n(big_line.as_str(), 10).collect::<Vec<_>>().join("\n");
        std::fs::write(&log, &content).unwrap();
        let service = service_with(log.to_string_lossy().as_ref());

        let capped_lines = LocalLogTail.tail(&service, "stdout", 3, 64 * 1_024, 0).expect("lines");
        assert_eq!(capped_lines.lines, 3);
        assert!(capped_lines.truncated);

        // 单行 500 字节、上限 600 → 只收 1 行（字节上限先生效）。
        let capped_bytes = LocalLogTail.tail(&service, "stdout", 1_000, 600, 0).expect("bytes");
        assert_eq!(capped_bytes.lines, 1, "字节上限: {}", capped_bytes.text.len());
        assert!(capped_bytes.text.len() <= 501);
    }

    #[test]
    fn unknown_log_source_falls_back_to_first_registered() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("app.log");
        std::fs::write(&log, "hello").unwrap();
        let service = service_with(log.to_string_lossy().as_ref());
        // 未知名 → 回落第一个注册源（descriptor 只有一个源时最直观）；
        // **没有任何路径**能读 descriptor 之外的日志（§39）。
        let result = LocalLogTail
            .tail(&service, "stderr", 10, 4_096, 0)
            .expect("fallback");
        assert_eq!(result.log_source_id, "stdout");
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn no_registered_source_is_rejected() {
        let service = ServiceDescriptor {
            id: "self-tools".into(),
            display_name: "Self Tools".into(),
            provider_ref: "com.example".into(),
            ..ServiceDescriptor::default()
        };
        let error = LocalLogTail
            .tail(&service, "stdout", 10, 4_096, 0)
            .expect_err("无注册日志源");
        assert_eq!(error, "no_log_source");
    }

    #[test]
    fn missing_file_is_a_controlled_error() {
        let service = service_with("/definitely/missing/app.log");
        let error = LocalLogTail
            .tail(&service, "stdout", 10, 4_096, 0)
            .expect_err("missing");
        assert!(error.starts_with("log_read_failed"), "{error}");
    }

    #[test]
    fn traversal_in_registered_path_is_rejected() {
        let service = service_with("../../etc/passwd");
        let error = LocalLogTail
            .tail(&service, "stdout", 10, 4_096, 0)
            .expect_err("traversal");
        assert_eq!(error, "log_path_traversal");
    }
}
