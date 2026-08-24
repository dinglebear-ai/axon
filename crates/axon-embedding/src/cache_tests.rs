use std::sync::Arc;
use std::{future::pending, time::Duration};

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
};
use tokio::sync::Mutex;

use super::*;
use crate::fake::FakeEmbeddingProvider;

#[derive(Default)]
struct MemoryCacheStore {
    entries: Mutex<HashMap<String, CachedEmbedding>>,
    retired: Mutex<Vec<String>>,
    fail_touch: bool,
    corrupt: Mutex<Vec<CorruptCacheEntry>>,
}

struct FailingCacheStore;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockedOperation {
    Read,
    Touch,
    Retire,
    Write,
}

struct BlockingCacheStore {
    blocked: BlockedOperation,
    lookup: EmbeddingCacheLookup,
}

#[async_trait]
impl EmbeddingVectorCacheStore for BlockingCacheStore {
    async fn get_many(
        &self,
        _keys: &[String],
        _expected_dimensions: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError> {
        if self.blocked == BlockedOperation::Read {
            return pending().await;
        }
        Ok(EmbeddingCacheLookup {
            hits: self.lookup.hits.clone(),
            observed_created_at: self.lookup.observed_created_at.clone(),
            corrupt_entries: self.lookup.corrupt_entries.clone(),
        })
    }

    async fn touch_many(&self, _keys: &[String]) -> Result<(), CacheStoreError> {
        if self.blocked == BlockedOperation::Touch {
            return pending().await;
        }
        Ok(())
    }

    async fn put_many(
        &self,
        _entries: &[CachedEmbedding],
        _max_entries: usize,
    ) -> Result<(), CacheStoreError> {
        if self.blocked == BlockedOperation::Write {
            return pending().await;
        }
        Ok(())
    }

    async fn retire_many(&self, _entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError> {
        if self.blocked == BlockedOperation::Retire {
            return pending().await;
        }
        Ok(())
    }
}

#[async_trait]
impl EmbeddingVectorCacheStore for FailingCacheStore {
    async fn get_many(
        &self,
        _keys: &[String],
        _expected_dimensions: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError> {
        Err("cache unavailable".into())
    }

    async fn touch_many(&self, _keys: &[String]) -> Result<(), CacheStoreError> {
        Err("cache unavailable".into())
    }

    async fn put_many(
        &self,
        _entries: &[CachedEmbedding],
        _max_entries: usize,
    ) -> Result<(), CacheStoreError> {
        Err("cache unavailable".into())
    }

    async fn retire_many(&self, _entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError> {
        Err("cache unavailable".into())
    }
}

#[async_trait]
impl EmbeddingVectorCacheStore for MemoryCacheStore {
    async fn get_many(
        &self,
        keys: &[String],
        _expected_dimensions: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError> {
        let entries = self.entries.lock().await;
        Ok(EmbeddingCacheLookup {
            hits: keys
                .iter()
                .filter_map(|key| entries.get(key).cloned().map(|entry| (key.clone(), entry)))
                .collect(),
            observed_created_at: HashMap::new(),
            corrupt_entries: self.corrupt.lock().await.clone(),
        })
    }

    async fn touch_many(&self, _keys: &[String]) -> Result<(), CacheStoreError> {
        if self.fail_touch {
            return Err("touch failed".into());
        }
        Ok(())
    }

    async fn put_many(
        &self,
        entries: &[CachedEmbedding],
        max_entries: usize,
    ) -> Result<(), CacheStoreError> {
        let mut stored = self.entries.lock().await;
        for entry in entries {
            stored.insert(entry.cache_key.clone(), entry.clone());
        }
        while stored.len() > max_entries {
            let key = stored.keys().next().cloned().expect("entry");
            stored.remove(&key);
        }
        Ok(())
    }

    async fn retire_many(&self, entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError> {
        self.retired
            .lock()
            .await
            .extend(entries.iter().map(|entry| entry.cache_key.clone()));
        let mut stored = self.entries.lock().await;
        for entry in entries {
            stored.remove(&entry.cache_key);
        }
        Ok(())
    }
}

fn batch(texts: &[&str]) -> EmbeddingBatch {
    EmbeddingBatch {
        batch_id: BatchId::new(uuid::Uuid::from_u128(1)),
        job_id: JobId::new(uuid::Uuid::from_u128(2)),
        provider_id: ProviderId::new("tei"),
        model: "fake-embedding".to_string(),
        items: texts
            .iter()
            .enumerate()
            .map(|(index, text)| EmbeddingInput {
                chunk_id: ChunkId::new(format!("chunk-{index}")),
                text: (*text).to_string(),
                content_kind: ContentKind::Markdown,
                metadata: MetadataMap::new(),
            })
            .collect(),
        instruction: Some("document: ".to_string()),
        priority: JobPriority::Normal,
        metadata: MetadataMap::new(),
    }
}

fn provider(
    fake: Arc<FakeEmbeddingProvider>,
    store: Arc<MemoryCacheStore>,
) -> CachedEmbeddingProvider {
    CachedEmbeddingProvider::new(
        fake,
        store,
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        100_000,
    )
}

fn provider_with_store(
    fake: Arc<FakeEmbeddingProvider>,
    store: Arc<dyn EmbeddingVectorCacheStore>,
) -> CachedEmbeddingProvider {
    CachedEmbeddingProvider::new(
        fake,
        store,
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        100_000,
    )
}

fn cached_entry(key: String, values: Vec<f32>) -> CachedEmbedding {
    CachedEmbedding {
        cache_key: key,
        provider_id: ProviderId::new("tei"),
        model: "fake-embedding".into(),
        dimensions: 4,
        values,
    }
}

/// Inner provider that returns valid vectors in reversed order, simulating a
/// provider that violates the order-preservation contract.
struct ReorderingProvider {
    inner: Arc<FakeEmbeddingProvider>,
}

#[async_trait]
impl EmbeddingProvider for ReorderingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult, ApiError> {
        let mut result = self.inner.embed(batch).await?;
        result.vectors.reverse();
        Ok(result)
    }

    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        self.inner.capabilities().await
    }
}

#[tokio::test]
async fn repeated_inputs_are_cached_and_deduplicated_in_original_order() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), store);

    let first = cached
        .embed(batch(&["alpha", "beta", "alpha"]))
        .await
        .unwrap();
    let second = cached.embed(batch(&["beta", "alpha"])).await.unwrap();

    let calls = fake.calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].items.len(), 2);
    assert_eq!(first.vectors[0].values, first.vectors[2].values);
    assert_eq!(second.vectors[0].values, first.vectors[1].values);
    assert_eq!(second.usage.requests, 0);
}

#[tokio::test]
async fn empty_batch_fails_like_the_raw_provider_without_touching_cache_or_provider() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), store);

    let error = cached.embed(batch(&[])).await.unwrap_err();

    assert_eq!(error.code.0, "embedding.batch_empty");
    assert!(fake.calls().await.is_empty());
}

#[tokio::test]
async fn fully_cached_batch_still_rejects_blank_and_duplicate_items() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), Arc::clone(&store));
    // Warm the cache so "alpha" is a guaranteed hit.
    cached.embed(batch(&["alpha"])).await.unwrap();
    assert_eq!(fake.calls().await.len(), 1);

    let mut blank = batch(&["alpha", "   "]);
    blank.items[1].chunk_id = ChunkId::new("chunk-blank");
    let error = cached.embed(blank).await.unwrap_err();
    assert_eq!(error.code.0, "embedding.blank_text");

    let mut duplicate = batch(&["alpha", "alpha"]);
    duplicate.items[1].chunk_id = duplicate.items[0].chunk_id.clone();
    let error = cached.embed(duplicate).await.unwrap_err();
    assert_eq!(error.code.0, "embedding.duplicate_chunk_id");

    // Validation must fire before any provider call, regardless of warmth.
    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test]
async fn reordered_provider_vectors_fail_closed_and_cache_nothing() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let reordering = Arc::new(ReorderingProvider {
        inner: Arc::clone(&fake),
    });
    let cached = CachedEmbeddingProvider::new(
        reordering,
        Arc::clone(&store) as Arc<dyn EmbeddingVectorCacheStore>,
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        100_000,
    );

    let error = cached.embed(batch(&["alpha", "beta"])).await.unwrap_err();

    assert_eq!(error.code.0, "embedding.cache.result_misaligned");
    assert_eq!(fake.calls().await.len(), 1);
    tokio::task::yield_now().await;
    assert!(
        store.entries.lock().await.is_empty(),
        "a mis-aligned provider result must not populate the cache"
    );
}

#[tokio::test]
async fn full_cache_hit_reports_unmetered_usage() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), Arc::clone(&store));
    cached.embed(batch(&["alpha"])).await.unwrap();

    let warm = cached.embed(batch(&["alpha"])).await.unwrap();

    assert_eq!(fake.calls().await.len(), 1);
    assert_eq!(warm.usage.requests, 0);
    assert_eq!(
        warm.usage.input_tokens, None,
        "a full cache hit is unmetered and must not report Some(0)"
    );
}

#[tokio::test]
async fn failed_touch_preserves_valid_hits() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let warm = provider(Arc::clone(&fake), Arc::clone(&store));
    warm.embed(batch(&["alpha"])).await.unwrap();

    let failing_store = Arc::new(MemoryCacheStore {
        entries: Mutex::new(store.entries.lock().await.clone()),
        fail_touch: true,
        ..MemoryCacheStore::default()
    });
    provider(Arc::clone(&fake), failing_store)
        .embed(batch(&["alpha"]))
        .await
        .unwrap();

    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test]
async fn unavailable_store_fails_open_to_the_provider() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = CachedEmbeddingProvider::new(
        Arc::clone(&fake) as Arc<dyn EmbeddingProvider>,
        Arc::new(FailingCacheStore),
        "http://tei.test",
        ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        100,
    );

    assert!(cached.embed(batch(&["alpha"])).await.is_ok());
    assert!(cached.embed(batch(&["alpha"])).await.is_ok());
    assert_eq!(fake.calls().await.len(), 2);
}

#[tokio::test]
async fn corrupt_identity_is_retired_and_refetched() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), Arc::clone(&store));
    let request = batch(&["alpha"]);
    let key = cache_key(
        "http://tei.test",
        &ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        &request,
        &request.items[0],
    );
    store.entries.lock().await.insert(
        key.clone(),
        CachedEmbedding {
            cache_key: key.clone(),
            provider_id: ProviderId::new("wrong-provider"),
            model: "fake-embedding".into(),
            dimensions: 4,
            values: vec![0.0; 4],
        },
    );

    cached.embed(request).await.unwrap();

    assert_eq!(fake.calls().await.len(), 1);
    assert_eq!(store.retired.lock().await.as_slice(), &[key]);
}

#[tokio::test]
async fn non_finite_cached_vector_is_retired_and_refetched() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let store = Arc::new(MemoryCacheStore::default());
    let cached = provider(Arc::clone(&fake), Arc::clone(&store));
    let request = batch(&["alpha"]);
    let key = cache_key(
        "http://tei.test",
        &ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        &request,
        &request.items[0],
    );
    store.entries.lock().await.insert(
        key.clone(),
        cached_entry(key.clone(), vec![0.0, f32::NAN, 1.0, f32::INFINITY]),
    );

    let result = cached.embed(request).await.unwrap();

    assert!(
        result.vectors[0]
            .values
            .iter()
            .all(|value| value.is_finite())
    );
    assert_eq!(fake.calls().await.len(), 1);
    assert_eq!(store.retired.lock().await.as_slice(), &[key]);
}

#[tokio::test(start_paused = true)]
async fn stalled_read_times_out_and_fails_open_to_provider() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider_with_store(
        Arc::clone(&fake),
        Arc::new(BlockingCacheStore {
            blocked: BlockedOperation::Read,
            lookup: EmbeddingCacheLookup::default(),
        }),
    );

    let result = tokio::time::timeout(Duration::from_secs(1), cached.embed(batch(&["alpha"])))
        .await
        .expect("optional cache read must be bounded")
        .unwrap();

    assert_eq!(result.vectors.len(), 1);
    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn stalled_touch_times_out_without_discarding_a_valid_hit() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let request = batch(&["alpha"]);
    let key = cache_key(
        "http://tei.test",
        &ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        &request,
        &request.items[0],
    );
    let cached = provider_with_store(
        Arc::clone(&fake),
        Arc::new(BlockingCacheStore {
            blocked: BlockedOperation::Touch,
            lookup: EmbeddingCacheLookup {
                hits: HashMap::from([(key.clone(), cached_entry(key, vec![1.0; 4]))]),
                observed_created_at: HashMap::new(),
                corrupt_entries: Vec::new(),
            },
        }),
    );

    let result = tokio::time::timeout(Duration::from_secs(1), cached.embed(request))
        .await
        .expect("optional cache touch must be bounded")
        .unwrap();

    assert_eq!(result.vectors[0].values, vec![1.0; 4]);
    assert!(fake.calls().await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn stalled_retirement_times_out_and_refetches_the_corrupt_row() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let request = batch(&["alpha"]);
    let key = cache_key(
        "http://tei.test",
        &ProviderId::new("tei"),
        "fake-embedding",
        4,
        InstructionSupport::QueryAndDocument,
        &request,
        &request.items[0],
    );
    let cached = provider_with_store(
        Arc::clone(&fake),
        Arc::new(BlockingCacheStore {
            blocked: BlockedOperation::Retire,
            lookup: EmbeddingCacheLookup {
                hits: HashMap::new(),
                observed_created_at: HashMap::new(),
                corrupt_entries: vec![CorruptCacheEntry {
                    cache_key: key,
                    created_at: 1,
                }],
            },
        }),
    );

    let result = tokio::time::timeout(Duration::from_secs(1), cached.embed(request))
        .await
        .expect("optional corrupt-row retirement must be bounded")
        .unwrap();

    assert_eq!(result.vectors.len(), 1);
    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn stalled_write_times_out_without_discarding_the_provider_result() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider_with_store(
        Arc::clone(&fake),
        Arc::new(BlockingCacheStore {
            blocked: BlockedOperation::Write,
            lookup: EmbeddingCacheLookup::default(),
        }),
    );

    let result = tokio::time::timeout(Duration::from_secs(1), cached.embed(batch(&["alpha"])))
        .await
        .expect("optional cache write must be bounded")
        .unwrap();

    assert_eq!(result.vectors.len(), 1);
    assert_eq!(fake.calls().await.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn timed_out_mutation_is_detached_instead_of_cancelled() {
    let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());
    let completed_in_task = Arc::clone(&completed);
    let release_in_task = Arc::clone(&release);

    run_detached_mutation(
        "write",
        1,
        Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .unwrap(),
        async move {
            release_in_task.notified().await;
            completed_in_task.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .await;

    assert!(!completed.load(std::sync::atomic::Ordering::SeqCst));
    release.notify_one();
    tokio::task::yield_now().await;
    assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn detached_mutations_are_bounded_per_provider() {
    let fake = Arc::new(FakeEmbeddingProvider::new("tei", 4));
    let cached = provider_with_store(
        Arc::clone(&fake),
        Arc::new(BlockingCacheStore {
            blocked: BlockedOperation::Write,
            lookup: EmbeddingCacheLookup::default(),
        }),
    );

    cached.embed(batch(&["one"])).await.unwrap();
    assert_eq!(cached.mutation_slots.available_permits(), 1);
    cached.embed(batch(&["two"])).await.unwrap();
    assert_eq!(cached.mutation_slots.available_permits(), 0);
    cached.embed(batch(&["three"])).await.unwrap();
    assert_eq!(cached.mutation_slots.available_permits(), 0);
}

#[test]
fn cache_identity_uses_only_the_effective_instruction() {
    let request = batch(&["same"]);
    let input = &request.items[0];
    let provider = ProviderId::new("tei");
    let enabled = cache_key(
        "authority",
        &provider,
        "model",
        4,
        InstructionSupport::QueryAndDocument,
        &request,
        input,
    );
    let disabled = cache_key(
        "authority",
        &provider,
        "model",
        4,
        InstructionSupport::None,
        &request,
        input,
    );
    let mut no_instruction = request.clone();
    no_instruction.instruction = None;
    let disabled_without_instruction = cache_key(
        "authority",
        &provider,
        "model",
        4,
        InstructionSupport::None,
        &no_instruction,
        input,
    );

    assert_ne!(enabled, disabled);
    assert_eq!(disabled, disabled_without_instruction);
}
