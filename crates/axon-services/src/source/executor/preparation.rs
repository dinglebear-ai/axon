//! Bounded CPU preparation for source documents.

use std::collections::BTreeMap;
use std::sync::Arc;

use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use futures_util::{StreamExt, stream};
use tokio::sync::Semaphore;

/// Shared by execution and publication compatibility; only the derived ceiling
/// affects prepared output, not concurrency or the raw admission budget.
pub(super) fn effective_content_limit(
    preparer: &DocumentPreparer,
    max_in_flight_bytes: usize,
    max_bytes_per_item: Option<u64>,
) -> usize {
    max_bytes_per_item
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(usize::MAX)
        .min(max_in_flight_bytes / 5)
        .min(preparer.semantic_config().max_content_bytes)
}

pub(super) async fn prepare_documents(
    documents: Vec<SourceDocument>,
    generation: &SourceGenerationId,
    enrichment_graph: &BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    preparer: DocumentPreparer,
    concurrency: usize,
    max_in_flight_bytes: usize,
    max_bytes_per_item: Option<u64>,
) -> anyhow::Result<Vec<PrepareSourceDocumentResult>> {
    let content_limit = effective_content_limit(&preparer, max_in_flight_bytes, max_bytes_per_item);
    let preparer = preparer.with_content_byte_limit(content_limit);
    let generation = generation.clone();
    let work_items = documents
        .into_iter()
        .map(|document| {
            let graph_candidates = enrichment_graph
                .get(&document.source_item_key)
                .cloned()
                .unwrap_or_default();
            (document, graph_candidates)
        })
        .collect::<Vec<_>>();
    bounded_blocking_map_in_order(
        work_items,
        concurrency,
        max_in_flight_bytes,
        |(document, _)| source_document_bytes(document),
        move |(document, graph_candidates)| {
            let item_key = document.source_item_key.0.clone();
            Ok(preparer
                .prepare(PrepareSourceDocumentRequest {
                    document,
                    generation: generation.clone(),
                    profile: None,
                    parse_facts: Vec::new(),
                    graph_candidates,
                    warnings: Vec::new(),
                    errors: Vec::new(),
                })
                .map_err(|error| {
                    ApiError::new(
                        "document.prepare_failed",
                        ErrorStage::Preparing,
                        format!("failed to prepare document: {error}"),
                    )
                    .with_source_item_key(item_key)
                })?)
        },
    )
    .await
}

fn source_document_bytes(document: &SourceDocument) -> usize {
    let content_bytes = match &document.content {
        ContentRef::InlineText { text } => text.len().saturating_mul(2),
        ContentRef::InlineBytes { bytes_base64, .. } => {
            // Transport plus decoded bytes and worst-case UTF-16 to UTF-8 output.
            let decoded = bytes_base64.len().saturating_add(3) / 4 * 3;
            bytes_base64
                .len()
                .saturating_add(decoded)
                .saturating_add(decoded.saturating_mul(2))
        }
        ContentRef::Artifact { artifact_id } => artifact_id.0.len(),
        ContentRef::External { uri, integrity } => {
            uri.len() + integrity.as_ref().map_or(0, String::len)
        }
    };
    content_bytes
        .saturating_add(document.canonical_uri.len())
        .saturating_add(document.title.as_ref().map_or(0, String::len))
        .saturating_add(document.path.as_ref().map_or(0, String::len))
        .max(1)
}

async fn bounded_blocking_map_in_order<T, R, W, F>(
    items: Vec<T>,
    concurrency: usize,
    byte_budget: usize,
    weight: W,
    work: F,
) -> anyhow::Result<Vec<R>>
where
    T: Send + 'static,
    R: Send + 'static,
    W: Fn(&T) -> usize,
    F: Fn(T) -> anyhow::Result<R> + Send + Sync + 'static,
{
    let byte_budget = byte_budget.min(u32::MAX as usize);
    if items.iter().any(|item| weight(item).max(1) > byte_budget) {
        anyhow::bail!("document preparation item exceeds resident byte budget");
    }
    let byte_slots = Arc::new(Semaphore::new(byte_budget));
    let work = Arc::new(work);
    let mut output = stream::iter(items.into_iter().enumerate())
        .map(|(index, item)| {
            let permits = weight(&item).max(1) as u32;
            let byte_slots = Arc::clone(&byte_slots);
            let work = Arc::clone(&work);
            async move {
                let byte_permit =
                    byte_slots
                        .acquire_many_owned(permits)
                        .await
                        .map_err(|error| {
                            anyhow::anyhow!("document preparation byte gate closed: {error}")
                        })?;
                let result = tokio::task::spawn_blocking(move || {
                    let _byte_permit = byte_permit;
                    work(item)
                })
                .await
                .map_err(|error| {
                    anyhow::anyhow!("document preparation task {index} failed: {error}")
                })?;
                result.map(|value| (index, value))
            }
        })
        // Poll completions as they become ready so one slow early document does
        // not prevent replacement work from entering the bounded worker set.
        // Sequence numbers restore the public input-order contract below.
        .buffer_unordered(concurrency.max(1))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
    output.sort_unstable_by_key(|(index, _)| *index);
    Ok(output.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
