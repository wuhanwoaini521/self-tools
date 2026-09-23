//! 日志脱敏（V7 §40）。
//!
//! 复用 V6 的 secret 检测（`core::memory::detect_secret`）+ 日志特有形态
//! （authorization header / bearer / cookie / 连接串）。
//! 脱敏在 application 完成：基础设施只负责有界读取，**不**判断内容。

use std::sync::LazyLock;

use devtoolbox_core::server::LogReadResult;
use regex::Regex;

/// 脱敏统计（不含任何内容）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RedactionStats {
    pub hits: usize,
}

/// 日志脱敏器（无状态，可共享）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LogRedactor;

static AUTHORIZATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(authorization\s*[:=]\s*)(bearer\s+)?[A-Za-z0-9._\-]+")
        .expect("authorization regex")
});
static COOKIE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(cookie\s*[:=]\s*)[^\r\n]+").expect("cookie regex"));
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._\-]{12,}").expect("bearer regex"));
static CONNECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp|mssql)://[^\s/:@]+:[^\s/@]+@",
    )
    .expect("connection string regex")
});
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b((?:api[_-]?key|access[_-]?token|refresh[_-]?token|secret|password|passwd|token)\s*[:=]\s*)([^\s,;'"]{4,})"#,
    )
    .expect("key value regex")
});

/// 脱敏占位符（同时作为「本行已脱敏」的标记）。
const REDACTION_PLACEHOLDER: &str = "[REDACTED]";

impl LogRedactor {
    /// 脱敏单行；返回（脱敏后文本, 命中次数）。
    #[must_use]
    pub fn redact_line(line: &str) -> (String, usize) {
        let mut text = line.to_string();
        let mut hits = 0usize;

        // 1) 结构化字段优先（保留键名、只替换值）——日志可读性优先。
        for (pattern, keep_group) in [
            (&*AUTHORIZATION, true),
            (&*COOKIE, true),
            (&*KEY_VALUE, true),
        ] {
            let replaced = if keep_group {
                pattern.replace_all(&text, "$1[REDACTED]")
            } else {
                pattern.replace_all(&text, "[REDACTED]")
            };
            if replaced != text {
                hits += 1;
                text = replaced.into_owned();
            }
        }
        // 2) 裸 bearer / 连接串（无键名）。
        for pattern in [&*BEARER, &*CONNECTION] {
            let replaced = pattern.replace_all(&text, "[REDACTED]");
            if replaced != text {
                hits += 1;
                text = replaced.into_owned();
            }
        }
        // 3) V6 secret 门兜底：对**尚未被占位符覆盖的片段**复检。
        //    不能因「行内出现过占位符」就跳过整行 —— 那会让同一行里
        //    第二个 secret（第一个已被结构化替换）原样泄漏（审查 V7-SEC-003）。
        for segment in text.split(REDACTION_PLACEHOLDER) {
            if let Some(kind) = devtoolbox_core::memory::detect_secret(segment) {
                hits += 1;
                return (format!("[REDACTED:{}]", secret_label(kind)), hits);
            }
        }
        (text, hits)
    }

    /// 脱敏整个读取结果（逐行；保留行结构）。
    #[must_use]
    pub fn redact(result: LogReadResult) -> LogReadResult {
        let mut redactions = 0usize;
        let lines: Vec<String> = result
            .text
            .lines()
            .map(|line| {
                let (redacted, hits) = Self::redact_line(line);
                redactions += hits;
                redacted
            })
            .collect();
        LogReadResult {
            text: lines.join("\n"),
            redactions: result.redactions + redactions,
            ..result
        }
    }
}

/// V6 secret 类别标签（用于脱敏占位符；只放类别，不放内容）。
fn secret_label(kind: devtoolbox_core::memory::SecretKind) -> &'static str {
    use devtoolbox_core::memory::SecretKind;
    match kind {
        SecretKind::ApiKey => "api_key",
        SecretKind::Token => "token",
        SecretKind::Password => "password",
        SecretKind::PrivateKey => "private_key",
        SecretKind::CredentialPath => "credential_path",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_fields_keep_key_and_hide_value() {
        let (text, hits) = LogRedactor::redact_line("Authorization: Bearer abcdef1234567890xyz");
        assert!(text.contains("Authorization:"), "键名保留: {text}");
        assert!(!text.contains("abcdef1234567890xyz"), "值必须移除: {text}");
        assert!(hits >= 1);

        let (text, _) = LogRedactor::redact_line("api_key=sk-abcdefghijklmnopqrstuvwx");
        assert!(text.contains("api_key"), "{text}");
        assert!(!text.contains("sk-abcdefghijklmnopqrstuvwx"), "{text}");
    }

    #[test]
    fn cookies_and_connection_strings_are_hidden() {
        let (text, _) = LogRedactor::redact_line("Set-Cookie: session=verysecretvalue");
        assert!(!text.contains("verysecretvalue"), "{text}");

        let (text, _) = LogRedactor::redact_line("db=postgres://app:s3cret@db.internal:5432/prod");
        assert!(!text.contains("s3cret"), "{text}");
    }

    #[test]
    fn v6_secret_patterns_fold_whole_line() {
        let (text, hits) = LogRedactor::redact_line("-----BEGIN RSA PRIVATE KEY-----");
        assert!(text.starts_with("[REDACTED:"), "{text}");
        assert_eq!(hits, 1);
    }

    #[test]
    fn normal_log_lines_are_untouched() {
        let line = "2026-09-21T10:00:00Z INFO request served path=/api/v1/history status=200";
        let (text, hits) = LogRedactor::redact_line(line);
        assert_eq!(text, line);
        assert_eq!(hits, 0);
    }

    #[test]
    fn second_secret_on_same_line_is_still_caught() {
        // V7-SEC-003：结构化替换打过第一个字段后，同行第二个 secret 必须仍被检测。
        let (text, hits) = LogRedactor::redact_line(
            "api_key=sk-abcdefghijklmnopqrstuvwx Authorization: Bearer abcdef1234567890xyz",
        );
        assert!(!text.contains("sk-abcdefghijklmnopqrstuvwx"), "{text}");
        assert!(!text.contains("abcdef1234567890xyz"), "{text}");
        assert!(hits >= 1);
    }

    #[test]
    fn redact_result_counts_and_preserves_structure() {
        let result = LogReadResult {
            service_id: "self-tools".into(),
            log_source_id: "stdout".into(),
            text: "line one\ntoken: abcdef1234567890\nline three".into(),
            lines: 3,
            redactions: 0,
            truncated: false,
        };
        let redacted = LogRedactor::redact(result);
        assert_eq!(redacted.lines, 3);
        assert!(redacted.redactions >= 1);
        assert!(redacted.text.starts_with("line one\n"));
        assert!(!redacted.text.contains("abcdef1234567890"));
    }
}
