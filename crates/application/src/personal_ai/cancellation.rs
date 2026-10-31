//! Per-request cancellation for an Agent run; distinct from its conversation ID.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use devtoolbox_core::AgentError;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

const MAX_ACTIVE_RUNS: usize = 64;

#[derive(Clone, Default)]
pub struct AgentRunRegistry {
    runs: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AgentRunRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Run one future under a validated caller-owned correlation ID. Dropping the
    /// provider/tool future on cancellation allows reqwest and async tools to stop.
    pub async fn run<F, T>(&self, trace_id: &str, future: F) -> Result<T, AgentError>
    where
        F: Future<Output = Result<T, AgentError>> + Send,
    {
        if trace_id.is_empty() || trace_id.len() > 128 || trace_id.chars().any(char::is_control) {
            return Err(AgentError::tool_invalid_argument("invalid AI trace ID"));
        }
        let token = CancellationToken::new();
        {
            let mut runs = self.runs.lock();
            if runs.contains_key(trace_id) {
                return Err(AgentError::tool_invalid_argument(
                    "AI trace ID is already active",
                ));
            }
            if runs.len() >= MAX_ACTIVE_RUNS {
                return Err(AgentError::tool_execution_failed("AI run capacity is full"));
            }
            runs.insert(trace_id.to_string(), token.clone());
        }
        let result = tokio::select! {
            _ = token.cancelled() => Err(AgentError::cancelled("AI request was cancelled")),
            result = future => result,
        };
        self.runs.lock().remove(trace_id);
        result
    }

    /// Cancel only an active run with this exact trace ID.
    pub fn cancel(&self, trace_id: &str) -> bool {
        let token = self.runs.lock().get(trace_id).cloned();
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.runs.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn cancellation_drops_run_future_and_releases_registry_entry() {
        let registry = AgentRunRegistry::new();
        let run_registry = registry.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            run_registry
                .run("run-1", async move {
                    let _ = started_tx.send(());
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok::<_, AgentError>(())
                })
                .await
        });
        started_rx.await.unwrap();
        assert_eq!(registry.active_count(), 1);
        assert!(registry.cancel("run-1"));
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code(), "personal_ai_cancelled");
        assert_eq!(registry.active_count(), 0);
        assert!(!registry.cancel("run-1"));
    }

    #[tokio::test]
    async fn duplicate_or_invalid_run_ids_are_rejected_without_cancelling_other_runs() {
        let registry = AgentRunRegistry::new();
        let run_registry = registry.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            run_registry
                .run("active", async move {
                    let _ = started_tx.send(());
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok::<_, AgentError>(())
                })
                .await
        });
        started_rx.await.unwrap();
        let duplicate = registry
            .run("active", async { Ok::<_, AgentError>(()) })
            .await
            .unwrap_err();
        assert_eq!(duplicate.code(), "personal_ai_tool_invalid_argument");
        let invalid = registry
            .run("bad\nid", async { Ok::<_, AgentError>(()) })
            .await
            .unwrap_err();
        assert_eq!(invalid.code(), "personal_ai_tool_invalid_argument");
        registry.cancel("active");
        task.await.unwrap().unwrap_err();
    }
}
