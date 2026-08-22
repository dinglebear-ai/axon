//! SQLite persistence adapter for `axon-embedding`'s cache boundary.

use async_trait::async_trait;
use axon_api::source::ProviderId;
use axon_embedding::cache::{
    CacheStoreError, CachedEmbedding, EmbeddingCacheLookup, EmbeddingVectorCacheStore,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use crate::scheduler::SqliteWriteGate;

// SQLite builds commonly admit at least 999 variables. Staying below 900 also
// leaves room for fixed binds and makes behavior independent of compile flags.
const KEY_BIND_BUDGET: usize = 900;
const WRITE_BINDS_PER_ROW: usize = 7;
const WRITE_ROW_BUDGET: usize = KEY_BIND_BUDGET / WRITE_BINDS_PER_ROW;

#[derive(Clone)]
pub struct SqliteEmbeddingVectorCacheStore {
    pool: SqlitePool,
    write_gate: SqliteWriteGate,
}

impl SqliteEmbeddingVectorCacheStore {
    pub fn new(pool: SqlitePool, write_gate: SqliteWriteGate) -> Self {
        Self { pool, write_gate }
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
                 FROM embedding_vector_cache WHERE cache_key IN (",
            );
            push_key_bind_list(&mut query, key_chunk);
            for row in query.build().fetch_all(&self.pool).await? {
                let key: String = row.try_get("cache_key")?;
                let dimensions: i64 = row.try_get("dimensions")?;
                let bytes: Vec<u8> = row.try_get("vector")?;
                let Ok(dimensions) = u32::try_from(dimensions) else {
                    lookup.corrupt_keys.push(key);
                    continue;
                };
                let Some(values) = decode_vector(&bytes, dimensions) else {
                    lookup.corrupt_keys.push(key);
                    continue;
                };
                if dimensions != expected_dimensions {
                    lookup.corrupt_keys.push(key);
                    continue;
                }
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
        let _write_permit = self.write_gate.lock().await;
        let mut transaction = self.pool.begin().await?;
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
        let _write_permit = self.write_gate.lock().await;
        let mut transaction = self.pool.begin().await?;
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
                 last_used_at = excluded.last_used_at",
            );
            query.build().execute(&mut *transaction).await?;
        }
        prune(&mut transaction, max_entries).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn retire_many(&self, keys: &[String]) -> Result<(), CacheStoreError> {
        if keys.is_empty() {
            return Ok(());
        }
        let _write_permit = self.write_gate.lock().await;
        let mut transaction = self.pool.begin().await?;
        for key_chunk in keys.chunks(KEY_BIND_BUDGET) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "DELETE FROM embedding_vector_cache WHERE cache_key IN (",
            );
            push_key_bind_list(&mut query, key_chunk);
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
) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
        .fetch_one(&mut **transaction)
        .await?;
    let max_entries = i64::try_from(max_entries).unwrap_or(i64::MAX).max(1);
    let excess = count.saturating_sub(max_entries).max(0);
    if excess > 0 {
        sqlx::query(
            "DELETE FROM embedding_vector_cache WHERE cache_key IN (\
             SELECT cache_key FROM embedding_vector_cache \
             ORDER BY last_used_at ASC, cache_key ASC LIMIT ?)",
        )
        .bind(excess)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
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
