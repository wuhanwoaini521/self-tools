//! LRC 逐句时间轴解析（纯函数，无 I/O）。
//!
//! 支持 NCE 数据的两种真实形态（见 `docs/language/NCE_IMPORT.md`）：
//! - `[mm:ss.xx]english sentence|中文翻译`（双栏）
//! - `[mm:ss.xx]english sentence`（仅英文）
//!
//! 规则：
//! - 元数据标签（`[ti:]` / `[ar:]` / `[al:]` / `[by:]` / `[offset:]` …）跳过；
//! - 一行可有多个时间戳（`[t1][t2]text`）→ 每个时间戳各生成一条；
//! - `end_ms = 下一句 start_ms`；最后一句 `end_ms = start_ms + 估算时长`；
//! - 竖线 `|` 分隔英中；无竖线时 `chinese = None`（诚实，不编造翻译）；
//! - 畸形行跳过并计入 `issues`，不让一行坏数据毁掉整课。

use serde::{Deserialize, Serialize};

/// 一条带时间轴的双语句子（解析中间态，落库时补 id / lesson_id / sequence）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimedLine {
    pub start_ms: i64,
    pub end_ms: i64,
    pub english: String,
    pub chinese: Option<String>,
}

/// LRC 解析结果。
#[derive(Clone, Debug, Default)]
pub struct LrcParse {
    pub lines: Vec<TimedLine>,
    /// 人类可读的跳过原因（导入报告用）。
    pub issues: Vec<String>,
}

/// 估算一句的朗读时长（最后一句没有「下一句开头」可推）：按词数估，下限 1.5s。
#[must_use]
pub fn estimate_duration_ms(english: &str) -> i64 {
    let words = english.split_whitespace().count() as i64;
    (words.max(1) * 450 + 800).clamp(1500, 15_000)
}

/// 解析 `[mm:ss.xx]` / `[mm:ss.xxx]` 时间戳；返回毫秒与消费掉的字符数。
fn parse_timestamp(raw: &str) -> Option<(i64, usize)> {
    let close = raw.find(']')?;
    let inner = &raw[1..close];
    let (minutes, rest) = inner.split_once(':')?;
    let minutes: i64 = minutes.trim().parse().ok()?;
    // 秒与百分/毫秒部分：`02.71` / `02.710`。
    let mut seconds_parts = rest.splitn(2, '.');
    let seconds: i64 = seconds_parts.next()?.trim().parse().ok()?;
    let fraction = seconds_parts.next().unwrap_or("0").trim();
    let millis: i64 = match fraction.len() {
        1 => fraction.parse::<i64>().ok()? * 100,
        2 => fraction.parse::<i64>().ok()? * 10,
        _ => fraction[..3.min(fraction.len())].parse().ok()?,
    };
    Some((minutes * 60_000 + seconds * 1_000 + millis, close + 1))
}

/// 元数据标签（`ti`/`ar`/`al`/`by`/`offset`/`re`/`ve`/`length`…）：字母键 + 冒号。
fn is_metadata(inner: &str) -> bool {
    let Some((key, _)) = inner.split_once(':') else {
        return false;
    };
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic()) && key.parse::<i64>().is_err()
}

/// 解析一份 LRC 文本。
#[must_use]
pub fn parse_lrc(text: &str) -> LrcParse {
    let mut parsed: Vec<(i64, String, Option<String>)> = Vec::new();
    let mut issues = Vec::new();

    for (line_no, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('[') {
            issues.push(format!("line {}: no timestamp", line_no + 1));
            continue;
        }

        // 收集本行全部时间戳。
        let mut stamps = Vec::new();
        let mut rest = line;
        while rest.starts_with('[') {
            let Some(close) = rest.find(']') else { break };
            let inner = &rest[1..close];
            if is_metadata(inner) {
                rest = &rest[close + 1..];
                continue;
            }
            match parse_timestamp(rest) {
                Some((ms, consumed)) => {
                    stamps.push(ms);
                    rest = &rest[consumed..];
                }
                None => break,
            }
        }
        if stamps.is_empty() {
            continue; // 纯元数据行
        }

        let content = rest.trim();
        if content.is_empty() {
            continue;
        }
        let (english, chinese) = match content.split_once('|') {
            Some((en, zh)) => {
                let zh = zh.trim();
                (
                    en.trim().to_string(),
                    (!zh.is_empty()).then(|| zh.to_string()),
                )
            }
            None => (content.to_string(), None),
        };
        if english.is_empty() {
            issues.push(format!("line {}: empty english", line_no + 1));
            continue;
        }
        for ms in stamps {
            parsed.push((ms, english.clone(), chinese.clone()));
        }
    }

    parsed.sort_by_key(|(ms, _, _)| *ms);
    let mut lines = Vec::with_capacity(parsed.len());
    for (index, (start, english, chinese)) in parsed.iter().enumerate() {
        let next_start = parsed.get(index + 1).map(|(ms, _, _)| *ms);
        let end = match next_start {
            Some(ms) if ms > *start => ms,
            _ => start + estimate_duration_ms(english),
        };
        lines.push(TimedLine {
            start_ms: *start,
            end_ms: end,
            english: english.clone(),
            chinese: chinese.clone(),
        });
    }
    LrcParse { lines, issues }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nce_flow_bilingual_format() {
        let lrc = "[al:新概念英语（一）]\n[ti:Excuse Me!]\n\
                   [00:00.61]Lesson 1|第1课\n[00:02.71]Excuse me!|打扰一下！\n[00:05.61]Yes?|是的？\n";
        let parsed = parse_lrc(lrc);
        assert_eq!(parsed.lines.len(), 3);
        assert_eq!(parsed.lines[0].start_ms, 610);
        assert_eq!(parsed.lines[0].end_ms, 2710);
        assert_eq!(parsed.lines[0].english, "Lesson 1");
        assert_eq!(parsed.lines[0].chinese.as_deref(), Some("第1课"));
        assert_eq!(parsed.lines[1].english, "Excuse me!");
        assert_eq!(parsed.lines[1].end_ms, 5610);
    }

    #[test]
    fn parses_english_only_lines() {
        let parsed = parse_lrc("[00:01.00]Hello world.\n[00:04.00]Goodbye.\n");
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].chinese, None);
        assert_eq!(parsed.lines[0].end_ms, 4000);
    }

    #[test]
    fn last_line_gets_estimated_end() {
        let parsed = parse_lrc("[00:10.00]This is the last line of the lesson.\n");
        assert_eq!(parsed.lines[0].start_ms, 10_000);
        assert!(parsed.lines[0].end_ms > 10_000);
        assert!(parsed.lines[0].end_ms <= 25_000);
    }

    #[test]
    fn multi_timestamp_lines_expand() {
        let parsed = parse_lrc("[00:01.00][00:05.00]Repeated chorus\n");
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].start_ms, 1000);
        assert_eq!(parsed.lines[1].start_ms, 5000);
    }

    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let parsed = parse_lrc("garbage line\n[00:01.00]Good line.\n[bad]nope\n");
        assert_eq!(parsed.lines.len(), 1);
        assert!(!parsed.issues.is_empty());
    }

    #[test]
    fn millis_precision_variants() {
        assert_eq!(parse_timestamp("[01:02.3]"), Some((62_300, 9)));
        assert_eq!(parse_timestamp("[01:02.34]"), Some((62_340, 10)));
        assert_eq!(parse_timestamp("[01:02.345]"), Some((62_345, 11)));
    }

    #[test]
    fn metadata_is_recognized() {
        assert!(is_metadata("ti:Excuse Me!"));
        assert!(is_metadata("offset:+200"));
        assert!(!is_metadata("00:12.34"));
    }
}
