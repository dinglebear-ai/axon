//! Provider-level dense embedding cache policy.
//!
//! Persistence stays behind [`EmbeddingVectorCacheStore`]; this crate owns the
//! cache identity, provider decoration, fail-open behavior, and result ordering.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, EmbeddingBatch, EmbeddingResult, EmbeddingVector, InstructionSupport,
    ProviderCapability, ProviderId, ProviderUsage,
};
use sha2::{Digest, Sha256};

use crate::batch::validate_batch;
use crate::provider::EmbeddingProvider;

const CACHE_KEY_VERSION: &str = "embedding-vector-cache-v1";
// Cache persistence is optional and local. It must never add an unbounded wait
// to a provider request when SQLite is busy or its pool is saturated.
//
// Accepted tradeoff: fail-open is outcome-open, not latency-open. With SQLite
// fully saturated, one `embed` call can still stall inline for up to ~3x this
// bound (~750 ms worst case: one bounded read plus up to two detachment waits)
// before the provider request proceeds.
const OPTIONAL_CACHE_OPERATION_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_OUTSTANDING_CACHE_MUTATIONS: usize = 2;

pub type CacheStoreError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
pub struct CachedEmbedding {
    pub cache_key: String,
    pub provider_id: ProviderId,
    pub model: String,
    pub dimensions: u32,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorruptCacheEntry {
    pub cache_key: String,
    pub created_at: i64,
}

#[derive(Debug, Default)]
pub struct EmbeddingCacheLookup {
    pub hits: HashMap<String, CachedEmbedding>,
    pub observed_created_at: HashMap<String, i64>,
    pub corrupt_entries: Vec<CorruptCacheEntry>,
}

#[async_trait]
pub trait EmbeddingVectorCacheStore: Send + Sync {
    async fn get_many(
        &self,
        keys: &[String],
        expected_dimensions: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError>;

    async fn touch_many(&self, keys: &[String]) -> Result<(), CacheStoreError>;

    async fn put_many(
        &self,
        entries: &[CachedEmbedding],
        max_entries: usize,
    ) -> Result<(), CacheStoreError>;

    async fn retire_many(&self, entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError>;
}

#[derive(Clone)]
pub struct CachedEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
    store: Arc<dyn EmbeddingVectorCacheStore>,
    authority: String,
    response_provider_id: ProviderId,
    model: String,
    dimensions: u32,
    instruction_support: InstructionSupport,
    max_entries: usize,
    mutation_slots: Arc<tokio::sync::Semaphore>,
}

impl CachedEmbeddingProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        inner: Arc<dyn EmbeddingProvider>,
        store: Arc<dyn EmbeddingVectorCacheStore>,
        authority: impl Into<String>,
        response_provider_id: ProviderId,
        model: impl Into<String>,
        dimensions: u32,
        instruction_support: InstructionSupport,
        max_entries: usize,
    ) -> Self {
        Self {
            inner,
            store,
            authority: authority.into(),
            response_provider_id,
            model: model.into(),
            dimensions,
            instruction_support,
            max_entries: max_entries.max(1),
            mutation_slots: Arc::new(tokio::sync::Semaphore::new(MAX_OUTSTANDING_CACHE_MUTATIONS)),
        }
    }

    async fn lookup(&self, keys: &[String]) -> HashMap<String, CachedEmbedding> {
        let unique_keys = keys
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut lookup = match bounded_store_operation(
            "read",
            unique_keys.len(),
            self.store.get_many(&unique_keys, self.dimensions),
        )
        .await
        {
            Some(Ok(lookup)) => lookup,
            Some(Err(error)) => {
                record_store_error("read", unique_keys.len(), &error);
                return HashMap::new();
            }
            None => return HashMap::new(),
        };

        let invalid_identity = lookup
            .hits
            .iter()
            .filter(|(_, entry)| {
                entry.provider_id != self.response_provider_id
                    || entry.model != self.model
                    || entry.dimensions != self.dimensions
                    || entry.values.len() != self.dimensions as usize
                    || entry.values.iter().any(|value| !value.is_finite())
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in &invalid_identity {
            lookup.hits.remove(key);
        }
        lookup
            .corrupt_entries
            .extend(invalid_identity.into_iter().map(|cache_key| {
                CorruptCacheEntry {
                    created_at: lookup
                        .observed_created_at
                        .get(&cache_key)
                        .copied()
                        .unwrap_or_default(),
                    cache_key,
                }
            }));
        lookup
            .corrupt_entries
            .sort_by(|a, b| a.cache_key.cmp(&b.cache_key));
        lookup
            .corrupt_entries
            .dedup_by(|a, b| a.cache_key == b.cache_key);

        if !lookup.corrupt_entries.is_empty() {
            metrics::counter!("axon_embedding_cache_corrupt_rows_total")
                .increment(lookup.corrupt_entries.len() as u64);
            let store = Arc::clone(&self.store);
            let entries = lookup.corrupt_entries.clone();
            if let Ok(slot) = Arc::clone(&self.mutation_slots).try_acquire_owned() {
                run_detached_mutation("retire", entries.len(), slot, async move {
                    store.retire_many(&entries).await
                })
                .await;
            } else {
                record_mutation_saturated("retire", entries.len());
            }
        }

        let hit_keys = lookup.hits.keys().cloned().collect::<Vec<_>>();
        if !hit_keys.is_empty() {
            let store = Arc::clone(&self.store);
            if let Ok(slot) = Arc::clone(&self.mutation_slots).try_acquire_owned() {
                spawn_observed_mutation("touch", hit_keys.len(), slot, async move {
                    store.touch_many(&hit_keys).await
                });
            } else {
                record_mutation_saturated("touch", hit_keys.len());
            }
        }
        lookup.hits
    }

    async fn fetch_misses(
        &self,
        batch: &EmbeddingBatch,
        keys: &[String],
        cached: &HashMap<String, CachedEmbedding>,
    ) -> Result<(Vec<String>, Option<EmbeddingResult>), ApiError> {
        let mut unique_miss_keys = Vec::new();
        let mut seen = HashSet::new();
        let mut miss_items = Vec::new();
        for (key, item) in keys.iter().zip(&batch.items) {
            if !cached.contains_key(key) && seen.insert(key.clone()) {
                unique_miss_keys.push(key.clone());
                miss_items.push(item.clone());
            }
        }
        let cached_inputs = keys.iter().filter(|key| cached.contains_key(*key)).count();
        let deduplicated = keys
            .len()
            .saturating_sub(cached_inputs.saturating_add(miss_items.len()));
        metrics::counter!("axon_embedding_cache_deduplicated_inputs_total")
            .increment(deduplicated as u64);

        if miss_items.is_empty() {
            return Ok((unique_miss_keys, None));
        }
        let miss_chunk_ids = miss_items
            .iter()
            .map(|item| item.chunk_id.clone())
            .collect::<Vec<_>>();
        let mut miss_batch = batch.clone();
        miss_batch.items = miss_items;
        let result = self.inner.embed(miss_batch).await?;
        // Miss keys and returned vectors are zipped by position below (here and
        // in `embed`); a provider that drops, duplicates, or reorders vectors
        // would otherwise cache wrong text→vector pairs. Fail the batch closed
        // instead, caching nothing from a mis-aligned result.
        let aligned = result.vectors.len() == miss_chunk_ids.len()
            && result
                .vectors
                .iter()
                .zip(&miss_chunk_ids)
                .all(|(vector, chunk_id)| &vector.chunk_id == chunk_id);
        if !aligned {
            metrics::counter!("axon_embedding_cache_misaligned_results_total").increment(1);
            return Err(ApiError::new(
                "embedding.cache.result_misaligned",
                axon_error::ErrorStage::Embedding,
                "embedding provider returned vectors that do not align with the requested inputs",
            ));
        }
        let entries = unique_miss_keys
            .iter()
            .zip(&result.vectors)
            .filter(|(_, vector)| vector.values.len() == self.dimensions as usize)
            .map(|(key, vector)| CachedEmbedding {
                cache_key: key.clone(),
                provider_id: result.provider_id.clone(),
                model: result.model.clone(),
                dimensions: result.dimensions,
                values: vector.values.clone(),
            })
            .collect::<Vec<_>>();
        let store = Arc::clone(&self.store);
        let max_entries = self.max_entries;
        if let Ok(slot) = Arc::clone(&self.mutation_slots).try_acquire_owned() {
            run_detached_mutation("write", entries.len(), slot, async move {
                store.put_many(&entries, max_entries).await
            })
            .await;
        } else {
            record_mutation_saturated("write", entries.len());
        }
        Ok((unique_miss_keys, Some(result)))
    }
}

fn spawn_observed_mutation<F>(
    operation: &'static str,
    key_count: usize,
    slot: tokio::sync::OwnedSemaphorePermit,
    future: F,
) where
    F: Future<Output = Result<(), CacheStoreError>> + Send + 'static,
{
    let task = tokio::spawn(async move {
        let _slot = slot;
        future.await
    });
    tokio::spawn(async move {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => record_store_error(operation, key_count, &error),
            Err(error) => {
                let error: CacheStoreError = Box::new(error);
                record_store_error(operation, key_count, &error);
            }
        }
    });
}

async fn run_detached_mutation<F>(
    operation: &'static str,
    key_count: usize,
    slot: tokio::sync::OwnedSemaphorePermit,
    future: F,
) where
    F: Future<Output = Result<(), CacheStoreError>> + Send + 'static,
{
    // The spawned task owns the mutation. Timing out the optional wait detaches
    // it instead of cancelling an admitted SQLite transaction mid-rollback.
    let mut task = tokio::spawn(async move {
        let _slot = slot;
        future.await
    });
    match tokio::time::timeout(OPTIONAL_CACHE_OPERATION_TIMEOUT, &mut task).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(error))) => record_store_error(operation, key_count, &error),
        Ok(Err(error)) => {
            let error: CacheStoreError = Box::new(error);
            record_store_error(operation, key_count, &error);
        }
        Err(_) => {
            record_store_timeout(operation, key_count);
            tokio::spawn(async move {
                match task.await {
                    Ok(Ok(())) => metrics::counter!(
                        "axon_embedding_cache_detached_mutations_completed_total",
                        "operation" => operation
                    )
                    .increment(1),
                    Ok(Err(error)) => record_store_error(operation, key_count, &error),
                    Err(error) => {
                        let error: CacheStoreError = Box::new(error);
                        record_store_error(operation, key_count, &error);
                    }
                }
            });
        }
    }
}

fn record_mutation_saturated(operation: &'static str, key_count: usize) {
    metrics::counter!(
        "axon_embedding_cache_mutations_saturated_total",
        "operation" => operation
    )
    .increment(1);
    tracing::warn!(
        operation,
        key_count,
        "embedding cache mutation skipped: admission saturated"
    );
}

async fn bounded_store_operation<T, F>(
    operation: &'static str,
    key_count: usize,
    future: F,
) -> Option<Result<T, CacheStoreError>>
where
    F: Future<Output = Result<T, CacheStoreError>>,
{
    match tokio::time::timeout(OPTIONAL_CACHE_OPERATION_TIMEOUT, future).await {
        Ok(result) => Some(result),
        Err(_) => {
            record_store_timeout(operation, key_count);
            None
        }
    }
}

fn record_store_timeout(operation: &'static str, key_count: usize) {
    metrics::counter!(
        "axon_embedding_cache_store_timeouts_total",
        "operation" => operation
    )
    .increment(1);
    metrics::counter!(
        "axon_embedding_cache_store_timeout_keys_total",
        "operation" => operation
    )
    .increment(key_count as u64);
    tracing::warn!(
        operation,
        key_count,
        timeout_ms = OPTIONAL_CACHE_OPERATION_TIMEOUT.as_millis(),
        "optional embedding cache operation timed out"
    );
}

fn record_store_error(operation: &'static str, key_count: usize, error: &CacheStoreError) {
    metrics::counter!("axon_embedding_cache_store_errors_total", "operation" => operation)
        .increment(1);
    metrics::counter!(
        "axon_embedding_cache_store_error_keys_total",
        "operation" => operation
    )
    .increment(key_count as u64);
    tracing::warn!(
        operation,
        key_count,
        %error,
        "optional embedding cache operation failed"
    );
}

#[async_trait]
impl EmbeddingProvider for CachedEmbeddingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult, ApiError> {
        // The decorator must preserve the inner provider contract regardless of
        // cache warmth: empty/blank/duplicate batches fail identically whether
        // the batch would hit or miss.
        validate_batch(&batch)?;
        let keys = batch
            .items
            .iter()
            .map(|item| {
                cache_key(
                    &self.authority,
                    &self.response_provider_id,
                    &self.model,
                    self.dimensions,
                    self.instruction_support,
                    &batch,
                    item,
                )
            })
            .collect::<Vec<_>>();
        let cached = self.lookup(&keys).await;
        let hit_count = keys.iter().filter(|key| cached.contains_key(*key)).count();
        metrics::counter!("axon_embedding_cache_hits_total").increment(hit_count as u64);
        metrics::counter!("axon_embedding_cache_misses_total")
            .increment((keys.len() - hit_count) as u64);

        let (unique_miss_keys, miss_result) = self.fetch_misses(&batch, &keys, &cached).await?;

        let mut resolved = cached;
        if let Some(result) = &miss_result {
            for (key, vector) in unique_miss_keys.iter().zip(&result.vectors) {
                resolved.insert(
                    key.clone(),
                    CachedEmbedding {
                        cache_key: key.clone(),
                        provider_id: result.provider_id.clone(),
                        model: result.model.clone(),
                        dimensions: result.dimensions,
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
            // A full cache hit is unmetered; report `None` like an unmetered
            // provider response rather than `Some(0)`, so usage aggregations do
            // not treat it as a metered zero-token request.
            .unwrap_or(ProviderUsage {
                input_tokens: None,
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
    instruction_support: InstructionSupport,
    batch: &EmbeddingBatch,
    input: &axon_api::source::EmbeddingInput,
) -> String {
    let effective_instruction = match batch.instruction.as_deref() {
        Some(instruction)
            if !instruction.is_empty() && instruction_support != InstructionSupport::None =>
        {
            instruction
        }
        _ => "",
    };
    let mut hasher = Sha256::new();
    for part in [
        CACHE_KEY_VERSION.as_bytes(),
        authority.as_bytes(),
        provider_id.0.as_bytes(),
        model.as_bytes(),
        &dimensions.to_le_bytes(),
        effective_instruction.as_bytes(),
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

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
