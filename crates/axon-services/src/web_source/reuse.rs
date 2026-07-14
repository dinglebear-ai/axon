use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::boundary::{DocumentCache, Result as BoundaryResult};

#[derive(Debug, Clone)]
pub(super) struct ReusedWebDocument {
    pub(super) document: SourceDocument,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InProcessWebDocumentCache;

pub(crate) fn document_cache_boundary() -> Arc<dyn DocumentCache> {
    Arc::new(InProcessWebDocumentCache)
}

fn cache() -> &'static Mutex<BTreeMap<DocumentCacheKey, CachedDocument>> {
    static CACHE: OnceLock<Mutex<BTreeMap<DocumentCacheKey, CachedDocument>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn cache_documents(
    source_id: &SourceId,
    generation: &SourceGenerationId,
    documents: &[SourceDocument],
) {
    let mut cache = cache()
        .lock()
        .expect("web source reuse cache mutex poisoned");
    for document in documents {
        cache.insert(
            DocumentCacheKey {
                source_id: source_id.clone(),
                source_item_key: document.source_item_key.clone(),
                generation: Some(generation.clone()),
            },
            CachedDocument {
                document: document.clone(),
                cached_at: timestamp(),
            },
        );
    }
}

#[async_trait]
impl DocumentCache for InProcessWebDocumentCache {
    async fn get(&self, key: DocumentCacheKey) -> BoundaryResult<Option<CachedDocument>> {
        Ok(cache()
            .lock()
            .expect("web source reuse cache mutex poisoned")
            .get(&key)
            .cloned())
    }

    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> BoundaryResult<()> {
        cache()
            .lock()
            .expect("web source reuse cache mutex poisoned")
            .insert(key, value);
        Ok(())
    }

    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> BoundaryResult<()> {
        let mut cache = cache()
            .lock()
            .expect("web source reuse cache mutex poisoned");
        match selector {
            DocumentCacheInvalidation::Key { key } => {
                cache.remove(&key);
            }
            DocumentCacheInvalidation::Source { source_id } => {
                cache.retain(|key, _| key.source_id != source_id);
            }
            DocumentCacheInvalidation::Generation { generation } => {
                cache.retain(|key, _| key.generation.as_ref() != Some(&generation));
            }
            DocumentCacheInvalidation::All => cache.clear(),
        }
        Ok(())
    }

    async fn reset(&self) -> BoundaryResult<()> {
        cache()
            .lock()
            .expect("web source reuse cache mutex poisoned")
            .clear();
        Ok(())
    }

    async fn capabilities(&self) -> BoundaryResult<DocumentCacheCapability> {
        Ok(CapabilityBase {
            name: "web-reuse-cache".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-services".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["in-process".to_string()],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

pub(super) fn load_reused_web_document(
    source_id: &SourceId,
    previous_generation: Option<&SourceGenerationId>,
    item_key: &SourceItemKey,
    next_generation: &SourceGenerationId,
) -> Option<ReusedWebDocument> {
    let generation = previous_generation?;
    let cached = cache()
        .lock()
        .expect("web source reuse cache mutex poisoned")
        .get(&DocumentCacheKey {
            source_id: source_id.clone(),
            source_item_key: item_key.clone(),
            generation: Some(generation.clone()),
        })
        .cloned()?;
    Some(ReusedWebDocument {
        document: retarget_document(cached.document, next_generation),
    })
}

#[cfg(test)]
pub(super) fn evict_document(
    source_id: &SourceId,
    generation: &SourceGenerationId,
    item_key: &SourceItemKey,
) {
    cache()
        .lock()
        .expect("web source reuse cache mutex poisoned")
        .remove(&DocumentCacheKey {
            source_id: source_id.clone(),
            source_item_key: item_key.clone(),
            generation: Some(generation.clone()),
        });
}

#[cfg(test)]
pub(super) fn reset_cache() {
    cache()
        .lock()
        .expect("web source reuse cache mutex poisoned")
        .clear();
}

fn retarget_document(
    mut document: SourceDocument,
    _next_generation: &SourceGenerationId,
) -> SourceDocument {
    document.metadata.remove("source_generation");
    document.metadata.remove("committed_generation");
    document
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}
