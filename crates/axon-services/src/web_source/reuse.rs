use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::boundary::{DocumentCache, Result as BoundaryResult};

const MAX_CACHED_DOCUMENTS: usize = 1024;
const MAX_CACHED_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct ReusedWebDocument {
    pub(super) document: SourceDocument,
}

#[derive(Debug, Clone)]
pub(crate) struct InProcessWebDocumentCache {
    entries: Arc<Mutex<BTreeMap<DocumentCacheKey, CachedDocument>>>,
}

impl InProcessWebDocumentCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

pub(super) async fn cache_documents(
    cache: &dyn DocumentCache,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    documents: &[SourceDocument],
) -> BoundaryResult<()> {
    for document in documents {
        cache
            .put(
                DocumentCacheKey {
                    source_id: source_id.clone(),
                    source_item_key: document.source_item_key.clone(),
                    generation: Some(generation.clone()),
                },
                CachedDocument {
                    document: document.clone(),
                    cached_at: timestamp(),
                },
            )
            .await?;
    }
    Ok(())
}

#[async_trait]
impl DocumentCache for InProcessWebDocumentCache {
    async fn get(&self, key: DocumentCacheKey) -> BoundaryResult<Option<CachedDocument>> {
        Ok(self
            .entries
            .lock()
            .expect("web source reuse cache mutex poisoned")
            .get(&key)
            .cloned())
    }

    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> BoundaryResult<()> {
        let mut entries = self
            .entries
            .lock()
            .expect("web source reuse cache mutex poisoned");
        entries.insert(key, value);
        enforce_cache_limits(&mut entries);
        Ok(())
    }

    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> BoundaryResult<()> {
        let mut entries = self
            .entries
            .lock()
            .expect("web source reuse cache mutex poisoned");
        match selector {
            DocumentCacheInvalidation::Key { key } => {
                entries.remove(&key);
            }
            DocumentCacheInvalidation::Source { source_id } => {
                entries.retain(|key, _| key.source_id != source_id);
            }
            DocumentCacheInvalidation::Generation { generation } => {
                entries.retain(|key, _| key.generation.as_ref() != Some(&generation));
            }
            DocumentCacheInvalidation::All => entries.clear(),
        }
        Ok(())
    }

    async fn reset(&self) -> BoundaryResult<()> {
        self.entries
            .lock()
            .expect("web source reuse cache mutex poisoned")
            .clear();
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
            name: "web-reuse-cache".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-services".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["in-process".to_string()],
            limits,
        }
        .into())
    }
}

pub(super) async fn load_reused_web_document(
    cache: &dyn DocumentCache,
    source_id: &SourceId,
    previous_generation: Option<&SourceGenerationId>,
    item_key: &SourceItemKey,
    next_generation: &SourceGenerationId,
) -> BoundaryResult<Option<ReusedWebDocument>> {
    let Some(generation) = previous_generation else {
        return Ok(None);
    };
    let cached = cache
        .get(DocumentCacheKey {
            source_id: source_id.clone(),
            source_item_key: item_key.clone(),
            generation: Some(generation.clone()),
        })
        .await?;
    Ok(cached.map(|cached| ReusedWebDocument {
        document: retarget_document(cached.document, next_generation),
    }))
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

fn enforce_cache_limits(cache: &mut BTreeMap<DocumentCacheKey, CachedDocument>) {
    if cache.len() <= MAX_CACHED_DOCUMENTS
        && estimated_cache_bytes(cache) <= MAX_CACHED_DOCUMENT_BYTES
    {
        return;
    }

    let mut entries = cache
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

    let mut total_bytes = entries.iter().map(|(_, _, bytes)| *bytes).sum::<usize>();
    let mut total_entries = cache.len();
    for (_, key, bytes) in entries {
        if total_entries <= MAX_CACHED_DOCUMENTS && total_bytes <= MAX_CACHED_DOCUMENT_BYTES {
            break;
        }
        if cache.remove(&key).is_some() {
            total_entries = total_entries.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(bytes);
        }
    }
}

fn estimated_cache_bytes(cache: &BTreeMap<DocumentCacheKey, CachedDocument>) -> usize {
    cache.values().map(estimated_cached_document_bytes).sum()
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
        + estimated_content_ref_bytes(&document.content)
}

fn estimated_content_ref_bytes(content: &ContentRef) -> usize {
    match content {
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
