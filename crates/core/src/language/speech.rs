//! 语音抽象（V11 §117）：`SpeechProvider` 端口 + 定性反馈（§118）。
//!
//! **铁律（§118）**：没有可靠 scoring 时禁止输出 `95 / 100` 这类伪精确分数；
//! 只提供定性反馈（发音清晰度描述 + 具体改进点）。
//!
//! 端口只定义了「合成 / 识别」能力；具体厂商（本地 TTS / Web Speech /
//! OpenAI-compatible audio）由 infrastructure adapter 实现，core/application
//! 不绑死任何厂商。

use serde::{Deserialize, Serialize};

/// 合成请求（文本 → 音频）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechSynthesisRequest {
    /// 目标文本（词 / 短语 / 句子）。
    pub text: String,
    /// 语言码（如 `ja` / `en` / `zh`）；空 = provider 默认。
    pub language: Option<String>,
    /// 语速（0.5..2.0，1.0 = 正常）。
    pub rate: Option<f32>,
}

/// 合成结果。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpeechSynthesis {
    /// audio bytes（MIME 由 `mime` 说明）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<u8>,
    /// MIME（如 `audio/mpeg` / `audio/wav`）。
    pub mime: String,
    /// 是否来自本地确定性合成（无音频数据时前端可用 Web Speech 兜底）。
    pub local_only: bool,
}

/// 识别请求（音频 → 文本；可选参考文本用于对照）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechRecognitionRequest {
    /// 音频 bytes。
    pub audio: Vec<u8>,
    pub mime: String,
    /// 语言码。
    pub language: Option<String>,
    /// 参考文本（目标句）；有它才能给定性反馈。
    pub reference_text: Option<String>,
}

/// 识别结果。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpeechRecognition {
    /// 转写文本（ASR 输出）。
    pub transcript: String,
    /// 置信度（0..1；未知 = None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

/// 定性发音反馈（§118：**禁止**伪精确分数）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PronunciationFeedback {
    /// 总体定性评价（如「接近目标」「有明显偏差」）。
    pub overall: QualitativeLevel,
    /// 与参考文本逐词对照后的差异（复用 language::speaking 的 WordDiff 结构语义）。
    #[serde(default)]
    pub missing_words: Vec<String>,
    #[serde(default)]
    pub wrong_words: Vec<String>,
    #[serde(default)]
    pub extra_words: Vec<String>,
    /// 具体改进建议（短句列表；无数据 = 空）。
    #[serde(default)]
    pub suggestions: Vec<String>,
}

/// 定性档位（不映射到任何百分制）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualitativeLevel {
    /// 转写与参考完全一致。
    Excellent,
    /// 少量偏差。
    Good,
    /// 明显偏差但主体可辨。
    Fair,
    /// 差异过大 / 无转写。
    #[default]
    Unclear,
}

impl QualitativeLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            QualitativeLevel::Excellent => "excellent",
            QualitativeLevel::Good => "good",
            QualitativeLevel::Fair => "fair",
            QualitativeLevel::Unclear => "unclear",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            QualitativeLevel::Excellent => "与目标一致",
            QualitativeLevel::Good => "接近目标",
            QualitativeLevel::Fair => "有明显偏差",
            QualitativeLevel::Unclear => "无法判断",
        }
    }

    /// 由词级 diff 推导档位（确定性；阈值集中在这一点）。
    #[must_use]
    pub fn from_word_diff(
        target_words: usize,
        missing: usize,
        wrong: usize,
        extra: usize,
    ) -> Self {
        if target_words == 0 {
            return QualitativeLevel::Unclear;
        }
        let deviation = (missing + wrong + extra) as f32 / target_words as f32;
        if deviation <= 0.0 {
            QualitativeLevel::Excellent
        } else if deviation <= 0.15 {
            QualitativeLevel::Good
        } else if deviation <= 0.5 {
            QualitativeLevel::Fair
        } else {
            QualitativeLevel::Unclear
        }
    }
}

/// 语音端口（TTS + ASR + 定性反馈）。
///
/// 实现方必须：
/// - 自带超时（不要把调用方挂起）；
/// - 在能力不足时返回受控错误（`SpeechError`），不伪造转写。
pub trait SpeechProvider: Send + Sync {
    /// 提供方标识（`web-speech` / `openai-compatible-audio` / `local-tts`…）。
    fn name(&self) -> &'static str;

    /// 是否可合成。
    fn can_synthesize(&self) -> bool;

    /// 是否可识别。
    fn can_recognize(&self) -> bool;

    /// 文本 → 音频。
    fn synthesize(
        &self,
        request: &SpeechSynthesisRequest,
    ) -> Result<SpeechSynthesis, SpeechError>;

    /// 音频 → 文本。
    fn recognize(&self, request: &SpeechRecognitionRequest) -> Result<SpeechRecognition, SpeechError>;
}

/// 语音错误（稳定 kind + 面向用户文本）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechError {
    pub kind: SpeechErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeechErrorKind {
    /// 未配置 / 不支持。
    Unsupported,
    /// 超时。
    Timeout,
    /// 传输错误。
    Transport,
    /// 无法识别。
    NotRecognized,
}

impl SpeechError {
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: SpeechErrorKind::Unsupported,
            message: message.into(),
        }
    }
    #[must_use]
    pub fn not_recognized(message: impl Into<String>) -> Self {
        Self {
            kind: SpeechErrorKind::NotRecognized,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SpeechError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for SpeechError {}

/// 由转写 + 参考文本生成**定性**反馈（§116/§118 的最低闭环）。
///
/// 复用 `crate::language::speaking` 的词级 diff（冻结实现），本函数只做
/// 档位映射与建议生成；**不**产出任何数值分数。
#[must_use]
pub fn qualitative_feedback(recognition: &SpeechRecognition, reference: &str) -> PronunciationFeedback {
    use crate::language::speaking::{compare_words, tokenize};
    let target = tokenize(reference);
    let spoken = tokenize(&recognition.transcript);
    if spoken.is_empty() {
        return PronunciationFeedback {
            overall: QualitativeLevel::Unclear,
            suggestions: vec!["没有收到可识别的语音，请再试一次或靠近麦克风。".into()],
            ..PronunciationFeedback::default()
        };
    }
    let diff = compare_words(&target, &spoken);
    let overall = QualitativeLevel::from_word_diff(target.len(), diff.missing.len(), diff.wrong.len(), diff.extra.len());
    let mut suggestions = Vec::new();
    if !diff.missing.is_empty() {
        suggestions.push(format!("漏读了：{}", diff.missing.join("、")));
    }
    if !diff.wrong.is_empty() {
        suggestions.push(format!("读错了：{}", diff.wrong.join("、")));
    }
    if !diff.extra.is_empty() {
        suggestions.push(format!("多读了：{}", diff.extra.join("、")));
    }
    if suggestions.is_empty() {
        suggestions.push("与目标一致，可以加快速度或换更长的句子练习。".into());
    }
    PronunciationFeedback {
        overall,
        missing_words: diff.missing,
        wrong_words: diff.wrong,
        extra_words: diff.extra,
        suggestions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recognition(transcript: &str) -> SpeechRecognition {
        SpeechRecognition {
            transcript: transcript.into(),
            confidence: Some(0.9),
        }
    }

    #[test]
    fn qualitative_levels_are_centralized_and_non_numeric() {
        assert_eq!(QualitativeLevel::from_word_diff(10, 0, 0, 0), QualitativeLevel::Excellent);
        assert_eq!(QualitativeLevel::from_word_diff(10, 1, 0, 0), QualitativeLevel::Good);
        assert_eq!(QualitativeLevel::from_word_diff(10, 2, 2, 1), QualitativeLevel::Fair);
        assert_eq!(QualitativeLevel::from_word_diff(10, 6, 0, 0), QualitativeLevel::Unclear);
        assert_eq!(QualitativeLevel::from_word_diff(0, 0, 0, 0), QualitativeLevel::Unclear);
        // 无目标 → Unclear。
        assert_eq!(QualitativeLevel::label(QualitativeLevel::Good), "接近目标");
    }

    #[test]
    fn perfect_match_gives_excellent_with_positive_suggestion() {
        let feedback = qualitative_feedback(&recognition("good morning everyone"), "good morning everyone");
        assert_eq!(feedback.overall, QualitativeLevel::Excellent);
        assert!(feedback.missing_words.is_empty());
        assert!(feedback.wrong_words.is_empty());
        assert_eq!(feedback.suggestions.len(), 1);
    }

    #[test]
    fn deviations_are_reported_qualitatively_only() {
        let feedback = qualitative_feedback(
            &recognition("good morning"),
            "good evening everyone",
        );
        assert_ne!(feedback.overall, QualitativeLevel::Excellent);
        assert!(!feedback.missing_words.is_empty() || !feedback.wrong_words.is_empty());
        assert!(!feedback.suggestions.is_empty());
        // §118：序列化形状里不得出现任何 0..100 的分数字段。
        let json = serde_json::to_string(&feedback).expect("serialize");
        assert!(!json.contains("score"), "{json}");
        assert!(!json.contains("95"), "{json}");
    }

    #[test]
    fn empty_transcript_is_unclear_with_actionable_hint() {
        let feedback = qualitative_feedback(&recognition("   "), "hello world");
        assert_eq!(feedback.overall, QualitativeLevel::Unclear);
        assert!(feedback.suggestions.iter().any(|text| text.contains("麦克风")));
    }

    #[test]
    fn cjk_reference_words_are_compared_as_tokens() {
        let feedback = qualitative_feedback(&recognition("你好 世界"), "你好 世界");
        assert_eq!(feedback.overall, QualitativeLevel::Excellent);
    }

    #[test]
    fn synthesis_request_round_trips() {
        let request = SpeechSynthesisRequest {
            text: "こんにちは".into(),
            language: Some("ja".into()),
            rate: Some(0.8),
        };
        let json = serde_json::to_value(&request).expect("serialize");
        assert_eq!(json["text"], "こんにちは");
        let back: SpeechSynthesisRequest = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, request);
    }

    #[test]
    fn speech_error_display_is_stable() {
        let error = SpeechError::unsupported("provider 未配置");
        assert_eq!(error.kind, SpeechErrorKind::Unsupported);
        assert!(error.to_string().contains("provider 未配置"));
    }
}
