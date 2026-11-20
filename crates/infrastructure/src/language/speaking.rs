//! 口语陪练 SQLite（`language.db` 内独立表组）。

use rusqlite::{OptionalExtension, params};

use devtoolbox_core::language::{
    CefrLevel, SpeakingCorrection, SpeakingFeedback, SpeakingScenario, SpeakingSession,
    SpeakingSessionStatus, SpeakingTurn, SpeakingTurnRole, SpeakingTurnSource,
};

use crate::error::InfrastructureError;

use super::store::LanguageStore;

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

impl LanguageStore {
    pub(crate) fn migrate_speaking_practice(&self) -> Result<(), InfrastructureError> {
        self.conn()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS speaking_scenarios (
                    id TEXT PRIMARY KEY,
                    slug TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    cefr TEXT NOT NULL,
                    body_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS speaking_sessions (
                    id TEXT PRIMARY KEY,
                    scenario_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    cefr TEXT NOT NULL,
                    locale_assist TEXT NOT NULL,
                    client_request_id TEXT NOT NULL UNIQUE,
                    completed_goal_ids_json TEXT NOT NULL,
                    coach_mode TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    ended_at INTEGER
                );
                CREATE INDEX IF NOT EXISTS idx_speaking_sessions_updated
                    ON speaking_sessions(updated_at DESC);
                CREATE TABLE IF NOT EXISTS speaking_turns (
                    id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    seq INTEGER NOT NULL,
                    role TEXT NOT NULL,
                    text TEXT NOT NULL,
                    assist_text TEXT,
                    source TEXT NOT NULL,
                    client_request_id TEXT,
                    created_at INTEGER NOT NULL,
                    UNIQUE(session_id, seq)
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_speaking_turn_client
                    ON speaking_turns(session_id, client_request_id)
                    WHERE client_request_id IS NOT NULL;
                CREATE TABLE IF NOT EXISTS speaking_feedback (
                    session_id TEXT PRIMARY KEY,
                    summary TEXT NOT NULL,
                    expressions_json TEXT NOT NULL,
                    errors_json TEXT NOT NULL,
                    completed_goal_ids_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );",
            )
            .map_err(sqlite)?;
        self.ensure_column(
            "speaking_turns",
            "corrections_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column("speaking_feedback", "report_json", "TEXT")?;
        Ok(())
    }

    fn ensure_column(
        &self,
        table: &str,
        column: &str,
        decl: &str,
    ) -> Result<(), InfrastructureError> {
        let sql =
            format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'");
        let has_column = self
            .conn()
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .map_err(sqlite)?;
        if has_column == 0 {
            self.conn()
                .execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"),
                    [],
                )
                .map_err(sqlite)?;
        }
        Ok(())
    }

    pub fn count_completed_speaking_sessions(&self) -> Result<u32, InfrastructureError> {
        let count: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM speaking_sessions WHERE status = 'completed'",
                [],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        Ok(u32::try_from(count).unwrap_or(0))
    }

    pub fn upsert_speaking_scenario(
        &self,
        scenario: &SpeakingScenario,
    ) -> Result<(), InfrastructureError> {
        let body = serde_json::to_string(scenario)
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        self.conn()
            .execute(
                "INSERT INTO speaking_scenarios (id, slug, title, cefr, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)
                 ON CONFLICT(id) DO UPDATE SET
                    slug = excluded.slug,
                    title = excluded.title,
                    cefr = excluded.cefr,
                    body_json = excluded.body_json",
                params![
                    scenario.id,
                    scenario.slug,
                    scenario.title,
                    scenario.cefr.as_str(),
                    body
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn list_speaking_scenarios(&self) -> Result<Vec<SpeakingScenario>, InfrastructureError> {
        let mut statement = self
            .conn()
            .prepare("SELECT body_json FROM speaking_scenarios ORDER BY slug ASC")
            .map_err(sqlite)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sqlite)?;
        let mut items = Vec::new();
        for row in rows {
            let json = row.map_err(sqlite)?;
            items.push(
                serde_json::from_str(&json)
                    .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?,
            );
        }
        Ok(items)
    }

    pub fn get_speaking_scenario(
        &self,
        id: &str,
    ) -> Result<Option<SpeakingScenario>, InfrastructureError> {
        let json: Option<String> = self
            .conn()
            .query_row(
                "SELECT body_json FROM speaking_scenarios WHERE id = ?1 OR slug = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        json.map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))
        })
        .transpose()
    }

    pub fn insert_speaking_session(
        &self,
        session: &SpeakingSession,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO speaking_sessions
                    (id, scenario_id, status, cefr, locale_assist, client_request_id,
                     completed_goal_ids_json, coach_mode, created_at, updated_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    session.id,
                    session.scenario_id,
                    session.status.as_str(),
                    session.cefr.as_str(),
                    session.locale_assist,
                    session.client_request_id,
                    serde_json::to_string(&session.completed_goal_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    session.coach_mode,
                    session.created_at,
                    session.updated_at,
                    session.ended_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn update_speaking_session(
        &self,
        session: &SpeakingSession,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "UPDATE speaking_sessions SET
                    status = ?2, cefr = ?3, locale_assist = ?4,
                    completed_goal_ids_json = ?5, coach_mode = ?6,
                    updated_at = ?7, ended_at = ?8
                 WHERE id = ?1",
                params![
                    session.id,
                    session.status.as_str(),
                    session.cefr.as_str(),
                    session.locale_assist,
                    serde_json::to_string(&session.completed_goal_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    session.coach_mode,
                    session.updated_at,
                    session.ended_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn get_speaking_session(
        &self,
        id: &str,
    ) -> Result<Option<SpeakingSession>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT id, scenario_id, status, cefr, locale_assist, client_request_id,
                        completed_goal_ids_json, coach_mode, created_at, updated_at, ended_at
                 FROM speaking_sessions WHERE id = ?1",
                [id],
                row_to_session,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn get_speaking_session_by_client_request(
        &self,
        client_request_id: &str,
    ) -> Result<Option<SpeakingSession>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT id, scenario_id, status, cefr, locale_assist, client_request_id,
                        completed_goal_ids_json, coach_mode, created_at, updated_at, ended_at
                 FROM speaking_sessions WHERE client_request_id = ?1",
                [client_request_id],
                row_to_session,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn insert_speaking_turn(&self, turn: &SpeakingTurn) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO speaking_turns
                    (id, session_id, seq, role, text, assist_text, source, client_request_id, created_at, corrections_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    turn.id,
                    turn.session_id,
                    turn.seq,
                    turn.role.as_str(),
                    turn.text,
                    turn.assist_text,
                    turn.source.as_str(),
                    turn.client_request_id,
                    turn.created_at,
                    serde_json::to_string(&turn.corrections).unwrap_or_else(|_| "[]".into()),
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn get_speaking_turn_by_client_request(
        &self,
        session_id: &str,
        client_request_id: &str,
    ) -> Result<Option<SpeakingTurn>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT id, session_id, seq, role, text, assist_text, source, client_request_id, created_at, corrections_json
                 FROM speaking_turns WHERE session_id = ?1 AND client_request_id = ?2",
                params![session_id, client_request_id],
                row_to_turn,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn list_speaking_turns(
        &self,
        session_id: &str,
    ) -> Result<Vec<SpeakingTurn>, InfrastructureError> {
        let mut statement = self
            .conn()
            .prepare(
                "SELECT id, session_id, seq, role, text, assist_text, source, client_request_id, created_at, corrections_json
                 FROM speaking_turns WHERE session_id = ?1 ORDER BY seq ASC",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([session_id], row_to_turn)
            .map_err(sqlite)?;
        let mut turns = Vec::new();
        for row in rows {
            turns.push(row.map_err(sqlite)?);
        }
        Ok(turns)
    }

    pub fn upsert_speaking_feedback(
        &self,
        feedback: &SpeakingFeedback,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO speaking_feedback
                    (session_id, summary, expressions_json, errors_json, completed_goal_ids_json, created_at, report_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(session_id) DO UPDATE SET
                    summary = excluded.summary,
                    expressions_json = excluded.expressions_json,
                    errors_json = excluded.errors_json,
                    completed_goal_ids_json = excluded.completed_goal_ids_json,
                    report_json = excluded.report_json",
                params![
                    feedback.session_id,
                    feedback.summary,
                    serde_json::to_string(&feedback.expressions).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&feedback.errors).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&feedback.completed_goal_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    feedback.created_at,
                    serde_json::to_string(feedback).unwrap_or_else(|_| "{}".into()),
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn get_speaking_feedback(
        &self,
        session_id: &str,
    ) -> Result<Option<SpeakingFeedback>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT session_id, summary, expressions_json, errors_json, completed_goal_ids_json, created_at, report_json
                 FROM speaking_feedback WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite)?
            .map(
                |(
                    session_id,
                    summary,
                    expressions_json,
                    errors_json,
                    goals_json,
                    created_at,
                    report_json,
                )| {
                    if let Some(raw) = report_json.filter(|item| !item.is_empty() && item != "{}")
                        && let Ok(parsed) = serde_json::from_str::<SpeakingFeedback>(&raw)
                    {
                        return Ok(parsed);
                    }
                    let errors: Vec<SpeakingCorrection> =
                        serde_json::from_str(&errors_json).unwrap_or_default();
                    Ok(SpeakingFeedback {
                        session_id,
                        summary,
                        expressions: serde_json::from_str(&expressions_json).unwrap_or_default(),
                        grammar: errors
                            .iter()
                            .filter(|item| item.category == "grammar")
                            .cloned()
                            .collect(),
                        unnatural: errors
                            .iter()
                            .filter(|item| item.category != "grammar")
                            .cloned()
                            .collect(),
                        pronunciation_status: "unassessed".into(),
                        task_total: 0,
                        drill_items: Vec::new(),
                        dict_matches: Vec::new(),
                        errors,
                        completed_goal_ids: serde_json::from_str(&goals_json).unwrap_or_default(),
                        next_focus: Vec::new(),
                        created_at,
                    })
                },
            )
            .transpose()
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpeakingSession> {
    let status_raw: String = row.get(2)?;
    let cefr_raw: String = row.get(3)?;
    let goals_json: String = row.get(6)?;
    let status =
        SpeakingSessionStatus::parse(&status_raw).ok_or_else(|| parse_error(2, &status_raw))?;
    let cefr = CefrLevel::parse(&cefr_raw).ok_or_else(|| parse_error(3, &cefr_raw))?;
    Ok(SpeakingSession {
        id: row.get(0)?,
        scenario_id: row.get(1)?,
        status,
        cefr,
        locale_assist: row.get(4)?,
        client_request_id: row.get(5)?,
        completed_goal_ids: serde_json::from_str(&goals_json).unwrap_or_default(),
        coach_mode: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        ended_at: row.get(10)?,
    })
}

fn row_to_turn(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpeakingTurn> {
    let role_raw: String = row.get(3)?;
    let source_raw: String = row.get(6)?;
    let role = SpeakingTurnRole::parse(&role_raw).ok_or_else(|| parse_error(3, &role_raw))?;
    let source =
        SpeakingTurnSource::parse(&source_raw).ok_or_else(|| parse_error(6, &source_raw))?;
    Ok(SpeakingTurn {
        id: row.get(0)?,
        session_id: row.get(1)?,
        seq: row.get(2)?,
        role,
        text: row.get(4)?,
        assist_text: row.get(5)?,
        source,
        client_request_id: row.get(7)?,
        created_at: row.get(8)?,
        corrections: {
            let raw: String = row
                .get::<_, Option<String>>(9)?
                .unwrap_or_else(|| "[]".into());
            serde_json::from_str(&raw).unwrap_or_default()
        },
    })
}

fn parse_error(index: usize, raw: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(ParseStatusError(raw.to_string())),
    )
}

#[derive(Debug)]
struct ParseStatusError(String);

impl std::fmt::Display for ParseStatusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid speaking enum value `{}`", self.0)
    }
}

impl std::error::Error for ParseStatusError {}
