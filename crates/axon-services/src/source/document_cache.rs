use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::boundary::{DocumentCache, Result as BoundaryResult};

const MAX_CACHED_DOCUMENTS: usize = 1024;
const MAX_CACHED_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Default)]
struct DocumentCacheState {
    entries: BTreeMap<DocumentCacheKey, CachedDocument>,
    total_bytes: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct InProcessDocumentCache {
    state: Arc<Mutex<DocumentCacheState>>,
}

impl InProcessDocumentCache {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DocumentCacheState::default())),
        }
    }
}

#[async_trait]
impl DocumentCache for InProcessDocumentCache {
    async fn get(&self, key: DocumentCacheKey) -> BoundaryResult<Option<CachedDocument>> {
        Ok(self
            .state
            .lock()
            .expect("source document cache mutex poisoned")
            .entries
            .get(&key)
            .cloned())
    }

    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> BoundaryResult<()> {
        let value_bytes = estimated_cached_document_bytes(&value);
        let mut state = self
            .state
            .lock()
            .expect("source document cache mutex poisoned");
        if let Some(previous) = state.entries.insert(key, value) {
            state.total_bytes = state
                .total_bytes
                .saturating_sub(estimated_cached_document_bytes(&previous));
        }
        state.total_bytes = state.total_bytes.saturating_add(value_bytes);
        enforce_cache_limits(&mut state);
        Ok(())
    }

    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> BoundaryResult<()> {
        let mut state = self
            .state
            .lock()
            .expect("source document cache mutex poisoned");
        let recalculate = match selector {
            DocumentCacheInvalidation::Key { key } => {
                if let Some(previous) = state.entries.remove(&key) {
                    state.total_bytes = state
                        .total_bytes
                        .saturating_sub(estimated_cached_document_bytes(&previous));
                }
                false
            }
            DocumentCacheInvalidation::Source { source_id } => {
                state.entries.retain(|key, _| key.source_id != source_id);
                true
            }
            DocumentCacheInvalidation::Generation { generation } => {
                state
                    .entries
                    .retain(|key, _| key.generation.as_ref() != Some(&generation));
                true
            }
            DocumentCacheInvalidation::All => {
                state.entries.clear();
                state.total_bytes = 0;
                false
            }
        };
        if recalculate {
            state.total_bytes = state
                .entries
                .values()
                .map(estimated_cached_document_bytes)
                .sum();
        }
        Ok(())
    }

    async fn reset(&self) -> BoundaryResult<()> {
        let mut state = self
            .state
            .lock()
            .expect("source document cache mutex poisoned");
        state.entries.clear();
        state.total_bytes = 0;
        Ok(())
    }

    async fn capabilities(&self) -> BoundaryResult<DocumentCacheCapability> {
        let mut limits = MetadataMap::new();
        limits.insert(
            "max_cached_documents".to_string(),
            serde_json::json!(MAX_CACHED_DOCUMENTS),
        );
        limits.insert(
            "max_cached_document_bytes".to_string(),
            serde_json::json!(MAX_CACHED_DOCUMENT_BYTES),
        );
        Ok(CapabilityBase {
            name: "source-document-cache".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-services".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["in-process".to_string(), "bounded".to_string()],
            limits,
        }
        .into())
    }
}

fn enforce_cache_limits(state: &mut DocumentCacheState) {
    if state.entries.len() <= MAX_CACHED_DOCUMENTS && state.total_bytes <= MAX_CACHED_DOCUMENT_BYTES
    {
        return;
    }
    let mut entries = state
        .entries
        .iter()
        .map(|(key, value)| {
            (
                value.cached_at.0.clone(),
                key.clone(),
                estimated_cached_document_bytes(value),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    for (_, key, bytes) in entries {
        if state.entries.len() <= MAX_CACHED_DOCUMENTS
            && state.total_bytes <= MAX_CACHED_DOCUMENT_BYTES
        {
            break;
        }
        if state.entries.remove(&key).is_some() {
            state.total_bytes = state.total_bytes.saturating_sub(bytes);
        }
    }
}

fn estimated_cached_document_bytes(value: &CachedDocument) -> usize {
    value.cached_at.0.len() + estimated_document_bytes(&value.document)
}

fn estimated_document_bytes(document: &SourceDocument) -> usize {
    document.source_id.0.len()
        + document.source_item_key.0.len()
        + document.canonical_uri.len()
        + document
            .mime_type
            .as_deref()
            .map(str::len)
            .unwrap_or_default()
        + document
            .metadata
            .iter()
            .map(|(key, value)| key.len() + value.to_string().len())
            .sum::<usize>()
        + match &document.content {
            ContentRef::InlineText { text } => text.len(),
            ContentRef::InlineBytes {
                bytes_base64,
                mime_type,
            } => bytes_base64.len() + mime_type.len(),
            ContentRef::Artifact { artifact_id } => artifact_id.0.len(),
            ContentRef::External { uri, integrity } => {
                uri.len() + integrity.as_deref().map(str::len).unwrap_or_default()
            }
        }
}
