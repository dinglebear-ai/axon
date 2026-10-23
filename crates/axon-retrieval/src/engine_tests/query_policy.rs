use super::*;
use axon_api::source::{
    CacheStoreError, CachedEmbedding, CorruptCacheEntry, EmbeddingCacheLookup,
    EmbeddingVectorCacheStore, InstructionSupport,
};
use axon_embedding::cache::CachedEmbeddingProvider;
use std::collections::HashMap;
use tokio::sync::Mutex;

const MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";

struct RecordingProvider {
    fake: FakeEmbeddingProvider,
    support: InstructionSupport,
}

#[async_trait]
impl EmbeddingProvider for RecordingProvider {
    async fn embed(&self, mut batch: EmbeddingBatch) -> EmbeddingProviderResult<EmbeddingResult> {
        batch.model = "fake-embedding".to_string();
        let mut result = self.fake.embed(batch).await?;
        result.model = MODEL.to_string();
        Ok(result)
    }
    async fn capabilities(&self) -> EmbeddingProviderResult<ProviderCapability> {
        let mut capability = self.fake.capabilities().await?;
        let embedding = capability
            .embedding
            .as_mut()
            .expect("fake embedding capability");
        embedding.model_id = MODEL.to_string();
        embedding.instruction_support = self.support;
        Ok(capability)
    }
}

#[derive(Default)]
struct RecordingCache(Mutex<HashMap<String, CachedEmbedding>>);

#[async_trait]
impl EmbeddingVectorCacheStore for RecordingCache {
    async fn get_many(
        &self,
        keys: &[String],
        _: u32,
    ) -> Result<EmbeddingCacheLookup, CacheStoreError> {
        let entries = self.0.lock().await;
        Ok(EmbeddingCacheLookup {
            hits: keys
                .iter()
                .filter_map(|key| entries.get(key).cloned().map(|v| (key.clone(), v)))
                .collect(),
            ..Default::default()
        })
    }
    async fn touch_many(&self, _: &[String]) -> Result<(), CacheStoreError> {
        Ok(())
    }
    async fn put_many(&self, entries: &[CachedEmbedding], _: usize) -> Result<(), CacheStoreError> {
        self.0
            .lock()
            .await
            .extend(entries.iter().map(|v| (v.cache_key.clone(), v.clone())));
        Ok(())
    }
    async fn retire_many(&self, entries: &[CorruptCacheEntry]) -> Result<(), CacheStoreError> {
        let mut stored = self.0.lock().await;
        for entry in entries {
            stored.remove(&entry.cache_key);
        }
        Ok(())
    }
}

fn document_batch(text: &str) -> EmbeddingBatch {
    EmbeddingBatch {
        batch_id: BatchId::new(Uuid::new_v4()),
        job_id: JobId::new(Uuid::new_v4()),
        provider_id: ProviderId::new("fake-embedding"),
        model: MODEL.to_string(),
        items: vec![EmbeddingInput {
            chunk_id: ChunkId::new("document"),
            text: text.to_string(),
            content_kind: ContentKind::PlainText,
            metadata: MetadataMap::new(),
        }],
        instruction: None,
        priority: JobPriority::Interactive,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn query_policy_controls_actual_provider_text_and_cache_identity() {
    let query = "Where is address renewal implemented?";
    let prefixed = format!(
        "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:{query}"
    );
    for (support, expected, expected_calls) in [
        (InstructionSupport::None, query, 1),
        (InstructionSupport::QueryOnly, prefixed.as_str(), 2),
    ] {
        let inner = Arc::new(RecordingProvider {
            fake: FakeEmbeddingProvider::new("fake-embedding", 4),
            support,
        });
        let cache = Arc::new(RecordingCache::default());
        let provider = Arc::new(CachedEmbeddingProvider::new(
            inner.clone(),
            cache.clone(),
            "query-policy-test",
            ProviderId::new("fake-embedding"),
            MODEL,
            4,
            support,
            100,
        ));
        // Ordinary documents reach the provider unchanged, even when query instructions are enabled.
        provider.embed(document_batch(query)).await.unwrap();
        assert_eq!(inner.fake.calls().await[0].items[0].text, query);
        let store = Arc::new(FakeVectorStore::new("fake-vectors"));
        store
            .ensure_collection(test_collection_spec_hybrid(4))
            .await
            .unwrap();
        let config = RetrievalEngineConfig::new(
            ProviderId::new("fake-embedding"),
            MODEL,
            4,
            RetrievalAccess::standard(),
        );
        let engine = RetrievalEngine::new(store, provider.clone(), config);
        let mut request = request();
        request.query = query.to_string();
        engine.retrieve(request.clone()).await.unwrap();
        engine.retrieve(request).await.unwrap();
        let calls = inner.fake.calls().await;
        assert_eq!(calls.len(), expected_calls, "{support:?}");
        assert_eq!(calls.last().unwrap().items[0].text, expected);
        assert!(calls.iter().all(|batch| batch.instruction.is_none()));
        // A document carrying exactly the effective query input reuses its cache entry:
        // no hidden instruction or query-only cache identity differs from actual text.
        provider.embed(document_batch(expected)).await.unwrap();
        assert_eq!(inner.fake.calls().await.len(), expected_calls);
        assert_eq!(cache.0.lock().await.len(), expected_calls);
    }
}
