use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use axon_api::source::*;

#[derive(Debug, Clone)]
pub(super) struct ReusedWebDocument {
    pub(super) document: SourceDocument,
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
