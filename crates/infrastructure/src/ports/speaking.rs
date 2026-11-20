//! `LanguageStore` → `SpeakingPracticeStore`.

use std::sync::Arc;

use devtoolbox_application::language::{SpeakingPracticeStore, SpeakingProgressPort};
use devtoolbox_core::language::{
    SpeakingFeedback, SpeakingScenario, SpeakingSession, SpeakingTurn,
};
use parking_lot::Mutex;

use crate::language::LanguageStore;

pub struct SpeakingPracticeStoreAdapter {
    store: Arc<Mutex<LanguageStore>>,
}

impl SpeakingPracticeStoreAdapter {
    #[must_use]
    pub fn new(store: Arc<Mutex<LanguageStore>>) -> Self {
        Self { store }
    }
}

impl SpeakingPracticeStore for SpeakingPracticeStoreAdapter {
    fn upsert_scenario(&self, scenario: &SpeakingScenario) -> Result<(), String> {
        self.store
            .lock()
            .upsert_speaking_scenario(scenario)
            .map_err(|error| error.to_string())
    }

    fn list_scenarios(&self) -> Result<Vec<SpeakingScenario>, String> {
        self.store
            .lock()
            .list_speaking_scenarios()
            .map_err(|error| error.to_string())
    }

    fn get_scenario(&self, id: &str) -> Result<Option<SpeakingScenario>, String> {
        self.store
            .lock()
            .get_speaking_scenario(id)
            .map_err(|error| error.to_string())
    }

    fn insert_session(&self, session: &SpeakingSession) -> Result<(), String> {
        self.store
            .lock()
            .insert_speaking_session(session)
            .map_err(|error| error.to_string())
    }

    fn get_session_by_client_request(
        &self,
        client_request_id: &str,
    ) -> Result<Option<SpeakingSession>, String> {
        self.store
            .lock()
            .get_speaking_session_by_client_request(client_request_id)
            .map_err(|error| error.to_string())
    }

    fn get_session(&self, id: &str) -> Result<Option<SpeakingSession>, String> {
        self.store
            .lock()
            .get_speaking_session(id)
            .map_err(|error| error.to_string())
    }

    fn update_session(&self, session: &SpeakingSession) -> Result<(), String> {
        self.store
            .lock()
            .update_speaking_session(session)
            .map_err(|error| error.to_string())
    }

    fn insert_turn(&self, turn: &SpeakingTurn) -> Result<(), String> {
        self.store
            .lock()
            .insert_speaking_turn(turn)
            .map_err(|error| error.to_string())
    }

    fn get_turn_by_client_request(
        &self,
        session_id: &str,
        client_request_id: &str,
    ) -> Result<Option<SpeakingTurn>, String> {
        self.store
            .lock()
            .get_speaking_turn_by_client_request(session_id, client_request_id)
            .map_err(|error| error.to_string())
    }

    fn list_turns(&self, session_id: &str) -> Result<Vec<SpeakingTurn>, String> {
        self.store
            .lock()
            .list_speaking_turns(session_id)
            .map_err(|error| error.to_string())
    }

    fn upsert_feedback(&self, feedback: &SpeakingFeedback) -> Result<(), String> {
        self.store
            .lock()
            .upsert_speaking_feedback(feedback)
            .map_err(|error| error.to_string())
    }

    fn get_feedback(&self, session_id: &str) -> Result<Option<SpeakingFeedback>, String> {
        self.store
            .lock()
            .get_speaking_feedback(session_id)
            .map_err(|error| error.to_string())
    }

    fn count_completed_sessions(&self) -> Result<u32, String> {
        self.store
            .lock()
            .count_completed_speaking_sessions()
            .map_err(|error| error.to_string())
    }
}

impl SpeakingProgressPort for SpeakingPracticeStoreAdapter {
    fn completed_sessions(&self) -> Result<u32, String> {
        self.count_completed_sessions()
    }
}
