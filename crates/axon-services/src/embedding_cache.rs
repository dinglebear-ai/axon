use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axon_api::source::{
    ApiError, EmbeddingBatch, EmbeddingResult, EmbeddingVector, ProviderCapability, ProviderId,
    ProviderUsage,
};
use axon_embedding::provider::EmbeddingProvider;
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

const CACHE_KEY_VERSION: &str = "embedding-vector-cache-v1";

#[derive(Clone)]
pub(crate) struct CachedEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
    pool: SqlitePool,
    authority: String,
    response_provider_id: ProviderId,
    model: String,
    dimensions: u32,
    max_entries: i64,
}

struct CachedVector {
    provider_id: ProviderId,
    values: Vec<f32>,
}

impl CachedEmbeddingProvider {
    pub(crate) fn new(
        inner: Arc<dyn EmbeddingProvider>,
        pool: SqlitePool,
        authority: impl Into<String>,
        response_provider_id: ProviderId,
        model: impl Into<String>,
        dimensions: u32,
        max_entries: usize,
    ) -> Self {
        Self {
            inner,
            pool,
            authority: authority.into(),
            response_provider_id,
            model: model.into(),
            dimensions,
            max_entries: i64::try_from(max_entries).unwrap_or(i64::MAX).max(1),
        }
    }

    async fn read(&self, keys: &[String]) -> Result<HashMap<String, CachedVector>, sqlx::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT cache_key, provider_id, dimensions, vector FROM embedding_vector_cache \
             WHERE cache_key IN (",
        );
        let mut separated = query.separated(", ");
        for key in keys {
            separated.push_bind(key);
        }
        separated.push_unseparated(")");
        let rows = query.build().fetch_all(&self.pool).await?;
        let mut cached = HashMap::with_capacity(rows.len());
        let mut hit_keys = Vec::with_capacity(rows.len());
        for row in rows {
            let key: String = row.get("cache_key");
            let dimensions: i64 = row.get("dimensions");
            let blob: Vec<u8> = row.get("vector");
            if dimensions != i64::from(self.dimensions) {
                continue;
            }
            let Some(values) = decode_vector(&blob, self.dimensions) else {
                continue;
            };
            hit_keys.push(key.clone());
            cached.insert(
                key,
                CachedVector {
                    provider_id: ProviderId::new(row.get::<String, _>("provider_id")),
                    values,
                },
            );
        }
        self.touch(&hit_keys).await?;
        Ok(cached)
    }

    async fn touch(&self, keys: &[String]) -> Result<(), sqlx::Error> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut query =
            QueryBuilder::<Sqlite>::new("UPDATE embedding_vector_cache SET last_used_at = ");
        query.push_bind(now_millis());
        query.push(", hit_count = hit_count + 1 WHERE cache_key IN (");
        let mut separated = query.separated(", ");
        for key in keys {
            separated.push_bind(key);
        }
        separated.push_unseparated(")");
        query.build().execute(&self.pool).await?;
        Ok(())
    }

    async fn write(&self, keys: &[String], result: &EmbeddingResult) -> Result<(), sqlx::Error> {
        if keys.is_empty() || result.vectors.is_empty() {
            return Ok(());
        }
        let now = now_millis();
        let rows = keys
            .iter()
            .zip(&result.vectors)
            .filter(|(_, vector)| vector.values.len() == self.dimensions as usize)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Ok(());
        }
        // One statement avoids an awaited SQLite round trip per vector. A TEI
        // batch is bounded well below SQLite's parameter limit (seven binds per
        // row), so the entire provider result can be committed atomically.
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO embedding_vector_cache \
             (cache_key, provider_id, model, dimensions, vector, created_at, last_used_at) ",
        );
        query.push_values(rows, |mut row, (key, vector)| {
            row.push_bind(key)
                .push_bind(&result.provider_id.0)
                .push_bind(&result.model)
                .push_bind(i64::from(result.dimensions))
                .push_bind(encode_vector(&vector.values))
                .push_bind(now)
                .push_bind(now);
        });
        query.push(
            " ON CONFLICT(cache_key) DO UPDATE SET \
             provider_id = excluded.provider_id, model = excluded.model, \
             dimensions = excluded.dimensions, vector = excluded.vector, \
             last_used_at = excluded.last_used_at",
        );
        query.build().execute(&self.pool).await?;
        self.prune().await
    }

    async fn prune(&self) -> Result<(), sqlx::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM embedding_vector_cache")
            .fetch_one(&self.pool)
            .await?;
        let excess = count.saturating_sub(self.max_entries).max(0);
        if excess == 0 {
            return Ok(());
        }
        sqlx::query(
            "DELETE FROM embedding_vector_cache WHERE cache_key IN (\
             SELECT cache_key FROM embedding_vector_cache \
             ORDER BY last_used_at ASC, cache_key ASC LIMIT ?)",
        )
        .bind(excess)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl EmbeddingProvider for CachedEmbeddingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult, ApiError> {
        let keys = batch
            .items
            .iter()
            .map(|item| {
                cache_key(
                    &self.authority,
                    &self.response_provider_id,
                    &self.model,
                    self.dimensions,
                    &batch,
                    item,
                )
            })
            .collect::<Vec<_>>();
        let cached = match self.read(&keys).await {
            Ok(cached) => cached,
            Err(error) => {
                tracing::warn!(%error, "embedding cache read failed; using provider");
                HashMap::new()
            }
        };

        let mut unique_miss_keys = Vec::new();
        let mut seen = HashSet::new();
        let mut miss_items = Vec::new();
        for (key, item) in keys.iter().zip(&batch.items) {
            if !cached.contains_key(key) && seen.insert(key.clone()) {
                unique_miss_keys.push(key.clone());
                miss_items.push(item.clone());
            }
        }
        metrics::counter!("axon_embedding_cache_hits_total")
            .increment((keys.len() - miss_items.len()) as u64);
        metrics::counter!("axon_embedding_cache_misses_total").increment(miss_items.len() as u64);

        let miss_result = if miss_items.is_empty() {
            None
        } else {
            let mut miss_batch = batch.clone();
            miss_batch.items = miss_items;
            let result = self.inner.embed(miss_batch).await?;
            if let Err(error) = self.write(&unique_miss_keys, &result).await {
                tracing::warn!(%error, "embedding cache write failed; returning provider result");
            }
            Some(result)
        };

        let mut resolved = cached;
        if let Some(result) = &miss_result {
            for (key, vector) in unique_miss_keys.iter().zip(&result.vectors) {
                resolved.insert(
                    key.clone(),
                    CachedVector {
                        provider_id: result.provider_id.clone(),
                        values: vector.values.clone(),
                    },
                );
            }
        }
        let vectors = keys
            .iter()
            .zip(&batch.items)
            .map(|(key, item)| {
                let values = resolved.get(key).ok_or_else(|| {
                    ApiError::new(
                        "embedding.cache.result_missing",
                        axon_error::ErrorStage::Embedding,
                        "embedding cache/provider result did not cover every input",
                    )
                })?;
                Ok(EmbeddingVector {
                    chunk_id: item.chunk_id.clone(),
                    values: values.values.clone(),
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;

        let provider_id = miss_result
            .as_ref()
            .map(|result| result.provider_id.clone())
            .or_else(|| {
                resolved
                    .values()
                    .next()
                    .map(|entry| entry.provider_id.clone())
            })
            .unwrap_or_else(|| self.response_provider_id.clone());
        let usage = miss_result
            .as_ref()
            .map(|result| result.usage.clone())
            .unwrap_or(ProviderUsage {
                input_tokens: Some(0),
                output_tokens: None,
                requests: 0,
                duration_ms: 0,
            });
        let warnings = miss_result
            .as_ref()
            .map(|result| result.warnings.clone())
            .unwrap_or_default();
        Ok(EmbeddingResult {
            batch_id: batch.batch_id,
            job_id: batch.job_id,
            provider_id,
            model: self.model.clone(),
            dimensions: self.dimensions,
            vectors,
            usage,
            warnings,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.inner.capabilities().await
    }
}

fn cache_key(
    authority: &str,
    provider_id: &ProviderId,
    model: &str,
    dimensions: u32,
    batch: &EmbeddingBatch,
    input: &axon_api::source::EmbeddingInput,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        CACHE_KEY_VERSION.as_bytes(),
        authority.as_bytes(),
        provider_id.0.as_bytes(),
        model.as_bytes(),
        &dimensions.to_le_bytes(),
        batch.instruction.as_deref().unwrap_or("").as_bytes(),
        serde_json::to_string(&input.content_kind)
            .expect("content kind serializes")
            .as_bytes(),
        input.text.as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn encode_vector(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * size_of::<f32>());
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_vector(bytes: &[u8], dimensions: u32) -> Option<Vec<f32>> {
    if bytes.len() != dimensions as usize * size_of::<f32>() {
        return None;
    }
    Some(
        bytes
            .chunks_exact(size_of::<f32>())
            .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
            .collect(),
    )
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "embedding_cache_tests.rs"]
mod tests;
