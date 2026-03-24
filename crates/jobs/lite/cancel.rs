use crate::crates::jobs::lite::ops::cancel_row;
use dashmap::DashMap;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Thread-safe map of in-flight job IDs to their CancellationTokens.
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

/// Background loop: polls SQLite every `interval` for jobs whose status was set to 'canceled'
/// from another process. Fires the in-memory CancellationToken when detected.
pub async fn poll_sqlite_for_cancels(
    pool: &Arc<SqlitePool>,
    store: &Arc<CancelStore>,
    jobs: &[(&str, Uuid)],
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;

        for &(table, id) in jobs {
            if !store.tokens.contains_key(&id) {
                continue;
            }

            let row: Option<(String,)> =
                sqlx::query_as(&format!("SELECT status FROM {} WHERE id = ?", table))
                    .bind(id.to_string())
                    .fetch_optional(pool.as_ref())
                    .await
                    .unwrap_or(None);

            if let Some((status,)) = row {
                if status == "canceled" {
                    if let Some(entry) = store.tokens.get(&id) {
                        entry.value().cancel();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;

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

    #[tokio::test]
    async fn sqlite_poll_fires_token_on_row_update() {
        let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
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

        // Simulate cross-process cancel: update the row directly
        sqlx::query("UPDATE axon_crawl_jobs SET status='canceled' WHERE id=?")
            .bind(id.to_string())
            .execute(pool.as_ref())
            .await
            .unwrap();

        // Start poll loop with a short interval
        let store2 = Arc::clone(&store);
        let pool2 = Arc::clone(&pool);
        tokio::spawn(async move {
            poll_sqlite_for_cancels(
                &pool2,
                &store2,
                &[("axon_crawl_jobs", id)],
                Duration::from_millis(50),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(token.is_cancelled(), "token should be fired by SQLite poll");
    }
}
