use crate::crates::jobs::lite::ops::cancel_row;
use dashmap::DashMap;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Thread-safe map of in-flight job IDs to their [`CancellationToken`]s.
///
/// Cancellation is in-process only. Cross-process cancellation via SQLite
/// polling is not implemented in lite mode (single-process assumption).
pub struct CancelStore {
    tokens: DashMap<Uuid, CancellationToken>,
}

impl CancelStore {
    pub fn new() -> Self {
        Self {
            tokens: DashMap::new(),
        }
    }

    /// Register a new cancellation token for a job. Returns a clone for the job to hold.
    pub fn register(&self, id: Uuid) -> CancellationToken {
        let token = CancellationToken::new();
        self.tokens.insert(id, token.clone());
        token
    }

    /// Remove a job's token (call when job reaches any terminal state).
    pub fn remove(&self, id: Uuid) {
        self.tokens.remove(&id);
    }

    /// Cancel a job: update the DB row AND fire the in-memory token.
    pub async fn cancel(
        &self,
        id: Uuid,
        pool: &SqlitePool,
        table: &str,
    ) -> Result<bool, sqlx::Error> {
        let updated = cancel_row(pool, table, id).await?;
        if let Some(entry) = self.tokens.get(&id) {
            entry.value().cancel();
        }
        Ok(updated)
    }
}

impl Default for CancelStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;
    use std::sync::Arc;

    #[tokio::test]
    async fn cancel_token_fires_immediately_for_same_process() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        let id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .unwrap();

        let store = Arc::new(CancelStore::new());
        let token = store.register(id);

        store.cancel(id, &pool, "axon_crawl_jobs").await.unwrap();

        assert!(token.is_cancelled(), "token should be cancelled");
    }
}
