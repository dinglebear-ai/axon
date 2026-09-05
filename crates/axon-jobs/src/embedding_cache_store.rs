//! SQLite persistence adapter for `axon-embedding`'s cache boundary.

use async_trait::async_trait;
use axon_api::source::{
    CacheStoreError, CachedEmbedding, CorruptCacheEntry, EmbeddingCacheLookup,
    EmbeddingVectorCacheStore, ProviderId,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::scheduler::SqliteWriteGate;

// SQLite builds commonly admit at least 999 variables. Staying below 900 also
// leaves room for fixed binds and makes behavior independent of compile flags.
const KEY_BIND_BUDGET: usize = 900;
const WRITE_BINDS_PER_ROW: usize = 7;
const WRITE_ROW_BUDGET: usize = KEY_BIND_BUDGET / WRITE_BINDS_PER_ROW;
const WRITE_ADMISSION_TIMEOUT: Duration = Duration::from_millis(250);
// Embeddings contain derived source information. Since cache keys deliberately
// carry no source provenance (one vector may be shared by many sources), a
// source-scoped purge cannot be correct. A fixed, non-refreshing creation TTL
// bounds reuse after every source reference is deleted. Physical removal is
// lazy: requested expired rows are retired and the amortized writer sweep
// removes unreferenced expired rows on its next maintenance cadence.
const MAX_CACHE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
#[cfg(not(test))]
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(test)]
const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(20);
const MAINTENANCE_DELETE_BUDGET: i64 = 512;

#[derive(Clone)]
pub struct SqliteEmbeddingVectorCacheStore {
    inner: Arc<CacheStoreInner>,
}

struct CacheStoreInner {
    pool: SqlitePool,
    write_gate: SqliteWriteGate,
    max_entries: AtomicUsize,
}

impl SqliteEmbeddingVectorCacheStore {
    pub fn new(pool: SqlitePool, write_gate: SqliteWriteGate, max_entries: usize) -> Self {
        Self::build(pool, write_gate, max_entries, true)
    }

    fn build(
        pool: SqlitePool,
        write_gate: SqliteWriteGate,
        max_entries: usize,
        start_maintenance: bool,
    ) -> Self {
        let inner = Arc::new(CacheStoreInner {
            pool,
            write_gate,
            max_entries: AtomicUsize::new(max_entries.max(1)),
        });
        if start_maintenance {
            spawn_maintenance(&inner);
        }
        Self { inner }
    }

    #[cfg(test)]
    fn new_without_maintenance(
        pool: SqlitePool,
        write_gate: SqliteWriteGate,
        max_entries: usize,
    ) -> Self {
        Self::build(pool, write_gate, max_entries, false)
    }
}

#[async_trait]
impl EmbeddingVectorCacheStore for SqliteEmbeddingVectorCacheStore {
    async fn get_many(
        &self,
        keys: &[String],
        expected_dimensions: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError> {
        let mut lookup = EmbeddingCacheLookup::default();
        for key_chunk in keys.chunks(KEY_BIND_BUDGET) {
            if key_chunk.is_empty() {
                continue;
            }
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT cache_key, provider_id, model, dimensions, vector \
                 , created_at FROM embedding_vector_cache WHERE cache_key IN (",
            );
            push_key_bind_list(&mut query, key_chunk);
            for row in query.build().fetch_all(&self.inner.pool).await? {
                let key: String = row.try_get("cache_key")?;
                let created_at: i64 = row.try_get("created_at")?;
                if created_at < cache_cutoff_millis() {
                    lookup.corrupt_entries.push(CorruptCacheEntry {
                        cache_key: key,
                        created_at,
                    });
                    continue;
                }
                let dimensions: i64 = row.try_get("dimensions")?;
                let bytes: Vec<u8> = row.try_get("vector")?;
                let Ok(dimensions) = u32::try_from(dimensions) else {
                    lookup.corrupt_entries.push(CorruptCacheEntry {
                        cache_key: key,
                        created_at,
                    });
                    continue;
                };
                let Some(values) = decode_vector(&bytes, dimensions) else {
                    lookup.corrupt_entries.push(CorruptCacheEntry {
                        cache_key: key,
                        created_at,
                    });
                    continue;
                };
                if dimensions != expected_dimensions {
                    lookup.corrupt_entries.push(CorruptCacheEntry {
                        cache_key: key,
                        created_at,
                    });
                    continue;
                }
                lookup.observed_created_at.insert(key.clone(), created_at);
                lookup.hits.insert(
                    key.clone(),
                    CachedEmbedding {
                        cache_key: key,
                        provider_id: ProviderId::new(row.try_get::<String, _>("provider_id")?),
                        model: row.try_get("model")?,
                        dimensions,
                        values,
                    },
                );
            }
        }
        Ok(lookup)
    }

    async fn touch_many(&self, keys: &[String]) -> Result<(), CacheStoreError> {
        if keys.is_empty() {
            return Ok(());
        }
        let Some(_write_permit) = self.inner.write_gate.try_lock() else {
            // LRU accuracy is advisory. A warm cache hit must never queue
            // behind source/job mutations on the shared SQLite writer gate.
            metrics::counter!(
                "axon_embedding_cache_touch_skipped_total",
                "reason" => "writer_busy"
            )
            .increment(1);
            return Ok(());
        };
        let mut transaction = self.inner.pool.begin().await?;
        let now = chrono::Utc::now().timestamp_millis();
        for key_chunk in keys.chunks(KEY_BIND_BUDGET - 1) {
            let mut query =
                QueryBuilder::<Sqlite>::new("UPDATE embedding_vector_cache SET last_used_at = ");
            query.push_bind(now);
            query.push(", hit_count = hit_count + 1 WHERE cache_key IN (");
            push_key_bind_list(&mut query, key_chunk);
            query.build().execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn put_many(
        &self,
        entries: &[CachedEmbedding],
        max_entries: usize,
    ) -> Result<(), CacheStoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let _write_permit = acquire_write_permit(&self.inner.write_gate).await?;
        let mut transaction = self.inner.pool.begin().await?;
        let now = chrono::Utc::now().timestamp_millis();
        for entry_chunk in entries.chunks(WRITE_ROW_BUDGET) {
            let valid = entry_chunk
                .iter()
                .filter(|entry| {
                    entry.values.len() == entry.dimensions as usize
                        && entry.values.iter().all(|value| value.is_finite())
                })
                .collect::<Vec<_>>();
            if valid.is_empty() {
                continue;
            }
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO embedding_vector_cache \
                 (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) ",
            );
            query.push_values(valid, |mut row, entry| {
                row.push_bind(&entry.cache_key)
                    .push_bind(&entry.provider_id.0)
                    .push_bind(&entry.model)
                    .push_bind(i64::from(entry.dimensions))
                    .push_bind(encode_vector(&entry.values))
                    .push_bind(now)
                    .push_bind(now);
            });
            query.push(
                " ON CONFLICT(cache_key) DO UPDATE SET \
                 provider_id = excluded.provider_id, model = excluded.model, \
                 dimensions = excluded.dimensions, vector = excluded.vector, \
                 created_at = CASE WHEN embedding_vector_cache.created_at < ",
            );
            query.push_bind(now.saturating_sub(MAX_CACHE_AGE.as_millis() as i64));
            query.push(
                " OR embedding_vector_cache.provider_id != excluded.provider_id \
                     OR embedding_vector_cache.model != excluded.model \
                     OR embedding_vector_cache.dimensions != excluded.dimensions \
                     OR embedding_vector_cache.vector != excluded.vector \
                     THEN MAX(excluded.created_at, embedding_vector_cache.created_at + 1) \
                     ELSE embedding_vector_cache.created_at END, \
                     last_used_at = excluded.last_used_at",
            );
            query.build().execute(&mut *transaction).await?;
        }
        self.inner
            .max_entries
            .store(max_entries.max(1), Ordering::Relaxed);
        prune(&mut transaction, max_entries, now).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn retire_many(&self, entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let _write_permit = acquire_write_permit(&self.inner.write_gate).await?;
        let mut transaction = self.inner.pool.begin().await?;
        for entry_chunk in entries.chunks(KEY_BIND_BUDGET / 2) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "DELETE FROM embedding_vector_cache WHERE (cache_key, created_at) IN (",
            );
            let mut separated = query.separated(", ");
            for entry in entry_chunk {
                separated
                    .push_unseparated("(")
                    .push_bind_unseparated(&entry.cache_key)
                    .push_unseparated(", ")
                    .push_bind_unseparated(entry.created_at)
                    .push_unseparated(")");
            }
            separated.push_unseparated(")");
            query.build().execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

fn push_key_bind_list<'a>(query: &mut QueryBuilder<'a, Sqlite>, keys: &'a [String]) {
    let mut separated = query.separated(", ");
    for key in keys {
        separated.push_bind(key);
    }
    separated.push_unseparated(")");
}

async fn prune(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    max_entries: usize,
    now: i64,
) -> Result<(), sqlx::Error> {
    let max_entries = i64::try_from(max_entries).unwrap_or(i64::MAX).max(1);
    let cutoff = now.saturating_sub(MAX_CACHE_AGE.as_millis() as i64);
    sqlx::query(
        "DELETE FROM embedding_vector_cache WHERE cache_key IN (\
         SELECT cache_key FROM embedding_vector_cache WHERE created_at < ? \
         ORDER BY created_at ASC, cache_key ASC LIMIT ?)",
    )
    .bind(cutoff)
    .bind(MAINTENANCE_DELETE_BUDGET)
    .execute(&mut **transaction)
    .await?;
    let count: Option<i64> = sqlx::query_scalar(
        "SELECT entry_count FROM embedding_vector_cache_state WHERE singleton = 1",
    )
    .fetch_optional(&mut **transaction)
    .await?;
    let count = match count {
        Some(count) => count,
        None => {
            // Self-heal a missing state singleton (external drift, manual DB
            // surgery) by recomputing the exact count instead of failing every
            // cache write forever; the triggers keep it maintained afterwards.
            sqlx::query(
                "INSERT OR IGNORE INTO embedding_vector_cache_state (singleton, entry_count)
                 SELECT 1, COUNT(*) FROM embedding_vector_cache",
            )
            .execute(&mut **transaction)
            .await?;
            sqlx::query_scalar(
                "SELECT entry_count FROM embedding_vector_cache_state WHERE singleton = 1",
            )
            .fetch_one(&mut **transaction)
            .await?
        }
    };
    // `max_entries` is a soft bound: each pass deletes at most
    // MAINTENANCE_DELETE_BUDGET rows, so a burst that overshoots by more than
    // the budget drains over subsequent put/maintenance passes by design.
    let victims = count
        .saturating_sub(max_entries)
        .clamp(0, MAINTENANCE_DELETE_BUDGET);
    if victims == 0 {
        return Ok(());
    }
    sqlx::query(
        "DELETE FROM embedding_vector_cache WHERE cache_key IN (\
         SELECT cache_key FROM embedding_vector_cache \
         ORDER BY last_used_at ASC, cache_key ASC LIMIT ?)",
    )
    .bind(victims)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn spawn_maintenance(inner: &Arc<CacheStoreInner>) {
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + MAINTENANCE_INTERVAL;
        let mut interval = tokio::time::interval_at(start, MAINTENANCE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(inner) = weak.upgrade() else {
                break;
            };
            run_periodic_maintenance(&inner).await;
        }
    });
}

async fn run_periodic_maintenance(inner: &CacheStoreInner) {
    let Ok(_permit) = tokio::time::timeout(WRITE_ADMISSION_TIMEOUT, inner.write_gate.lock()).await
    else {
        metrics::counter!("axon_embedding_cache_maintenance_skipped_total", "reason" => "writer_busy")
            .increment(1);
        return;
    };
    let mut transaction = match inner.pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            record_maintenance_failure("begin", &error);
            return;
        }
    };
    let max_entries = inner.max_entries.load(Ordering::Relaxed);
    match prune(
        &mut transaction,
        max_entries,
        chrono::Utc::now().timestamp_millis(),
    )
    .await
    {
        Ok(()) => {
            if let Err(error) = transaction.commit().await {
                record_maintenance_failure("commit", &error);
            }
        }
        Err(error) => {
            record_maintenance_failure("prune", &error);
            if let Err(rollback_error) = transaction.rollback().await {
                record_maintenance_failure("rollback", &rollback_error);
            }
        }
    }
}

fn record_maintenance_failure(stage: &'static str, error: &sqlx::Error) {
    metrics::counter!("axon_embedding_cache_maintenance_failures_total", "stage" => stage)
        .increment(1);
    tracing::warn!(stage, %error, "embedding cache maintenance pass failed");
}

fn cache_cutoff_millis() -> i64 {
    chrono::Utc::now()
        .timestamp_millis()
        .saturating_sub(MAX_CACHE_AGE.as_millis() as i64)
}

async fn acquire_write_permit(
    gate: &SqliteWriteGate,
) -> Result<tokio::sync::MutexGuard<'_, ()>, CacheStoreError> {
    tokio::time::timeout(WRITE_ADMISSION_TIMEOUT, gate.lock())
        .await
        .map_err(|_| "embedding cache SQLite writer admission timed out".into())
}

fn encode_vector(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size_of_val(values));
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_vector(bytes: &[u8], dimensions: u32) -> Option<Vec<f32>> {
    if bytes.len() != dimensions as usize * size_of::<f32>() {
        return None;
    }
    let values = bytes
        .chunks_exact(size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
        .collect::<Vec<_>>();
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

#[cfg(test)]
#[path = "embedding_cache_store_tests.rs"]
mod tests;
