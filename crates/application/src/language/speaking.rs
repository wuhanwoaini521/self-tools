//! 场景式口语陪练（Phase 2：文本 + 可选 HTTP 语音闭环）。
//!
//! LLM / ASR / TTS 都可选。未配置时走脚本教练、键盘输入、浏览器朗读，不假装有模型。

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};

use devtoolbox_core::language::{
    CefrLevel, SpeakingCorrection, SpeakingDictMatch, SpeakingDrillItem, SpeakingFeedback,
    SpeakingScenario, SpeakingSession, SpeakingSessionEvent, SpeakingSessionStatus, SpeakingTurn,
    SpeakingTurnRole, SpeakingTurnSource, all_goals_complete, apply_event, builtin_scenarios,
    completed_goals, scenario_by_id, scripted_reply_for,
};
use devtoolbox_core::learning::{
    LearningAction, LearningEvent, ReviewCardType, UniversalReviewCard,
};
use devtoolbox_core::personal_ai::{ChatMessage, ChatModelProvider, ChatRequest};
use devtoolbox_core::travel::extract_json;

use crate::error::ApplicationError;

/// 会话存储端口（application 不碰 SQLite）。
pub trait SpeakingPracticeStore: Send + Sync {
    fn upsert_scenario(&self, scenario: &SpeakingScenario) -> Result<(), String>;
    fn list_scenarios(&self) -> Result<Vec<SpeakingScenario>, String>;
    fn get_scenario(&self, id: &str) -> Result<Option<SpeakingScenario>, String>;
    fn insert_session(&self, session: &SpeakingSession) -> Result<(), String>;
    fn get_session_by_client_request(
        &self,
        client_request_id: &str,
    ) -> Result<Option<SpeakingSession>, String>;
    fn get_session(&self, id: &str) -> Result<Option<SpeakingSession>, String>;
    fn update_session(&self, session: &SpeakingSession) -> Result<(), String>;
    fn insert_turn(&self, turn: &SpeakingTurn) -> Result<(), String>;
    fn get_turn_by_client_request(
        &self,
        session_id: &str,
        client_request_id: &str,
    ) -> Result<Option<SpeakingTurn>, String>;
    fn list_turns(&self, session_id: &str) -> Result<Vec<SpeakingTurn>, String>;
    fn upsert_feedback(&self, feedback: &SpeakingFeedback) -> Result<(), String>;
    fn get_feedback(&self, session_id: &str) -> Result<Option<SpeakingFeedback>, String>;
    fn count_completed_sessions(&self) -> Result<u32, String>;
}

#[derive(Debug)]
pub enum SpeakingPracticeError {
    NotFound(String),
    Invalid(String),
    EmptyTurn,
    EmptyAudio,
    AudioDisabled,
    AsrUnavailable(String),
    Unavailable(String),
    Timeout(String),
    Storage(String),
}

impl std::fmt::Display for SpeakingPracticeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message)
            | Self::Invalid(message)
            | Self::Unavailable(message)
            | Self::AsrUnavailable(message)
            | Self::Timeout(message)
            | Self::Storage(message) => write!(formatter, "{message}"),
            Self::EmptyTurn => write!(formatter, "empty speaking turn"),
            Self::EmptyAudio => write!(formatter, "empty speaking audio"),
            Self::AudioDisabled => write!(formatter, "speaking audio is disabled"),
        }
    }
}

impl From<SpeakingPracticeError> for ApplicationError {
    fn from(error: SpeakingPracticeError) -> Self {
        ApplicationError::Language {
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub completed: Vec<String>,
    pub remaining: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachPayload {
    pub reply_text: String,
    pub assist_text: Option<String>,
    pub corrections: Vec<SpeakingCorrection>,
    pub target_words_used: Vec<String>,
    pub llm_used: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingSessionView {
    pub session: SpeakingSession,
    pub scenario: SpeakingScenario,
    pub turns: Vec<SpeakingTurn>,
    pub task_progress: TaskProgress,
    pub coach: Option<CoachPayload>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingAudioCapabilities {
    pub audio_enabled: bool,
    pub asr_configured: bool,
    pub tts_configured: bool,
    pub asr_provider: String,
    pub tts_provider: String,
    pub realtime_available: bool,
    pub barge_in_available: bool,
    pub phoneme_scoring: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsResult {
    pub audio_available: bool,
    pub provider: String,
    pub mime_type: Option<String>,
    pub audio_base64: Option<String>,
    pub text: String,
    pub turn_id: Option<String>,
}

/// 后端 ASR。未注入时客户端必须自带转写（浏览器 Web Speech）或改用键盘。
#[async_trait]
pub trait SpeechToTextPort: Send + Sync {
    fn name(&self) -> &'static str;
    async fn transcribe(
        &self,
        audio: &[u8],
        mime_type: &str,
    ) -> Result<String, SpeakingPracticeError>;
}

/// 后端 TTS。未注入时客户端用系统朗读，字幕仍以会话 turn 文本为准。
#[async_trait]
pub trait TextToSpeechPort: Send + Sync {
    fn name(&self) -> &'static str;
    async fn synthesize(&self, text: &str) -> Result<(String, Vec<u8>), SpeakingPracticeError>;
}

/// 词典对齐（目标词 → lemma/释义）。未注入时 `inDictionary=false`。
pub trait SpeakingDictionaryPort: Send + Sync {
    fn lookup_word(&self, word: &str) -> Result<Option<SpeakingDictMatch>, String>;
}

/// 只通过 Learning 端口写卡和事件，禁止口语模块自建 SRS。
pub trait SpeakingLearningPort: Send + Sync {
    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), String>;
    fn record_event(&self, event: &LearningEvent, now: i64) -> Result<(), String>;
    fn due_speaking_prompts(&self, now: i64, limit: usize) -> Result<Vec<String>, String>;
}

impl SpeakingLearningPort for crate::learning::LearningService {
    fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), String> {
        crate::learning::LearningService::upsert_review_card(self, card)
            .map_err(|error| error.to_string())
    }

    fn record_event(&self, event: &LearningEvent, now: i64) -> Result<(), String> {
        crate::learning::LearningService::record_event(self, event, now)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn due_speaking_prompts(&self, now: i64, limit: usize) -> Result<Vec<String>, String> {
        crate::learning::LearningService::get_review_queue(self, Some("language"), now, limit)
            .map(|items| {
                items
                    .into_iter()
                    .filter(|item| item.card.entity_type == "speaking_expression")
                    .map(|item| item.card.prompt)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewImportResult {
    pub cards_added: u32,
    pub card_ids: Vec<String>,
}

const MAX_AUDIO_BYTES: usize = 1_500_000;

pub struct SpeakingPracticeService {
    store: Arc<dyn SpeakingPracticeStore>,
    llm: Option<Arc<dyn ChatModelProvider>>,
    asr: Option<Arc<dyn SpeechToTextPort>>,
    tts: Option<Arc<dyn TextToSpeechPort>>,
    audio_enabled: bool,
    dictionary: Option<Arc<dyn SpeakingDictionaryPort>>,
    learning: Option<Arc<dyn SpeakingLearningPort>>,
}

impl SpeakingPracticeService {
    #[must_use]
    pub fn new(
        store: Arc<dyn SpeakingPracticeStore>,
        llm: Option<Arc<dyn ChatModelProvider>>,
    ) -> Self {
        Self {
            store,
            llm,
            asr: None,
            tts: None,
            audio_enabled: true,
            dictionary: None,
            learning: None,
        }
    }

    #[must_use]
    pub fn with_audio(
        mut self,
        audio_enabled: bool,
        asr: Option<Arc<dyn SpeechToTextPort>>,
        tts: Option<Arc<dyn TextToSpeechPort>>,
    ) -> Self {
        self.audio_enabled = audio_enabled;
        self.asr = asr;
        self.tts = tts;
        self
    }

    #[must_use]
    pub fn with_dictionary(mut self, dictionary: Arc<dyn SpeakingDictionaryPort>) -> Self {
        self.dictionary = Some(dictionary);
        self
    }

    #[must_use]
    pub fn with_learning(mut self, learning: Arc<dyn SpeakingLearningPort>) -> Self {
        self.learning = Some(learning);
        self
    }

    pub fn capabilities(&self) -> SpeakingAudioCapabilities {
        SpeakingAudioCapabilities {
            audio_enabled: self.audio_enabled,
            asr_configured: self.asr.is_some(),
            tts_configured: self.tts.is_some(),
            asr_provider: self
                .asr
                .as_ref()
                .map_or_else(|| "none".into(), |item| item.name().to_string()),
            tts_provider: self
                .tts
                .as_ref()
                .map_or_else(|| "none".into(), |item| item.name().to_string()),
            realtime_available: false,
            barge_in_available: true,
            phoneme_scoring: "unavailable".into(),
        }
    }

    /// 把内置场景写入库（幂等）。
    pub fn seed_scenarios(&self) -> Result<(), SpeakingPracticeError> {
        for scenario in builtin_scenarios() {
            self.store
                .upsert_scenario(&scenario)
                .map_err(SpeakingPracticeError::Storage)?;
        }
        Ok(())
    }

    pub fn list_scenarios(&self) -> Result<Vec<SpeakingScenario>, SpeakingPracticeError> {
        self.seed_scenarios()?;
        let mut items = self
            .store
            .list_scenarios()
            .map_err(SpeakingPracticeError::Storage)?;
        if items.is_empty() {
            items = builtin_scenarios();
        }
        Ok(items)
    }

    pub fn get_scenario(&self, id: &str) -> Result<SpeakingScenario, SpeakingPracticeError> {
        self.seed_scenarios()?;
        self.store
            .get_scenario(id)
            .map_err(SpeakingPracticeError::Storage)?
            .or_else(|| scenario_by_id(id))
            .ok_or_else(|| SpeakingPracticeError::NotFound(format!("scenario `{id}` not found")))
    }

    pub async fn create_session(
        &self,
        scenario_id: &str,
        cefr: Option<&str>,
        locale_assist: &str,
        client_request_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let request_id = client_request_id.trim();
        if request_id.is_empty() {
            return Err(SpeakingPracticeError::Invalid(
                "clientRequestId is required".into(),
            ));
        }
        if let Some(existing) = self
            .store
            .get_session_by_client_request(request_id)
            .map_err(SpeakingPracticeError::Storage)?
        {
            return self.view(&existing.id, None);
        }
        let scenario = self.get_scenario(scenario_id)?;
        let cefr = match cefr.map(str::trim).filter(|item| !item.is_empty()) {
            Some(raw) => CefrLevel::parse(raw).ok_or_else(|| {
                SpeakingPracticeError::Invalid(format!("unknown CEFR level `{raw}`"))
            })?,
            None => scenario.cefr,
        };
        let assist = if locale_assist.trim().eq_ignore_ascii_case("none") {
            "none"
        } else {
            "zh"
        };
        let session_id = format!("spk:{request_id}");
        let mut session = SpeakingSession {
            id: session_id.clone(),
            scenario_id: scenario.id.clone(),
            status: SpeakingSessionStatus::Idle,
            cefr,
            locale_assist: assist.to_string(),
            client_request_id: request_id.to_string(),
            completed_goal_ids: Vec::new(),
            coach_mode: if self.llm.is_some() {
                "llm".into()
            } else {
                "scripted".into()
            },
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        session.status = apply_event(session.status, SpeakingSessionEvent::Start)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        self.store
            .insert_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        let opening = SpeakingTurn {
            id: format!("{session_id}:t0"),
            session_id: session_id.clone(),
            seq: 0,
            role: SpeakingTurnRole::Coach,
            text: scenario.opening_line.clone(),
            assist_text: scenario.opening_line_zh.clone(),
            source: SpeakingTurnSource::Typed,
            client_request_id: Some(format!("{request_id}:open")),
            corrections: Vec::new(),
            created_at: now,
        };
        self.store
            .insert_turn(&opening)
            .map_err(SpeakingPracticeError::Storage)?;
        session.status = apply_event(session.status, SpeakingSessionEvent::CoachReady)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.status = apply_event(session.status, SpeakingSessionEvent::Start)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        self.view(
            &session.id,
            Some(CoachPayload {
                reply_text: opening.text,
                assist_text: opening.assist_text,
                corrections: Vec::new(),
                target_words_used: Vec::new(),
                llm_used: false,
            }),
        )
    }

    pub fn get_session(
        &self,
        session_id: &str,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        self.view(session_id, None)
    }

    pub async fn submit_turn(
        &self,
        session_id: &str,
        text: &str,
        client_request_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        self.submit_user_text(
            session_id,
            text,
            client_request_id,
            SpeakingTurnSource::Typed,
            now,
        )
        .await
    }

    pub async fn submit_audio(
        &self,
        session_id: &str,
        transcript: Option<&str>,
        audio_base64: Option<&str>,
        mime_type: Option<&str>,
        client_request_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        if !self.audio_enabled {
            return Err(SpeakingPracticeError::AudioDisabled);
        }
        let request_id = client_request_id.trim();
        if request_id.is_empty() {
            return Err(SpeakingPracticeError::Invalid(
                "clientRequestId is required".into(),
            ));
        }
        if self
            .store
            .get_turn_by_client_request(session_id, request_id)
            .map_err(SpeakingPracticeError::Storage)?
            .is_some()
        {
            return self.view(session_id, None);
        }
        let mut session = self.require_session(session_id)?;
        session.status = apply_event(session.status, SpeakingSessionEvent::AudioReceived)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;

        let typed = transcript.unwrap_or("").trim().to_string();
        let resolved = if !typed.is_empty() {
            Ok(typed)
        } else {
            match self.transcribe_audio(audio_base64, mime_type).await {
                Ok(text) => Ok(text),
                Err(error) => {
                    let _ = self.abandon_transcription(&mut session, now);
                    Err(error)
                }
            }
        }?;
        self.submit_user_text(
            session_id,
            &resolved,
            request_id,
            SpeakingTurnSource::Asr,
            now,
        )
        .await
    }

    pub async fn synthesize_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<TtsResult, SpeakingPracticeError> {
        let turns = self
            .store
            .list_turns(session_id)
            .map_err(SpeakingPracticeError::Storage)?;
        let turn = turns
            .into_iter()
            .find(|item| item.id == turn_id)
            .ok_or_else(|| {
                SpeakingPracticeError::NotFound(format!("turn `{turn_id}` not found"))
            })?;
        if turn.role != SpeakingTurnRole::Coach {
            return Err(SpeakingPracticeError::Invalid(
                "TTS is only available for coach turns".into(),
            ));
        }
        self.synthesize_text(&turn.text, Some(turn.id)).await
    }

    async fn synthesize_text(
        &self,
        text: &str,
        turn_id: Option<String>,
    ) -> Result<TtsResult, SpeakingPracticeError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(SpeakingPracticeError::Invalid("empty TTS text".into()));
        }
        let Some(tts) = &self.tts else {
            return Ok(TtsResult {
                audio_available: false,
                provider: "none".into(),
                mime_type: None,
                audio_base64: None,
                text: trimmed.to_string(),
                turn_id,
            });
        };
        let (mime_type, bytes) = tts.synthesize(trimmed).await?;
        Ok(TtsResult {
            audio_available: true,
            provider: tts.name().to_string(),
            mime_type: Some(mime_type),
            audio_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
            text: trimmed.to_string(),
            turn_id,
        })
    }

    async fn transcribe_audio(
        &self,
        audio_base64: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<String, SpeakingPracticeError> {
        let encoded = audio_base64.unwrap_or("").trim();
        if encoded.is_empty() {
            return Err(SpeakingPracticeError::EmptyAudio);
        }
        let payload = encoded
            .rsplit_once(',')
            .map(|(_, data)| data)
            .unwrap_or(encoded);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|error| {
                SpeakingPracticeError::Invalid(format!("audio was not valid base64: {error}"))
            })?;
        if bytes.is_empty() {
            return Err(SpeakingPracticeError::EmptyAudio);
        }
        if bytes.len() > MAX_AUDIO_BYTES {
            return Err(SpeakingPracticeError::Invalid(
                "audio exceeds the 1.5MB limit".into(),
            ));
        }
        let Some(asr) = &self.asr else {
            return Err(SpeakingPracticeError::AsrUnavailable(
                "backend ASR is not configured; type the line or use browser speech recognition"
                    .into(),
            ));
        };
        let mime = mime_type.unwrap_or("audio/webm");
        let text = asr.transcribe(&bytes, mime).await?;
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return Err(SpeakingPracticeError::EmptyTurn);
        }
        Ok(trimmed)
    }

    fn abandon_transcription(
        &self,
        session: &mut SpeakingSession,
        now: i64,
    ) -> Result<(), SpeakingPracticeError> {
        if session.status == SpeakingSessionStatus::Transcribing {
            session.status = apply_event(session.status, SpeakingSessionEvent::Resume)
                .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
            session.updated_at = now;
            self.store
                .update_session(session)
                .map_err(SpeakingPracticeError::Storage)?;
        }
        Ok(())
    }

    async fn submit_user_text(
        &self,
        session_id: &str,
        text: &str,
        client_request_id: &str,
        source: SpeakingTurnSource,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(SpeakingPracticeError::EmptyTurn);
        }
        let request_id = client_request_id.trim();
        if request_id.is_empty() {
            return Err(SpeakingPracticeError::Invalid(
                "clientRequestId is required".into(),
            ));
        }
        if let Some(existing) = self
            .store
            .get_turn_by_client_request(session_id, request_id)
            .map_err(SpeakingPracticeError::Storage)?
        {
            let _ = existing;
            return self.view(session_id, None);
        }
        let mut session = self.require_session(session_id)?;
        session.status = apply_event(session.status, SpeakingSessionEvent::UserTurn)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        let turns = self
            .store
            .list_turns(session_id)
            .map_err(SpeakingPracticeError::Storage)?;
        let seq = turns.iter().map(|item| item.seq).max().unwrap_or(-1) + 1;
        let user_turn = SpeakingTurn {
            id: format!("{session_id}:t{seq}"),
            session_id: session_id.to_string(),
            seq,
            role: SpeakingTurnRole::User,
            text: trimmed.to_string(),
            assist_text: None,
            source,
            client_request_id: Some(request_id.to_string()),
            corrections: Vec::new(),
            created_at: now,
        };
        self.store
            .insert_turn(&user_turn)
            .map_err(SpeakingPracticeError::Storage)?;
        let scenario = self.get_scenario(&session.scenario_id)?;
        let mut user_texts: Vec<String> = turns
            .iter()
            .filter(|item| item.role == SpeakingTurnRole::User)
            .map(|item| item.text.clone())
            .collect();
        user_texts.push(trimmed.to_string());
        session.completed_goal_ids = completed_goals(&scenario, &user_texts);
        let coach = match self.coach_reply(&scenario, &session, &turns, trimmed).await {
            Ok(reply) => reply,
            Err(error) => {
                session.status = apply_event(session.status, SpeakingSessionEvent::Fail)
                    .unwrap_or(SpeakingSessionStatus::Failed);
                session.updated_at = now;
                self.store
                    .update_session(&session)
                    .map_err(SpeakingPracticeError::Storage)?;
                return Err(error);
            }
        };
        let coach_seq = seq + 1;
        let coach_turn = SpeakingTurn {
            id: format!("{session_id}:t{coach_seq}"),
            session_id: session_id.to_string(),
            seq: coach_seq,
            role: SpeakingTurnRole::Coach,
            text: coach.reply_text.clone(),
            assist_text: coach.assist_text.clone(),
            source: SpeakingTurnSource::Typed,
            client_request_id: Some(format!("{request_id}:coach")),
            corrections: coach.corrections.clone(),
            created_at: now,
        };
        self.store
            .insert_turn(&coach_turn)
            .map_err(SpeakingPracticeError::Storage)?;
        session.status = apply_event(session.status, SpeakingSessionEvent::CoachReady)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        if all_goals_complete(&scenario, &session.completed_goal_ids) {
            session.status = apply_event(session.status, SpeakingSessionEvent::Complete)
                .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
            session.ended_at = Some(now);
            self.write_feedback(&session, &scenario, now)?;
        } else {
            session.status = apply_event(session.status, SpeakingSessionEvent::Start)
                .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        }
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        self.view(session_id, Some(coach))
    }

    pub fn pause(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        self.apply_simple(session_id, SpeakingSessionEvent::Pause, now)
    }

    pub fn resume(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let mut session = self.require_session(session_id)?;
        let event = if session.status == SpeakingSessionStatus::Failed {
            SpeakingSessionEvent::Retry
        } else {
            SpeakingSessionEvent::Resume
        };
        session.status = apply_event(session.status, event)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        self.view(session_id, None)
    }

    pub fn complete(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let mut session = self.require_session(session_id)?;
        if session.status != SpeakingSessionStatus::Completed {
            session.status = apply_event(session.status, SpeakingSessionEvent::Complete)
                .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
            session.ended_at = Some(now);
            session.updated_at = now;
            let scenario = self.get_scenario(&session.scenario_id)?;
            self.write_feedback(&session, &scenario, now)?;
            self.store
                .update_session(&session)
                .map_err(SpeakingPracticeError::Storage)?;
        }
        self.view(session_id, None)
    }

    pub fn interrupt(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        self.apply_simple(session_id, SpeakingSessionEvent::Interrupt, now)
    }

    pub fn add_expressions_to_review(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<ReviewImportResult, SpeakingPracticeError> {
        let Some(learning) = &self.learning else {
            return Err(SpeakingPracticeError::Unavailable(
                "learning review is not configured".into(),
            ));
        };
        let feedback = self.report(session_id)?;
        let session = self.require_session(session_id)?;
        let mut card_ids = Vec::new();
        for word in &feedback.expressions {
            let normalized = word.trim();
            if normalized.is_empty() {
                continue;
            }
            let match_row = feedback
                .dict_matches
                .iter()
                .find(|item| item.word.eq_ignore_ascii_case(normalized));
            let lemma = match_row
                .map(|item| item.lemma.as_str())
                .unwrap_or(normalized);
            let gloss = match_row
                .and_then(|item| item.gloss.clone())
                .unwrap_or_else(|| normalized.to_string());
            let id = format!("speaking-expr:{}", lemma.to_ascii_lowercase());
            let card = UniversalReviewCard {
                id: id.clone(),
                module: "language".into(),
                entity_id: format!("speaking-vocab:{lemma}"),
                entity_type: "speaking_expression".into(),
                card_type: ReviewCardType::Recall,
                prompt: format!("Use this expression: {lemma}"),
                answer: gloss,
                options: None,
                hint: Some(normalized.to_string()),
                context: Some(session.scenario_id.clone()),
                source_title: Some("Speaking practice".into()),
                user_note: None,
                due_at: now,
                interval_days: 0.0,
                ease: 2.5,
                mastery_score: 0.0,
                repetition_count: 0,
                lapses: 0,
                last_reviewed_at: None,
                created_at: now,
            };
            learning
                .upsert_review_card(&card)
                .map_err(SpeakingPracticeError::Storage)?;
            card_ids.push(id);
        }
        for item in feedback
            .errors
            .iter()
            .chain(feedback.grammar.iter())
            .chain(feedback.unnatural.iter())
        {
            let suggestion = item.suggestion.trim();
            if suggestion.is_empty() {
                continue;
            }
            let id = format!(
                "speaking-expr:{}",
                suggestion.to_ascii_lowercase().replace(' ', "-")
            );
            if card_ids.iter().any(|existing| existing == &id) {
                continue;
            }
            let card = UniversalReviewCard {
                id: id.clone(),
                module: "language".into(),
                entity_id: format!("speaking-fix:{id}"),
                entity_type: "speaking_expression".into(),
                card_type: ReviewCardType::Recall,
                prompt: format!("Say it this way: {suggestion}"),
                answer: item
                    .explanation
                    .clone()
                    .unwrap_or_else(|| item.span.clone()),
                options: None,
                hint: Some(item.span.clone()),
                context: Some(session.scenario_id.clone()),
                source_title: Some("Speaking practice".into()),
                user_note: None,
                due_at: now,
                interval_days: 0.0,
                ease: 2.5,
                mastery_score: 0.0,
                repetition_count: 0,
                lapses: 0,
                last_reviewed_at: None,
                created_at: now,
            };
            learning
                .upsert_review_card(&card)
                .map_err(SpeakingPracticeError::Storage)?;
            card_ids.push(id);
        }
        Ok(ReviewImportResult {
            cards_added: u32::try_from(card_ids.len()).unwrap_or(0),
            card_ids,
        })
    }

    pub fn report(&self, session_id: &str) -> Result<SpeakingFeedback, SpeakingPracticeError> {
        self.store
            .get_feedback(session_id)
            .map_err(SpeakingPracticeError::Storage)?
            .ok_or_else(|| {
                SpeakingPracticeError::NotFound(format!("no report for session `{session_id}`"))
            })
    }

    fn apply_simple(
        &self,
        session_id: &str,
        event: SpeakingSessionEvent,
        now: i64,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let mut session = self.require_session(session_id)?;
        session.status = apply_event(session.status, event)
            .map_err(|error| SpeakingPracticeError::Invalid(error.to_string()))?;
        session.updated_at = now;
        self.store
            .update_session(&session)
            .map_err(SpeakingPracticeError::Storage)?;
        self.view(session_id, None)
    }

    fn require_session(&self, session_id: &str) -> Result<SpeakingSession, SpeakingPracticeError> {
        self.store
            .get_session(session_id)
            .map_err(SpeakingPracticeError::Storage)?
            .ok_or_else(|| {
                SpeakingPracticeError::NotFound(format!("session `{session_id}` not found"))
            })
    }

    fn view(
        &self,
        session_id: &str,
        coach: Option<CoachPayload>,
    ) -> Result<SpeakingSessionView, SpeakingPracticeError> {
        let session = self.require_session(session_id)?;
        let scenario = self.get_scenario(&session.scenario_id)?;
        let turns = self
            .store
            .list_turns(session_id)
            .map_err(SpeakingPracticeError::Storage)?;
        let remaining = scenario
            .goals
            .iter()
            .filter(|goal| !session.completed_goal_ids.iter().any(|id| id == &goal.id))
            .map(|goal| goal.id.clone())
            .collect();
        Ok(SpeakingSessionView {
            task_progress: TaskProgress {
                completed: session.completed_goal_ids.clone(),
                remaining,
            },
            session,
            scenario,
            turns,
            coach,
        })
    }

    async fn coach_reply(
        &self,
        scenario: &SpeakingScenario,
        session: &SpeakingSession,
        previous: &[SpeakingTurn],
        user_text: &str,
    ) -> Result<CoachPayload, SpeakingPracticeError> {
        if let Some(llm) = &self.llm {
            match self
                .llm_reply(llm.as_ref(), scenario, session, previous, user_text)
                .await
            {
                Ok(payload) => return Ok(payload),
                Err(SpeakingPracticeError::Unavailable(_)) => {
                    // 配置被清空或明确不可用 → 脚本降级，不把会话打成 Failed。
                }
                Err(error) => return Err(error),
            }
        }
        let (text, assist) = scripted_reply_for(scenario, &session.completed_goal_ids);
        Ok(CoachPayload {
            reply_text: text,
            assist_text: if session.locale_assist == "zh" {
                assist
            } else {
                None
            },
            corrections: Vec::new(),
            target_words_used: Vec::new(),
            llm_used: false,
        })
    }

    async fn llm_reply(
        &self,
        llm: &dyn ChatModelProvider,
        scenario: &SpeakingScenario,
        session: &SpeakingSession,
        previous: &[SpeakingTurn],
        user_text: &str,
    ) -> Result<CoachPayload, SpeakingPracticeError> {
        let history = previous
            .iter()
            .map(|turn| format!("{}: {}", turn.role.as_str(), turn.text))
            .collect::<Vec<_>>()
            .join("\n");
        let goals = scenario
            .goals
            .iter()
            .map(|goal| format!("- {}: {}", goal.id, goal.description))
            .collect::<Vec<_>>()
            .join("\n");
        let focus = self.due_focus_lines();
        let system = format!(
            "{}\nCEFR: {}.\nYou MUST reply with a single JSON object: \
             {{\"replyText\":\"...\",\"assistText\":\"...or null\",\
             \"corrections\":[{{\"span\":\"\",\"category\":\"grammar|unnatural\",\
             \"suggestion\":\"\",\"explanation\":\"why this is more natural, in the learner assist language\"}}],\
             \"targetWordsUsed\":[\"\"]}}.\n\
             Stay in character. Ask a short follow-up that moves the scene forward.\n\
             If the learner's last line has a grammar or unnatural issue, put one correction with a clear explanation.\n\
             Example: span \"I very like\", suggestion \"I really like\", explanation \"English uses 'really' as the adverb, not 'very', before a verb.\"\n\
             category must never be pronunciation. Do not invent phoneme or fluency scores.\n\
             Keep replyText in English, 1-3 sentences.\n\
             Unfinished goals:\n{goals}\n\
             Weak points to reuse if they fit the scene:\n{focus}",
            scenario.role_prompt,
            session.cefr.as_str(),
        );
        let user = format!("Conversation so far:\n{history}\nuser: {user_text}");
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            session_id: Some(session.id.clone()),
            tools: Vec::new(),
            temperature: Some(0.4),
            max_tokens: Some(400),
        };
        let response = llm.chat(request).await.map_err(|error| match error.kind {
            devtoolbox_core::ProviderErrorKind::Unavailable => {
                SpeakingPracticeError::Unavailable(error.message)
            }
            devtoolbox_core::ProviderErrorKind::Timeout => {
                SpeakingPracticeError::Timeout(error.message)
            }
            _ => SpeakingPracticeError::Unavailable(error.message),
        })?;
        let raw = response.content.unwrap_or_default();
        let value = extract_json(&raw).map_err(|error| {
            SpeakingPracticeError::Invalid(format!("coach JSON was not usable: {error}"))
        })?;
        let reply_text = value
            .get("replyText")
            .or_else(|| value.get("reply_text"))
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if reply_text.is_empty() {
            return Err(SpeakingPracticeError::Invalid(
                "coach JSON missing replyText".into(),
            ));
        }
        let assist_text = value
            .get("assistText")
            .or_else(|| value.get("assist_text"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string);
        let corrections = value
            .get("corrections")
            .and_then(|item| item.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let category = item
                    .get("category")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unnatural");
                if category.eq_ignore_ascii_case("pronunciation")
                    || category.eq_ignore_ascii_case("pronunciation_unknown")
                {
                    return None;
                }
                Some(SpeakingCorrection {
                    span: item
                        .get("span")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string(),
                    category: if category == "grammar" {
                        "grammar".into()
                    } else {
                        "unnatural".into()
                    },
                    suggestion: item
                        .get("suggestion")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string(),
                    explanation: item
                        .get("explanation")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                })
            })
            .collect();
        let target_words_used = value
            .get("targetWordsUsed")
            .or_else(|| value.get("target_words_used"))
            .and_then(|item| item.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect();
        Ok(CoachPayload {
            reply_text,
            assist_text: if session.locale_assist == "zh" {
                assist_text
            } else {
                None
            },
            corrections,
            target_words_used,
            llm_used: true,
        })
    }

    fn write_feedback(
        &self,
        session: &SpeakingSession,
        scenario: &SpeakingScenario,
        now: i64,
    ) -> Result<(), SpeakingPracticeError> {
        if self
            .store
            .get_feedback(&session.id)
            .map_err(SpeakingPracticeError::Storage)?
            .is_some()
        {
            return Ok(());
        }
        let done = all_goals_complete(scenario, &session.completed_goal_ids);
        let summary = if done {
            format!(
                "You finished the {} scenario: {}.",
                scenario.title, scenario.description
            )
        } else {
            format!(
                "Session ended before every goal was met in {}.",
                scenario.title
            )
        };
        let expressions = scenario
            .target_vocab
            .iter()
            .map(|word| word.word.clone())
            .collect();
        let turns = self
            .store
            .list_turns(&session.id)
            .map_err(SpeakingPracticeError::Storage)?;
        let mut grammar = Vec::new();
        let mut unnatural = Vec::new();
        for turn in &turns {
            for item in &turn.corrections {
                if item.category.eq_ignore_ascii_case("pronunciation") {
                    continue;
                }
                if item.category.eq_ignore_ascii_case("grammar") {
                    grammar.push(item.clone());
                } else {
                    unnatural.push(item.clone());
                }
            }
        }
        let mut errors = grammar.clone();
        errors.extend(unnatural.clone());
        let mut drill_items = Vec::new();
        for goal in scenario
            .goals
            .iter()
            .filter(|goal| !session.completed_goal_ids.iter().any(|id| id == &goal.id))
        {
            drill_items.push(SpeakingDrillItem {
                kind: "goal".into(),
                prompt: goal.description.clone(),
                prompt_zh: Some(goal.description_zh.clone()),
            });
        }
        let user_blob = turns
            .iter()
            .filter(|item| item.role == SpeakingTurnRole::User)
            .map(|item| item.text.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        for word in &scenario.target_vocab {
            if !user_blob.contains(&word.word.to_ascii_lowercase()) {
                drill_items.push(SpeakingDrillItem {
                    kind: "expression".into(),
                    prompt: word.word.clone(),
                    prompt_zh: Some(word.gloss_zh.clone()),
                });
            }
        }
        let mut dict_matches = Vec::new();
        for word in &scenario.target_vocab {
            let hit = match &self.dictionary {
                Some(dict) => dict
                    .lookup_word(&word.word)
                    .map_err(SpeakingPracticeError::Storage)?,
                None => None,
            };
            dict_matches.push(hit.unwrap_or(SpeakingDictMatch {
                word: word.word.clone(),
                lemma: word.word.clone(),
                gloss: Some(word.gloss_zh.clone()),
                in_dictionary: false,
            }));
        }
        let next_focus = next_focus_from(&errors, &drill_items);
        let feedback = SpeakingFeedback {
            session_id: session.id.clone(),
            summary,
            expressions,
            errors,
            grammar,
            unnatural,
            pronunciation_status: "unassessed".into(),
            task_total: u32::try_from(scenario.goals.len()).unwrap_or(0),
            next_focus,
            drill_items,
            dict_matches,
            completed_goal_ids: session.completed_goal_ids.clone(),
            created_at: now,
        };
        self.store
            .upsert_feedback(&feedback)
            .map_err(SpeakingPracticeError::Storage)?;
        self.record_session_event(session, now)
    }

    fn record_session_event(
        &self,
        session: &SpeakingSession,
        now: i64,
    ) -> Result<(), SpeakingPracticeError> {
        let Some(learning) = &self.learning else {
            return Ok(());
        };
        let event = LearningEvent {
            id: String::new(),
            module: "language".into(),
            entity_type: "speaking_session".into(),
            entity_id: session.id.clone(),
            entity_title: Some(session.scenario_id.clone()),
            action: LearningAction::Complete,
            timestamp: now,
            duration_ms: None,
            metadata: serde_json::json!({
                "scenarioId": session.scenario_id,
                "completedGoalIds": session.completed_goal_ids,
            }),
            source: Some("speaking-practice".into()),
        };
        learning
            .record_event(&event, now)
            .map_err(SpeakingPracticeError::Storage)
    }

    fn due_focus_lines(&self) -> String {
        let Some(learning) = &self.learning else {
            return "(none)".into();
        };
        match learning.due_speaking_prompts(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_secs() as i64)
                .unwrap_or(0),
            6,
        ) {
            Ok(items) if !items.is_empty() => items
                .into_iter()
                .map(|line| format!("- {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => "(none)".into(),
        }
    }
}

fn next_focus_from(errors: &[SpeakingCorrection], drills: &[SpeakingDrillItem]) -> Vec<String> {
    let mut focus = Vec::new();
    for item in errors {
        let line = item.suggestion.trim();
        if !line.is_empty() && !focus.iter().any(|existing| existing == line) {
            focus.push(line.to_string());
        }
        if focus.len() >= 6 {
            return focus;
        }
    }
    for item in drills {
        if !focus.iter().any(|existing| existing == &item.prompt) {
            focus.push(item.prompt.clone());
        }
        if focus.len() >= 6 {
            break;
        }
    }
    focus
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub scenario_id: String,
    pub cefr: Option<String>,
    pub locale_assist: Option<String>,
    pub client_request_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTurnInput {
    pub text: String,
    pub client_request_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAudioInput {
    #[serde(default)]
    pub transcript: Option<String>,
    #[serde(default)]
    pub audio_base64: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    pub client_request_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStore {
        scenarios: Mutex<HashMap<String, SpeakingScenario>>,
        sessions: Mutex<HashMap<String, SpeakingSession>>,
        by_client: Mutex<HashMap<String, String>>,
        turns: Mutex<HashMap<String, Vec<SpeakingTurn>>>,
        feedback: Mutex<HashMap<String, SpeakingFeedback>>,
    }

    impl SpeakingPracticeStore for MemoryStore {
        fn upsert_scenario(&self, scenario: &SpeakingScenario) -> Result<(), String> {
            self.scenarios
                .lock()
                .insert(scenario.id.clone(), scenario.clone());
            Ok(())
        }
        fn list_scenarios(&self) -> Result<Vec<SpeakingScenario>, String> {
            Ok(self.scenarios.lock().values().cloned().collect())
        }
        fn get_scenario(&self, id: &str) -> Result<Option<SpeakingScenario>, String> {
            let lock = self.scenarios.lock();
            Ok(lock
                .get(id)
                .cloned()
                .or_else(|| lock.values().find(|item| item.slug == id).cloned()))
        }
        fn insert_session(&self, session: &SpeakingSession) -> Result<(), String> {
            self.by_client
                .lock()
                .insert(session.client_request_id.clone(), session.id.clone());
            self.sessions
                .lock()
                .insert(session.id.clone(), session.clone());
            Ok(())
        }
        fn get_session_by_client_request(
            &self,
            client_request_id: &str,
        ) -> Result<Option<SpeakingSession>, String> {
            let id = self.by_client.lock().get(client_request_id).cloned();
            match id {
                Some(id) => self.get_session(&id),
                None => Ok(None),
            }
        }
        fn get_session(&self, id: &str) -> Result<Option<SpeakingSession>, String> {
            Ok(self.sessions.lock().get(id).cloned())
        }
        fn update_session(&self, session: &SpeakingSession) -> Result<(), String> {
            self.sessions
                .lock()
                .insert(session.id.clone(), session.clone());
            Ok(())
        }
        fn insert_turn(&self, turn: &SpeakingTurn) -> Result<(), String> {
            self.turns
                .lock()
                .entry(turn.session_id.clone())
                .or_default()
                .push(turn.clone());
            Ok(())
        }
        fn get_turn_by_client_request(
            &self,
            session_id: &str,
            client_request_id: &str,
        ) -> Result<Option<SpeakingTurn>, String> {
            Ok(self.turns.lock().get(session_id).and_then(|turns| {
                turns
                    .iter()
                    .find(|item| item.client_request_id.as_deref() == Some(client_request_id))
                    .cloned()
            }))
        }
        fn list_turns(&self, session_id: &str) -> Result<Vec<SpeakingTurn>, String> {
            Ok(self
                .turns
                .lock()
                .get(session_id)
                .cloned()
                .unwrap_or_default())
        }
        fn upsert_feedback(&self, feedback: &SpeakingFeedback) -> Result<(), String> {
            self.feedback
                .lock()
                .insert(feedback.session_id.clone(), feedback.clone());
            Ok(())
        }
        fn get_feedback(&self, session_id: &str) -> Result<Option<SpeakingFeedback>, String> {
            Ok(self.feedback.lock().get(session_id).cloned())
        }
        fn count_completed_sessions(&self) -> Result<u32, String> {
            Ok(self
                .sessions
                .lock()
                .values()
                .filter(|item| item.status == SpeakingSessionStatus::Completed)
                .count() as u32)
        }
    }

    struct ScriptLlm;

    #[async_trait]
    impl ChatModelProvider for ScriptLlm {
        fn name(&self) -> &'static str {
            "script"
        }
        async fn chat(
            &self,
            _request: ChatRequest,
        ) -> Result<devtoolbox_core::ChatResponse, devtoolbox_core::ProviderError> {
            Ok(devtoolbox_core::ChatResponse {
                content: Some(
                    r#"{"replyText":"A latte coming up. Card or cash?","assistText":"拿铁马上好。刷卡还是现金？","corrections":[{"span":"I very like","category":"unnatural","suggestion":"I really like","explanation":"English uses really, not very, before a verb like like."}],"targetWordsUsed":["latte"]}"#.into(),
                ),
                reasoning_content: None,
                tool_calls: Vec::new(),
                usage: devtoolbox_core::ChatUsage::default(),
            })
        }
    }

    fn service(llm: Option<Arc<dyn ChatModelProvider>>) -> SpeakingPracticeService {
        SpeakingPracticeService::new(Arc::new(MemoryStore::default()), llm)
    }

    #[tokio::test]
    async fn cafe_scripted_path_completes_without_llm() {
        let service = service(None);
        let created = service
            .create_session("speaking:cafe", None, "zh", "req-cafe-1", 1)
            .await
            .unwrap();
        assert_eq!(created.session.status, SpeakingSessionStatus::Listening);
        assert_eq!(created.turns.len(), 1);
        assert!(!created.coach.unwrap().llm_used);

        let again = service
            .create_session("speaking:cafe", None, "zh", "req-cafe-1", 2)
            .await
            .unwrap();
        assert_eq!(again.session.id, created.session.id);

        let empty = service
            .submit_turn(&created.session.id, "   ", "t0", 3)
            .await
            .unwrap_err();
        assert!(matches!(empty, SpeakingPracticeError::EmptyTurn));

        let _ = service
            .submit_turn(&created.session.id, "Hello!", "t1", 4)
            .await
            .unwrap();
        let _ = service
            .submit_turn(&created.session.id, "I'd like a latte.", "t2", 5)
            .await
            .unwrap();
        let done = service
            .submit_turn(&created.session.id, "I'll pay by card.", "t3", 6)
            .await
            .unwrap();
        assert_eq!(done.session.status, SpeakingSessionStatus::Completed);
        assert!(
            service
                .report(&created.session.id)
                .unwrap()
                .summary
                .contains("cafe")
        );
    }

    #[tokio::test]
    async fn llm_json_is_used_when_provider_is_configured() {
        let service = service(Some(Arc::new(ScriptLlm)));
        let created = service
            .create_session("speaking:cafe", Some("A2"), "zh", "req-llm-1", 1)
            .await
            .unwrap();
        let view = service
            .submit_turn(&created.session.id, "Hi, a latte please", "u1", 2)
            .await
            .unwrap();
        assert_eq!(view.coach.as_ref().map(|item| item.llm_used), Some(true));
        assert!(
            view.turns
                .iter()
                .any(|turn| turn.text.contains("latte coming up"))
        );
    }

    #[tokio::test]
    async fn pause_and_resume_round_trip() {
        let service = service(None);
        let created = service
            .create_session("speaking:hotel", None, "none", "req-pause", 1)
            .await
            .unwrap();
        let paused = service.pause(&created.session.id, 2).unwrap();
        assert_eq!(paused.session.status, SpeakingSessionStatus::Paused);
        let resumed = service.resume(&created.session.id, 3).unwrap();
        assert_eq!(resumed.session.status, SpeakingSessionStatus::Listening);
    }

    struct FakeAsr;

    #[async_trait]
    impl SpeechToTextPort for FakeAsr {
        fn name(&self) -> &'static str {
            "fake-asr"
        }
        async fn transcribe(
            &self,
            _audio: &[u8],
            _mime_type: &str,
        ) -> Result<String, SpeakingPracticeError> {
            Ok("I'd like a latte.".into())
        }
    }

    struct FakeTts;

    #[async_trait]
    impl TextToSpeechPort for FakeTts {
        fn name(&self) -> &'static str {
            "fake-tts"
        }
        async fn synthesize(
            &self,
            _text: &str,
        ) -> Result<(String, Vec<u8>), SpeakingPracticeError> {
            Ok(("audio/wav".into(), b"RIFF....".to_vec()))
        }
    }

    #[tokio::test]
    async fn browser_transcript_is_accepted_without_backend_asr() {
        let service = service(None);
        let created = service
            .create_session("speaking:cafe", None, "zh", "req-audio-1", 1)
            .await
            .unwrap();
        let view = service
            .submit_audio(
                &created.session.id,
                Some("I'd like a latte."),
                Some("AAAA"),
                Some("audio/webm"),
                "a1",
                2,
            )
            .await
            .unwrap();
        assert_eq!(
            view.turns
                .iter()
                .find(|item| item.role == SpeakingTurnRole::User)
                .map(|item| item.source),
            Some(SpeakingTurnSource::Asr)
        );
        assert_eq!(view.session.status, SpeakingSessionStatus::Listening);
    }

    #[tokio::test]
    async fn audio_without_transcript_fails_honestly_when_asr_missing() {
        let service = service(None);
        let created = service
            .create_session("speaking:cafe", None, "zh", "req-audio-2", 1)
            .await
            .unwrap();
        let error = service
            .submit_audio(
                &created.session.id,
                None,
                Some("AAAA"),
                Some("audio/webm"),
                "a2",
                2,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, SpeakingPracticeError::AsrUnavailable(_)));
        let again = service.get_session(&created.session.id).unwrap();
        assert_eq!(again.session.status, SpeakingSessionStatus::Listening);
    }

    #[tokio::test]
    async fn configured_asr_transcribes_uploaded_audio() {
        let service = service(None).with_audio(true, Some(Arc::new(FakeAsr)), None);
        let created = service
            .create_session("speaking:cafe", None, "zh", "req-audio-3", 1)
            .await
            .unwrap();
        let view = service
            .submit_audio(
                &created.session.id,
                None,
                Some("AAAA"),
                Some("audio/webm"),
                "a3",
                2,
            )
            .await
            .unwrap();
        assert!(
            view.turns
                .iter()
                .any(|item| item.role == SpeakingTurnRole::User && item.text.contains("latte"))
        );
    }

    #[tokio::test]
    async fn tts_degrades_when_unconfigured_and_returns_audio_when_mocked() {
        let plain = service(None);
        let created = plain
            .create_session("speaking:cafe", None, "zh", "req-tts-1", 1)
            .await
            .unwrap();
        let opening = created.turns[0].id.clone();
        let missing = plain
            .synthesize_turn(&created.session.id, &opening)
            .await
            .unwrap();
        assert!(!missing.audio_available);
        assert_eq!(missing.text, created.turns[0].text);

        let voiced = service(None).with_audio(true, None, Some(Arc::new(FakeTts)));
        let created = voiced
            .create_session("speaking:cafe", None, "zh", "req-tts-2", 1)
            .await
            .unwrap();
        let opening = created.turns[0].id.clone();
        let ready = voiced
            .synthesize_turn(&created.session.id, &opening)
            .await
            .unwrap();
        assert!(ready.audio_available);
        assert_eq!(ready.provider, "fake-tts");
        assert!(voiced.capabilities().tts_configured);
        assert!(!voiced.capabilities().realtime_available);
        assert_eq!(voiced.capabilities().phoneme_scoring, "unavailable");
    }

    struct FakeLearning {
        cards: Mutex<Vec<UniversalReviewCard>>,
        events: Mutex<Vec<LearningEvent>>,
    }

    impl Default for FakeLearning {
        fn default() -> Self {
            Self {
                cards: Mutex::new(Vec::new()),
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl SpeakingLearningPort for FakeLearning {
        fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), String> {
            self.cards.lock().push(card.clone());
            Ok(())
        }
        fn record_event(&self, event: &LearningEvent, _now: i64) -> Result<(), String> {
            self.events.lock().push(event.clone());
            Ok(())
        }
        fn due_speaking_prompts(&self, _now: i64, _limit: usize) -> Result<Vec<String>, String> {
            Ok(self
                .cards
                .lock()
                .iter()
                .filter(|card| card.entity_type == "speaking_expression")
                .map(|card| card.prompt.clone())
                .collect())
        }
    }

    struct FakeDict;

    impl SpeakingDictionaryPort for FakeDict {
        fn lookup_word(&self, word: &str) -> Result<Option<SpeakingDictMatch>, String> {
            Ok(Some(SpeakingDictMatch {
                word: word.to_string(),
                lemma: word.to_ascii_lowercase(),
                gloss: Some("词典释义".into()),
                in_dictionary: true,
            }))
        }
    }

    #[tokio::test]
    async fn report_splits_errors_and_never_fakes_pronunciation() {
        let learning = Arc::new(FakeLearning::default());
        let service = service(Some(Arc::new(ScriptLlm)))
            .with_learning(learning.clone())
            .with_dictionary(Arc::new(FakeDict));
        let created = service
            .create_session("speaking:cafe", None, "zh", "req-report", 1)
            .await
            .unwrap();
        let first = service
            .submit_turn(&created.session.id, "I very like coffee.", "u1", 2)
            .await
            .unwrap();
        assert_eq!(
            first.coach.as_ref().unwrap().corrections[0].suggestion,
            "I really like"
        );
        assert!(
            first.coach.as_ref().unwrap().corrections[0]
                .explanation
                .as_deref()
                .unwrap_or("")
                .contains("really")
        );
        let _ = service
            .submit_turn(&created.session.id, "I'll pay by card.", "u2", 3)
            .await
            .unwrap();
        let done = service.complete(&created.session.id, 4).unwrap();
        assert_eq!(done.session.status, SpeakingSessionStatus::Completed);
        let report = service.report(&created.session.id).unwrap();
        assert_eq!(report.pronunciation_status, "unassessed");
        assert_eq!(report.task_total, 3);
        assert!(!report.next_focus.is_empty());
        assert!(report.dict_matches.iter().any(|item| item.in_dictionary));
        assert!(!learning.events.lock().is_empty());
        let imported = service
            .add_expressions_to_review(&created.session.id, 5)
            .unwrap();
        assert!(imported.cards_added >= 1);
        assert!(!learning.cards.lock().is_empty());
    }
}
