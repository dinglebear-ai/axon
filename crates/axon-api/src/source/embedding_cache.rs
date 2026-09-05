//! Provider-neutral dense embedding cache persistence contracts.

use std::collections::HashMap;

use async_trait::async_trait;

use super::ProviderId;

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
