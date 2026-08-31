use axon_api::source::PreparedDocument;

/// Admission weight for prepared documents. Even a document that produces no
/// chunks still owns lifecycle state and must retain capacity until that state
/// is durably checkpointed.
pub(in crate::source::executor) fn charged_chunk_count(documents: &[PreparedDocument]) -> usize {
    documents
        .iter()
        .map(|document| document.chunks.len().max(1))
        .sum()
}

pub(in crate::source::executor) fn chunk_batches(
    documents: Vec<PreparedDocument>,
    max_chunks: usize,
) -> Vec<Vec<PreparedDocument>> {
    let max_chunks = max_chunks.max(1);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut chunks = 0;
    for document in documents
        .into_iter()
        .flat_map(|document| split_oversized_document(document, max_chunks))
    {
        let count = charged_chunk_count(std::slice::from_ref(&document));
        if !current.is_empty() && chunks + count > max_chunks {
            batches.push(std::mem::take(&mut current));
            chunks = 0;
        }
        chunks += count;
        current.push(document);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

pub(in crate::source::executor) fn split_oversized_document(
    mut document: PreparedDocument,
    max_chunks: usize,
) -> Vec<PreparedDocument> {
    let max_chunks = max_chunks.max(1);
    if document.chunks.len() <= max_chunks {
        return vec![document];
    }

    let chunks = std::mem::take(&mut document.chunks);
    let window_count = chunks.len().div_ceil(max_chunks);
    let mut chunks = chunks.into_iter();
    let mut continuation = document.clone();
    continuation.graph_candidates.clear();
    continuation.warnings.clear();
    continuation.errors.clear();

    document.chunks = chunks.by_ref().take(max_chunks).collect();
    let mut windows = Vec::with_capacity(window_count);
    windows.push(document);
    loop {
        let window_chunks = chunks.by_ref().take(max_chunks).collect::<Vec<_>>();
        if window_chunks.is_empty() {
            break;
        }
        let mut window = continuation.clone();
        window.chunks = window_chunks;
        windows.push(window);
    }
    windows
}
