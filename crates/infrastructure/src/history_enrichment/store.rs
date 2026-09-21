//! Enrichment SQLite 仓储（V5 Gate 2）。
//!
//! 与 TravelStore 同款 rusqlite 方案：上层用 `Mutex` 串行化（AppState 持有
//! `Arc<Mutex<EnrichmentSqliteStore>>`）。JSON 损坏 → 错误，由上层当作缺失处理。

use std::path::Path;

use rusqlite::Connection;

use crate::error::InfrastructureError;
use devtoolbox_core::history_enrichment::{
    EnrichmentKey, EnrichmentRecord,
    EnrichmentSection, EnrichmentState,
};

/// 读状态（库内列），与派生 STALE 无关。
fn parse_state(raw: &str) -> EnrichmentState {
    match raw {
        "READY" => EnrichmentState::Ready,
        "GENERATING" => EnrichmentState::Generating,
        "FAILED" => EnrichmentState::Failed,
        "REVIEWED" => EnrichmentState::Reviewed,
        _ => EnrichmentState::Missing,
    }
}

fn state_text(state: EnrichmentState) -> &'static str {
    match state {
        EnrichmentState::Missing => "MISSING",
        EnrichmentState::Generating => "GENERATING",
        EnrichmentState::Ready => "READY",
        EnrichmentState::Stale => "READY", // 派生状态不落库
        EnrichmentState::Failed => "FAILED",
        EnrichmentState::Reviewed => "REVIEWED",
    }
}

/// SQLite Enrichment 仓储。
pub struct EnrichmentSqliteStore {
    connection: Connection,
}

impl EnrichmentSqliteStore {
    /// 打开（必要时创建）数据库并确保 Schema 存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS history_enrichment (
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    section TEXT NOT NULL,
                    locale TEXT NOT NULL,
                    schema_version INTEGER NOT NULL,
                    revision INTEGER NOT NULL,
                    state TEXT NOT NULL,
                    payload_json TEXT,
                    metadata_json TEXT,
                    error TEXT,
                    reviewed INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (entity_type, entity_id, section, locale, schema_version, revision)
                );",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    fn insert(&self, record: &EnrichmentRecord) -> Result<(), InfrastructureError> {
        let key = &record.key;
        let payload_json = record
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?
            .unwrap_or_default();
        let metadata_json = record
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?
            .unwrap_or_default();
        self.connection
            .execute(
                "INSERT INTO history_enrichment
                    (entity_type, entity_id, section, locale, schema_version, revision,
                     state, payload_json, metadata_json, error, reviewed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(entity_type, entity_id, section, locale, schema_version, revision)
                 DO UPDATE SET state=excluded.state, payload_json=excluded.payload_json,
                    metadata_json=excluded.metadata_json, error=excluded.error,
                    reviewed=excluded.reviewed",
                rusqlite::params![
                    key.entity_type,
                    key.entity_id,
                    key.section.as_str(),
                    key.locale,
                    key.schema_version,
                    record.revision,
                    state_text(record.state),
                    payload_json,
                    metadata_json,
                    record.error,
                    record.reviewed as u8,
                ],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }

    fn row_to_record(
        row: &rusqlite::Row<'_>,
    ) -> Result<EnrichmentRecord, InfrastructureError> {
        let entity_type: String = row.get(0).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let entity_id: String = row.get(1).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let section_raw: String = row.get(2).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let locale: String = row.get(3).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let schema_version: u16 = row.get(4).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let revision: u32 = row.get(5).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let state: String = row.get(6).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let payload_json: Option<String> = row.get(7).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let metadata_json: Option<String> = row.get(8).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let error: Option<String> = row.get(9).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;
        let reviewed: u8 = row.get(10).map_err(|e| InfrastructureError::Sqlite(e.to_string()))?;

        let section = match section_raw.as_str() {
            "overview" => EnrichmentSection::Overview,
            "background" => EnrichmentSection::Background,
            "impact" => EnrichmentSection::Impact,
            other => {
                return Err(InfrastructureError::Sqlite(format!("unknown section: {other}")));
            }
        };
        Ok(EnrichmentRecord {
            key: EnrichmentKey {
                entity_type,
                entity_id,
                section,
                locale,
                schema_version,
            },
            revision,
            state: parse_state(&state),
            payload: payload_json
                .filter(|raw| !raw.is_empty())
                .map(|raw| serde_json::from_str(&raw))
                .transpose()
                .map_err(|error| InfrastructureError::Sqlite(format!("payload json: {error}")))?,
            metadata: metadata_json
                .filter(|raw| !raw.is_empty())
                .map(|raw| serde_json::from_str(&raw))
                .transpose()
                .map_err(|error| InfrastructureError::Sqlite(format!("metadata json: {error}")))?,
            error,
            reviewed: reviewed != 0,
        })
    }

    fn load_rows(&self, key: &EnrichmentKey) -> Result<Vec<EnrichmentRecord>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT entity_type, entity_id, section, locale, schema_version, revision,
                        state, payload_json, metadata_json, error, reviewed
                 FROM history_enrichment
                 WHERE entity_type=?1 AND entity_id=?2 AND section=?3 AND locale=?4 AND schema_version=?5",
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        let mut rows = statement.query(rusqlite::params![
            key.entity_type,
            key.entity_id,
            key.section.as_str(),
            key.locale,
            key.schema_version,
        ])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(Self::row_to_record(row)?);
        }
        Ok(records)
    }

    fn delete(&self, key: &EnrichmentKey, revision: u32) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "DELETE FROM history_enrichment
                 WHERE entity_type=?1 AND entity_id=?2 AND section=?3 AND locale=?4
                   AND schema_version=?5 AND revision=?6",
                rusqlite::params![
                    key.entity_type,
                    key.entity_id,
                    key.section.as_str(),
                    key.locale,
                    key.schema_version,
                    revision,
                ],
            )
            .map_err(|source| InfrastructureError::Sqlite(source.to_string()))?;
        Ok(())
    }
}

impl EnrichmentSqliteStore {
    pub fn load_best(&self, key: &EnrichmentKey) -> Result<Option<EnrichmentRecord>, String> {
        let records = self.load_rows(key).map_err(|error| error.to_string())?;
        if records.is_empty() {
            return Ok(None);
        }
        // 已审定行优先（最高 revision）；否则最高 revision。
        Ok(records
            .iter()
            .filter(|record| record.reviewed)
            .max_by_key(|record| record.revision)
            .or_else(|| records.iter().max_by_key(|record| record.revision))
            .cloned())
    }

    pub fn load_revision(
        &self,
        key: &EnrichmentKey,
        revision: u32,
    ) -> Result<Option<EnrichmentRecord>, String> {
        Ok(self
            .load_rows(key)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|record| record.revision == revision))
    }

    pub fn list_revisions(&self, key: &EnrichmentKey) -> Result<Vec<EnrichmentRecord>, String> {
        let mut records = self.load_rows(key).map_err(|error| error.to_string())?;
        records.sort_by_key(|record| record.revision);
        Ok(records)
    }

    pub fn next_revision(&self, key: &EnrichmentKey) -> Result<u32, String> {
        Ok(self
            .load_rows(key)
            .map_err(|error| error.to_string())?
            .iter()
            .map(|record| record.revision)
            .max()
            .unwrap_or(0)
            + 1)
    }

    pub fn put(&self, record: &EnrichmentRecord) -> Result<(), String> {
        self.insert(record).map_err(|error| error.to_string())
    }

    pub fn mark_reviewed(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        self.connection
            .execute(
                "UPDATE history_enrichment SET reviewed=1
                 WHERE entity_type=?1 AND entity_id=?2 AND section=?3 AND locale=?4
                   AND schema_version=?5 AND revision=?6",
                rusqlite::params![
                    key.entity_type,
                    key.entity_id,
                    key.section.as_str(),
                    key.locale,
                    key.schema_version,
                    revision,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn delete_revision(&self, key: &EnrichmentKey, revision: u32) -> Result<(), String> {
        self.delete(key, revision).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_enrichment::EnrichmentSqliteStore;
    use devtoolbox_core::history_enrichment::{
        EnrichmentClaim, EnrichmentMetadata, EnrichmentPayload, EnrichmentSection,
    };

    fn key() -> EnrichmentKey {
        EnrichmentKey::new("event", "zunyi_meeting", EnrichmentSection::Overview, "zh-CN")
    }

    fn ready_record(key: EnrichmentKey, revision: u32) -> EnrichmentRecord {
        EnrichmentRecord {
            key: key.clone(),
            revision,
            state: EnrichmentState::Ready,
            payload: Some(EnrichmentPayload {
                section: "overview".into(),
                content: "内容".into(),
                claims: vec![EnrichmentClaim { text: "x".into(), source_ids: vec!["u".into()] }],
                uncertainties: vec![],
                controversies: vec![],
            }),
            metadata: Some(EnrichmentMetadata {
                generated_at: 1,
                refreshed_at: 1,
                model: Some("m".into()),
                provider: Some("p".into()),
                prompt_version: "v".into(),
                schema_version: 1,
                canonical_revision: Some("rev".into()),
                source_ids: vec!["u".into()],
                generation_count: revision,
            }),
            error: None,
            reviewed: false,
        }
    }

    #[test]
    fn round_trip_and_best_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = EnrichmentSqliteStore::open(directory.path().join("enrichment.db")).unwrap();
        let key = key();
        store.put(&ready_record(key.clone(), 1)).unwrap();
        store.put(&ready_record(key.clone(), 2)).unwrap();
        let best = store.load_best(&key).unwrap().unwrap();
        assert_eq!(best.revision, 2);
        let revisions = store.list_revisions(&key).unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(store.next_revision(&key).unwrap(), 3);
    }

    #[test]
    fn reviewed_row_wins_over_higher_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = EnrichmentSqliteStore::open(directory.path().join("enrichment.db")).unwrap();
        let key = key();
        store.put(&ready_record(key.clone(), 1)).unwrap();
        store.mark_reviewed(&key, 1).unwrap();
        let best = store.load_best(&key).unwrap().unwrap();
        assert!(best.reviewed);
        assert_eq!(best.revision, 1);
    }

    #[test]
    fn delete_revision_removes_row() {
        let directory = tempfile::tempdir().unwrap();
        let store = EnrichmentSqliteStore::open(directory.path().join("enrichment.db")).unwrap();
        let key = key();
        store.put(&ready_record(key.clone(), 1)).unwrap();
        store.delete_revision(&key, 1).unwrap();
        assert!(store.load_best(&key).unwrap().is_none());
    }
}