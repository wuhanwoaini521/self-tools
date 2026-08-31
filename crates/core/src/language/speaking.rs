//! 口语评分（#67/#68）。
//!
//! V1 只做 Accuracy / Completeness / Fluency，不做口音分。全部纯函数：
//! - Accuracy：单词级匹配占比（Missing / Wrong / Extra 加权惩罚）；
//! - Completeness：目标词被覆盖的比例；
//! - Fluency：时长比 + 长停顿惩罚。
//!
//! 转写来源可为用户手动输入或前端 Web Speech（可选），Rust 只管打分。

use serde::{Deserialize, Serialize};

/// 单词级比较结果（#68）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WordDiff {
    pub missing: Vec<String>,
    pub wrong: Vec<String>,
    pub extra: Vec<String>,
}

/// 转写词表（去除标点，按空白分词；内部统一小写比较）。
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric() && ch != '\'' && ch != '-' && !is_cjk(ch))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_cjk(ch: char) -> bool {
    // 汉字/假名；不包含 CJK 标点（。、「」等），保证分词正确
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3040..=0x30FF | 0x31F0..=0x31FF)
}

/// 比较目标词与用户词序列：从左到右做简易对齐（动态规划 LCS）。
#[must_use]
pub fn compare_words(target: &[String], spoken: &[String]) -> WordDiff {
    let mut missing = Vec::new();
    let mut wrong = Vec::new();
    let mut extra = Vec::new();
    let mut ti = 0;
    let mut si = 0;
    while ti < target.len() && si < spoken.len() {
        if target[ti] == spoken[si] {
            ti += 1;
            si += 1;
            continue;
        }
        // 向前看：目标里的词是否在后面出现（漏说/插词）
        let target_ahead = target[ti..].iter().position(|w| w == &spoken[si]);
        let spoken_ahead = spoken[si..].iter().position(|w| w == &target[ti]);
        match (target_ahead, spoken_ahead) {
            (Some(0), _) => {
                ti += 1;
                si += 1;
            }
            (Some(_), Some(0)) => {
                ti += 1;
                si += 1;
            }
            (Some(ta), Some(sa)) if ta <= sa => {
                for w in &target[ti..ti + ta] {
                    missing.push(w.clone());
                }
                ti += ta;
            }
            _ => {
                for w in &spoken[si..si + spoken_ahead.unwrap_or(1)] {
                    extra.push(w.clone());
                }
                si += spoken_ahead.unwrap_or(1);
            }
        }
    }
    for w in &target[ti..] {
        missing.push(w.clone());
    }
    for w in &spoken[si..] {
        extra.push(w.clone());
    }
    wrong.extend(
        missing
            .iter()
            .filter(|&word| spoken.iter().any(|s| same_base(word, s)))
            .cloned(),
    );
    WordDiff {
        missing,
        wrong: wrong.clone(),
        extra,
    }
}

/// 词干近似判断（tone 数字/曲折变化忽略）——用于把接近词归为 wrong 而非 missing。
fn same_base(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim_end_matches(|ch: char| ch.is_ascii_digit())
            .to_string()
    };
    let (na, nb) = (norm(a), norm(b));
    na == nb || na.starts_with(nb.as_str()) || nb.starts_with(na.as_str())
}

/// 评分结果（0–100）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeakingScore {
    pub accuracy: u8,
    pub completeness: u8,
    pub fluency: u8,
}

/// 计算三个分项。
/// `duration_ms`：用户录音时长；`target_ms`：目标句参考时长（TTS 播放时长或估算）。
/// `long_pauses_ms` 数组：大于阈值的停顿时长（前端用 AnalyserNode 能量检测）。
#[must_use]
pub fn score(
    target: &str,
    transcript: &str,
    duration_ms: u64,
    target_ms: u64,
    long_pauses_ms: &[u64],
) -> SpeakingScore {
    let target_words = tokenize(target);
    let spoken_words = tokenize(transcript);
    if target_words.is_empty() {
        return SpeakingScore {
            accuracy: 0,
            completeness: 0,
            fluency: 0,
        };
    }
    let diff = compare_words(&target_words, &spoken_words);
    // Accuracy：缺陷词（miss/wrong/extra，去重）占总目标词比例
    let mut flawed: Vec<String> = Vec::new();
    flawed.extend(diff.missing.iter().cloned());
    flawed.extend(diff.wrong.iter().cloned());
    flawed.extend(diff.extra.iter().cloned());
    flawed.sort();
    flawed.dedup();
    let accuracy = if flawed.is_empty() {
        100
    } else {
        let penalty = (flawed.len() as f64 / target_words.len() as f64).min(1.0);
        ((1.0 - penalty) * 100.0).round() as u8
    };
    // Completeness：目标词中被覆盖的比例（missing 中真正属于目标的去重词数）
    let mut missing_in_target: Vec<String> = diff
        .missing
        .iter()
        .filter(|word| target_words.contains(word))
        .cloned()
        .collect();
    missing_in_target.sort();
    missing_in_target.dedup();
    let covered = target_words.len() - missing_in_target.len();
    let completeness = ((covered as f64 / target_words.len() as f64) * 100.0).round() as u8;
    // Fluency：时长比（接近预期最佳） + 长停顿惩罚
    let duration_ratio = if target_ms > 0 {
        (duration_ms as f64 / target_ms as f64).clamp(0.0, 2.5)
    } else {
        0.0
    };
    let pause_penalty: u64 = long_pauses_ms.iter().map(|pause| pause / 1000).sum();
    let mut fluency = 100.0;
    if duration_ms > 0 {
        // 100 - |ratio-1|*40；耗时越接近参考越好
        fluency -= (duration_ratio - 1.0).abs() * 40.0;
    }
    fluency = (fluency - pause_penalty as f64 * 5.0).clamp(0.0, 100.0);
    SpeakingScore {
        accuracy,
        completeness,
        fluency: fluency.round() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_transcript_scores_high() {
        let score = score(
            "I would like to make a reservation",
            "i would like to make a reservation",
            4_000,
            4_000,
            &[],
        );
        assert_eq!(score.accuracy, 100);
        assert_eq!(score.completeness, 100);
        assert!(score.fluency >= 95, "fluency was {score:?}");
    }

    #[test]
    fn missing_words_lower_scores() {
        let score = score(
            "I would like to make a reservation",
            "I would like a reservation",
            3_000,
            4_000,
            &[],
        );
        assert!(score.accuracy < 100);
        assert!(score.completeness < 100);
    }

    #[test]
    fn tokenize_splits_punctuation_but_keeps_cjk() {
        assert_eq!(tokenize("Hello, world!"), vec!["hello", "world"]);
        assert_eq!(tokenize("食咗飯未呀？"), vec!["食咗飯未呀"]);
        assert_eq!(tokenize("ご飯を食べる。"), vec!["ご飯を食べる"]);
    }

    #[test]
    fn long_pauses_punish_fluency() {
        let good = score("A", "a", 1_000, 1_000, &[]);
        let pausing = score("A", "a", 1_000, 1_000, &[3_000, 2_000]);
        assert!(pausing.fluency < good.fluency);
    }

    #[test]
    fn words_quoted_are_kept() {
        assert_eq!(tokenize("it's a don't"), vec!["it's", "a", "don't"]);
    }
}
