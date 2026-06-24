use axon_core::logging::log_info;

pub(super) struct ContextStartLog<'a> {
    pub(super) reranked_len: usize,
    pub(super) top_chunks_len: usize,
    pub(super) top_full_docs_len: usize,
    pub(super) max_context_chars: usize,
    pub(super) doc_chunk_limit: usize,
    pub(super) doc_fetch_concurrency: usize,
    pub(super) skip_full_docs: bool,
    pub(super) skip_reason: &'a str,
}

pub(super) fn log_context_start(inputs: ContextStartLog<'_>) {
    log_info(&format!(
        "ask context assembly start reranked={} top_chunks_planned={} full_docs_planned={} max_context_chars={} doc_chunk_limit={} doc_fetch_concurrency={} skip_full_docs={} skip_reason={}",
        inputs.reranked_len,
        inputs.top_chunks_len,
        inputs.top_full_docs_len,
        inputs.max_context_chars,
        inputs.doc_chunk_limit,
        inputs.doc_fetch_concurrency,
        inputs.skip_full_docs,
        inputs.skip_reason,
    ));
}

pub(super) struct ContextCompleteLog {
    pub(super) top_chunks_selected: usize,
    pub(super) full_docs_selected: usize,
    pub(super) supplemental_count: usize,
    pub(super) context_chars: usize,
    pub(super) elapsed_ms: u128,
}

pub(super) fn log_context_complete(inputs: ContextCompleteLog) {
    log_info(&format!(
        "ask context assembly complete chunks={} full_docs={} supplemental={} context_chars={} elapsed_ms={}",
        inputs.top_chunks_selected,
        inputs.full_docs_selected,
        inputs.supplemental_count,
        inputs.context_chars,
        inputs.elapsed_ms,
    ));
}
