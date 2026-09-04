//! Bounded CPU preparation for source documents.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Arc;

use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use tokio::sync::Semaphore;

const MAX_IN_FLIGHT_BYTES: usize = 64 * 1024 * 1024;

fn max_in_flight_bytes() -> usize {
    std::env::var("AXON_PREP_MAX_IN_FLIGHT_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(MAX_IN_FLIGHT_BYTES)
        .clamp(1024 * 1024, u32::MAX as usize)
}

pub(super) async fn prepare_documents(
    documents: Vec<SourceDocument>,
    generation: &SourceGenerationId,
    enrichment_graph: &BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    preparer: DocumentPreparer,
    concurrency: usize,
) -> anyhow::Result<Vec<PreparedDocument>> {
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
        max_in_flight_bytes(),
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
                .map_err(|error| anyhow::anyhow!("failed to prepare {item_key}: {error}"))?
                .document)
        },
    )
    .await
}

fn source_document_bytes(document: &SourceDocument) -> usize {
    let content_bytes = match &document.content {
        ContentRef::InlineText { text } => text.len(),
        ContentRef::InlineBytes { bytes_base64, .. } => bytes_base64.len(),
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
    let task_slots = Arc::new(Semaphore::new(concurrency.max(1)));
    let byte_budget = byte_budget.max(1).min(u32::MAX as usize);
    let byte_slots = Arc::new(Semaphore::new(byte_budget));
    let work = Arc::new(work);
    let mut handles = VecDeque::with_capacity(concurrency.max(1));
    let mut output = Vec::with_capacity(items.len());

    for item in items {
        let task_permit = Arc::clone(&task_slots)
            .acquire_owned()
            .await
            .map_err(|error| anyhow::anyhow!("document preparation gate closed: {error}"))?;
        let permits = weight(&item).max(1).min(byte_budget) as u32;
        let byte_permit = Arc::clone(&byte_slots)
            .acquire_many_owned(permits)
            .await
            .map_err(|error| anyhow::anyhow!("document preparation byte gate closed: {error}"))?;
        let work = Arc::clone(&work);
        handles.push_back(tokio::task::spawn_blocking(move || {
            let _task_permit = task_permit;
            let _byte_permit = byte_permit;
            work(item)
        }));
        if handles.len() >= concurrency.max(1) {
            let index = output.len();
            let handle = handles.pop_front().expect("bounded queue is non-empty");
            output.push(handle.await.map_err(|error| {
                anyhow::anyhow!("document preparation task {index} failed: {error}")
            })??);
        }
    }

    for handle in handles {
        let index = output.len();
        output.push(handle.await.map_err(|error| {
            anyhow::anyhow!("document preparation task {index} failed: {error}")
        })??);
    }
    Ok(output)
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
