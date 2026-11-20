//! 口语陪练会话状态机（Phase 1 文本回合）。
//!
//! 后端是权威状态；前端只镜像。非法转移返回错误，不默默纠正。

use serde::{Deserialize, Serialize};

use super::scenario::CefrLevel;

/// 会话生命周期。`Listening` = 等待用户开口或打字；`Transcribing` = 正在转写音频。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeakingSessionStatus {
    Idle,
    Listening,
    Transcribing,
    Thinking,
    Speaking,
    Paused,
    Interrupted,
    Failed,
    Completed,
}

impl SpeakingSessionStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listening => "listening",
            Self::Transcribing => "transcribing",
            Self::Thinking => "thinking",
            Self::Speaking => "speaking",
            Self::Paused => "paused",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::Completed => "completed",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "idle" => Some(Self::Idle),
            "listening" => Some(Self::Listening),
            "transcribing" => Some(Self::Transcribing),
            "thinking" => Some(Self::Thinking),
            "speaking" => Some(Self::Speaking),
            "paused" => Some(Self::Paused),
            "interrupted" => Some(Self::Interrupted),
            "failed" => Some(Self::Failed),
            "completed" => Some(Self::Completed),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// 驱动状态机的事件。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeakingSessionEvent {
    Start,
    UserTurn,
    AudioReceived,
    CoachReady,
    Pause,
    Resume,
    Fail,
    Retry,
    Complete,
    Interrupt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidSessionTransition {
    pub from: SpeakingSessionStatus,
    pub event: &'static str,
}

impl std::fmt::Display for InvalidSessionTransition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot apply {} while session is {}",
            self.event,
            self.from.as_str()
        )
    }
}

/// 纯函数转移。未知组合返回错误，由应用层映射为 400。
pub fn apply_event(
    status: SpeakingSessionStatus,
    event: SpeakingSessionEvent,
) -> Result<SpeakingSessionStatus, InvalidSessionTransition> {
    let next = match (status, event) {
        (SpeakingSessionStatus::Idle, SpeakingSessionEvent::Start) => {
            SpeakingSessionStatus::Thinking
        }
        (
            SpeakingSessionStatus::Listening
            | SpeakingSessionStatus::Failed
            | SpeakingSessionStatus::Transcribing,
            SpeakingSessionEvent::UserTurn,
        ) => SpeakingSessionStatus::Thinking,
        (
            SpeakingSessionStatus::Listening | SpeakingSessionStatus::Failed,
            SpeakingSessionEvent::AudioReceived,
        ) => SpeakingSessionStatus::Transcribing,
        (SpeakingSessionStatus::Thinking, SpeakingSessionEvent::CoachReady) => {
            SpeakingSessionStatus::Speaking
        }
        (SpeakingSessionStatus::Speaking, SpeakingSessionEvent::Start) => {
            SpeakingSessionStatus::Listening
        }
        (
            SpeakingSessionStatus::Listening | SpeakingSessionStatus::Speaking,
            SpeakingSessionEvent::Pause,
        ) => SpeakingSessionStatus::Paused,
        (SpeakingSessionStatus::Paused, SpeakingSessionEvent::Resume) => {
            SpeakingSessionStatus::Listening
        }
        (
            SpeakingSessionStatus::Thinking
            | SpeakingSessionStatus::Listening
            | SpeakingSessionStatus::Speaking
            | SpeakingSessionStatus::Transcribing,
            SpeakingSessionEvent::Fail,
        ) => SpeakingSessionStatus::Failed,
        (SpeakingSessionStatus::Failed, SpeakingSessionEvent::Retry) => {
            SpeakingSessionStatus::Listening
        }
        (
            SpeakingSessionStatus::Listening
            | SpeakingSessionStatus::Speaking
            | SpeakingSessionStatus::Paused
            | SpeakingSessionStatus::Failed,
            SpeakingSessionEvent::Complete,
        ) => SpeakingSessionStatus::Completed,
        (SpeakingSessionStatus::Speaking, SpeakingSessionEvent::Interrupt) => {
            SpeakingSessionStatus::Interrupted
        }
        (
            SpeakingSessionStatus::Interrupted | SpeakingSessionStatus::Transcribing,
            SpeakingSessionEvent::Resume,
        ) => SpeakingSessionStatus::Listening,
        _ => {
            return Err(InvalidSessionTransition {
                from: status,
                event: event_name(event),
            });
        }
    };
    Ok(next)
}

fn event_name(event: SpeakingSessionEvent) -> &'static str {
    match event {
        SpeakingSessionEvent::Start => "start",
        SpeakingSessionEvent::UserTurn => "userTurn",
        SpeakingSessionEvent::AudioReceived => "audioReceived",
        SpeakingSessionEvent::CoachReady => "coachReady",
        SpeakingSessionEvent::Pause => "pause",
        SpeakingSessionEvent::Resume => "resume",
        SpeakingSessionEvent::Fail => "fail",
        SpeakingSessionEvent::Retry => "retry",
        SpeakingSessionEvent::Complete => "complete",
        SpeakingSessionEvent::Interrupt => "interrupt",
    }
}

/// 回合角色。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeakingTurnRole {
    User,
    Coach,
}

impl SpeakingTurnRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Coach => "coach",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "coach" => Some(Self::Coach),
            _ => None,
        }
    }
}

/// 文本来源：键盘，或 ASR 转写。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeakingTurnSource {
    Typed,
    Asr,
}

impl SpeakingTurnSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Typed => "typed",
            Self::Asr => "asr",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "typed" => Some(Self::Typed),
            "asr" => Some(Self::Asr),
            _ => None,
        }
    }
}

/// 持久化会话。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingSession {
    pub id: String,
    pub scenario_id: String,
    pub status: SpeakingSessionStatus,
    pub cefr: CefrLevel,
    pub locale_assist: String,
    pub client_request_id: String,
    pub completed_goal_ids: Vec<String>,
    pub coach_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
}

/// 一轮对话。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingTurn {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: SpeakingTurnRole,
    pub text: String,
    pub assist_text: Option<String>,
    pub source: SpeakingTurnSource,
    pub client_request_id: Option<String>,
    #[serde(default)]
    pub corrections: Vec<SpeakingCorrection>,
    pub created_at: i64,
}

/// 结束报告。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingFeedback {
    pub session_id: String,
    pub summary: String,
    pub expressions: Vec<String>,
    pub errors: Vec<SpeakingCorrection>,
    #[serde(default)]
    pub grammar: Vec<SpeakingCorrection>,
    #[serde(default)]
    pub unnatural: Vec<SpeakingCorrection>,
    /// 音素级发音尚未接入；固定为 `unassessed`，禁止用 LLM 分数冒充。
    #[serde(default = "unassessed_pronunciation")]
    pub pronunciation_status: String,
    #[serde(default)]
    pub task_total: u32,
    #[serde(default)]
    pub drill_items: Vec<SpeakingDrillItem>,
    #[serde(default)]
    pub dict_matches: Vec<SpeakingDictMatch>,
    pub completed_goal_ids: Vec<String>,
    /// 下场对话应主动练的薄弱点（建议句、目标词、未完成任务）。
    #[serde(default)]
    pub next_focus: Vec<String>,
    pub created_at: i64,
}

fn unassessed_pronunciation() -> String {
    "unassessed".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingCorrection {
    pub span: String,
    pub category: String,
    pub suggestion: String,
    /// 为什么这样说更自然 / 更对。可空：脚本教练没有解释。
    #[serde(default)]
    pub explanation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingDrillItem {
    pub kind: String,
    pub prompt: String,
    pub prompt_zh: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingDictMatch {
    pub word: String,
    pub lemma: String,
    pub gloss: Option<String>,
    pub in_dictionary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_reaches_listening_then_complete() {
        let mut status = SpeakingSessionStatus::Idle;
        status = apply_event(status, SpeakingSessionEvent::Start).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Thinking);
        status = apply_event(status, SpeakingSessionEvent::CoachReady).unwrap();
        status = apply_event(status, SpeakingSessionEvent::Start).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Listening);
        status = apply_event(status, SpeakingSessionEvent::UserTurn).unwrap();
        status = apply_event(status, SpeakingSessionEvent::CoachReady).unwrap();
        status = apply_event(status, SpeakingSessionEvent::Complete).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Completed);
    }

    #[test]
    fn pause_from_completed_is_rejected() {
        let error = apply_event(
            SpeakingSessionStatus::Completed,
            SpeakingSessionEvent::Pause,
        )
        .unwrap_err();
        assert_eq!(error.from, SpeakingSessionStatus::Completed);
    }

    #[test]
    fn failed_session_can_retry_then_accept_a_turn() {
        let mut status = SpeakingSessionStatus::Failed;
        status = apply_event(status, SpeakingSessionEvent::Retry).unwrap();
        status = apply_event(status, SpeakingSessionEvent::UserTurn).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Thinking);
    }

    #[test]
    fn audio_turn_goes_through_transcribing() {
        let mut status = SpeakingSessionStatus::Listening;
        status = apply_event(status, SpeakingSessionEvent::AudioReceived).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Transcribing);
        status = apply_event(status, SpeakingSessionEvent::UserTurn).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Thinking);
    }

    #[test]
    fn abandoned_transcription_returns_to_listening() {
        let mut status = SpeakingSessionStatus::Listening;
        status = apply_event(status, SpeakingSessionEvent::AudioReceived).unwrap();
        status = apply_event(status, SpeakingSessionEvent::Resume).unwrap();
        assert_eq!(status, SpeakingSessionStatus::Listening);
    }
}
