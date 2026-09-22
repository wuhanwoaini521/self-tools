//! 动作审计存储（V7 §63-§66/§111）。
//!
//! SQLite `config/server_actions.db`（本地单文件；不引 Redis/PG）。
//! **只存结构与稳定码**（§65）：不含 API key、完整日志、完整 prompt、
//! secret 值。保留策略：条数上限 + 天数上限，先到先裁（§112）。

use std::path::Path;

use devtoolbox_core::server::{ActionOutcome, ActionRisk, AuditEntry, AuditSource};
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::InfrastructureError;

const SCHEMA_VERSION: u32 = 1;

/// 审计 SQLite 存储。
pub struct ServerActionAuditSqlite {
    connection: parking_lot::Mutex<Connection>,
}

impl ServerActionAuditSqlite {
    /// 打开（或创建）审计库。
    pub fn open_store(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        let connection = Connection::open(path).map_err(store_error)?;
        let store = Self {
            connection: parking_lot::Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .execute_batch(
                r"
                CREATE TABLE IF NOT EXISTS action_audit (
                    id TEXT PRIMARY KEY,
                    timestamp INTEGER NOT NULL,
                    source TEXT NOT NULL DEFAULT 'desktop',
                    session_id TEXT NOT NULL,
                    action_type TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    risk TEXT NOT NULL,
                    confirmed INTEGER NOT NULL,
                    result TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    error_code TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_action_audit_timestamp
                    ON action_audit(timestamp DESC);
                CREATE TABLE IF NOT EXISTS schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )
            .map_err(store_error)?;
        connection
            .execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )
            .map_err(store_error)?;
        Ok(())
    }

    /// 写入一条审计（§64）。
    pub fn record(&self, entry: &AuditEntry) -> Result<(), InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .execute(
                r"INSERT OR REPLACE INTO action_audit
                  (id, timestamp, source, session_id, action_type, target_id, risk,
                   confirmed, result, duration_ms, error_code)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    entry.id,
                    entry.timestamp,
                    entry.source.as_str(),
                    entry.session_id,
                    entry.action_type,
                    entry.target_id,
                    entry.risk.as_str(),
                    i64::from(entry.confirmed),
                    entry.result.as_str(),
                    entry.duration_ms as i64,
                    entry.error_code.clone().unwrap_or_default(),
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    /// 最近审计（§66 UI；不含任何正文）。
    pub fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>, InfrastructureError> {
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                r"SELECT id, timestamp, source, session_id, action_type, target_id, risk,
                         confirmed, result, duration_ms, error_code
                  FROM action_audit ORDER BY timestamp DESC, id DESC LIMIT ?1",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(params![limit.max(1) as i64], |row| {
                let source: String = row.get(2)?;
                let risk: String = row.get(6)?;
                let result: String = row.get(8)?;
                let error_code: String = row.get(10)?;
                Ok(AuditEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    source: AuditSource::parse(&source).unwrap_or(AuditSource::Desktop),
                    session_id: row.get(3)?,
                    action_type: row.get(4)?,
                    target_id: row.get(5)?,
                    risk: ActionRisk::parse(&risk).unwrap_or(ActionRisk::Read),
                    confirmed: row.get::<_, i64>(7)? != 0,
                    result: ActionOutcome::parse(&result).unwrap_or(ActionOutcome::Failed),
                    duration_ms: row.get::<_, i64>(9)? as u64,
                    error_code: (!error_code.is_empty()).then_some(error_code),
                })
            })
            .map_err(store_error)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(store_error)?);
        }
        Ok(entries)
    }

    /// 保留策略（§112）：超条数 / 超天数 → 裁最旧。
    pub fn prune(
        &self,
        max_entries: usize,
        retention_days: i64,
        now: i64,
    ) -> Result<usize, InfrastructureError> {
        let connection = self.connection.lock();
        let cutoff = now.saturating_sub(retention_days.max(1) * 86_400);
        let by_age = connection
            .execute(
                "DELETE FROM action_audit WHERE timestamp < ?1",
                params![cutoff],
            )
            .map_err(store_error)?;
        let by_count = if max_entries > 0 {
            connection
                .execute(
                    r"DELETE FROM action_audit WHERE id NOT IN (
                        SELECT id FROM action_audit ORDER BY timestamp DESC, id DESC LIMIT ?1
                      )",
                    params![max_entries as i64],
                )
                .map_err(store_error)?
        } else {
            0
        };
        Ok(by_age + by_count)
    }

    /// 当前条数（测试用）。
    pub fn count(&self) -> Result<usize, InfrastructureError> {
        let connection = self.connection.lock();
        connection
            .query_row("SELECT COUNT(*) FROM action_audit", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()
            .map_err(store_error)?
            .map(|count| count as usize)
            .ok_or_else(|| store_error_message("count failed"))
    }
}

fn store_error(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(format!("server audit store: {error}"))
}

fn store_error_message(message: &str) -> InfrastructureError {
    InfrastructureError::Sqlite(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, timestamp: i64) -> AuditEntry {
        AuditEntry {
            id: id.into(),
            timestamp,
            source: AuditSource::Desktop,
            session_id: "session-1".into(),
            action_type: "services.restart".into(),
            target_id: "self-tools".into(),
            risk: ActionRisk::System,
            confirmed: true,
            result: ActionOutcome::Success,
            duration_ms: 12,
            error_code: None,
        }
    }

    #[test]
    fn record_and_recent_round_trip_without_content() {
        let directory = tempfile::tempdir().unwrap();
        let store = ServerActionAuditSqlite::open_store(directory.path().join("audit.db")).unwrap();
        store.record(&entry("aud-1", 1_000)).unwrap();
        store.record(&entry("aud-2", 2_000)).unwrap();

        let recent = store.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, "aud-2", "倒序");
        assert_eq!(recent[0].risk, ActionRisk::System);
        assert_eq!(recent[0].result, ActionOutcome::Success);
        assert!(recent[0].confirmed);
    }

    #[test]
    fn prune_enforces_entry_cap_and_age() {
        let directory = tempfile::tempdir().unwrap();
        let store = ServerActionAuditSqlite::open_store(directory.path().join("audit.db")).unwrap();
        for index in 0..5 {
            store
                .record(&entry(&format!("aud-{index}"), 1_000 + index as i64))
                .unwrap();
        }
        let removed = store.prune(2, 30, 10_000).unwrap();
        assert!(removed >= 3, "条数上限裁剪: {removed}");
        assert_eq!(store.count().unwrap(), 2);

        // 超龄裁剪。
        store.record(&entry("old", 1)).unwrap();
        let removed_age = store.prune(0, 1, 1_000_000).unwrap();
        assert!(removed_age >= 1, "超龄裁剪: {removed_age}");
    }

    #[test]
    fn count_reports_entries() {
        let directory = tempfile::tempdir().unwrap();
        let store = ServerActionAuditSqlite::open_store(directory.path().join("audit.db")).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        store.record(&entry("aud-1", 1)).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }
}

/// 测试辅助：合法注册服务（供组合根测试使用）。
pub mod test_support {
    use devtoolbox_core::server::{HealthCheckKind, ServiceDescriptor, ServiceProviderType};

    /// 合法注册服务（`allowed_actions = ["restart"]`）。
    #[must_use]
    pub fn service(id: &str) -> ServiceDescriptor {
        ServiceDescriptor {
            id: id.into(),
            display_name: "Self Tools".into(),
            description: "后端".into(),
            provider_type: ServiceProviderType::Launchd,
            provider_ref: "com.example.self-tools".into(),
            health_check: HealthCheckKind::Launchd,
            allowed_actions: vec!["restart".into()],
            ..ServiceDescriptor::default()
        }
    }

    /// 合法注册应用（http URL + health_url）。
    #[must_use]
    pub fn app(id: &str) -> devtoolbox_core::server::ApplicationDescriptor {
        devtoolbox_core::server::ApplicationDescriptor {
            id: id.into(),
            name: "Self Tools".into(),
            description: "后台".into(),
            url: "http://127.0.0.1:8080".into(),
            health_url: Some("http://127.0.0.1:8080/health".into()),
            service_id: Some("self-tools".into()),
            category: "dev".into(),
            tags: Vec::new(),
        }
    }
}
