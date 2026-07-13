use axon_api::source::*;

const VERTICAL_PARSE_FACTS_KEY: &str = "_axon_vertical_parse_facts";
const VERTICAL_GRAPH_CANDIDATES_KEY: &str = "_axon_vertical_graph_candidates";

pub(super) fn take_vertical_parse_artifacts(
    document: &mut SourceDocument,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let facts = document
        .metadata
        .remove(VERTICAL_PARSE_FACTS_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let candidates = document
        .metadata
        .remove(VERTICAL_GRAPH_CANDIDATES_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    (facts, candidates)
}

pub(super) fn sanitize_web_payload_metadata(document: &mut PreparedDocument) {
    sanitize_metadata(&mut document.metadata);
    for chunk in &mut document.chunks {
        sanitize_metadata(&mut chunk.metadata);
        if let Some(title) = chunk.title.as_deref().or(document
            .metadata
            .get("web_title")
            .and_then(|value| value.as_str()))
        {
            chunk
                .metadata
                .insert("web_title".to_string(), serde_json::json!(title));
        }
    }
}

fn sanitize_metadata(metadata: &mut MetadataMap) {
    for field in ["web_render_mode", "web_status", "web_etag"] {
        metadata.remove(field);
    }
}

pub(super) fn changed_diff_batches(
    diff: &SourceManifestDiff,
    batch_size: usize,
) -> Vec<SourceManifestDiff> {
    let batch_size = batch_size.max(1);
    let mut batches = Vec::new();
    let mut current = empty_diff_like(diff);
    for item in &diff.added {
        current.added.push(item.clone());
        if changed_batch_len(&current) == batch_size {
            push_changed_batch(&mut batches, &mut current, diff);
        }
    }
    for item in &diff.modified {
        current.modified.push(item.clone());
        if changed_batch_len(&current) == batch_size {
            push_changed_batch(&mut batches, &mut current, diff);
        }
    }
    if changed_batch_len(&current) > 0 {
        push_changed_batch(&mut batches, &mut current, diff);
    }
    batches
}

fn changed_batch_len(batch: &SourceManifestDiff) -> usize {
    batch.added.len() + batch.modified.len()
}

fn push_changed_batch(
    batches: &mut Vec<SourceManifestDiff>,
    current: &mut SourceManifestDiff,
    diff: &SourceManifestDiff,
) {
    current.counts.added = current.added.len() as u64;
    current.counts.modified = current.modified.len() as u64;
    batches.push(std::mem::replace(current, empty_diff_like(diff)));
}

fn empty_diff_like(diff: &SourceManifestDiff) -> SourceManifestDiff {
    SourceManifestDiff {
        header: diff.header.clone(),
        source_id: diff.source_id.clone(),
        previous_generation: diff.previous_generation.clone(),
        next_generation: diff.next_generation.clone(),
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

pub(super) fn prepared_document_batches(
    documents: Vec<PreparedDocument>,
    max_chunks: usize,
) -> Vec<Vec<PreparedDocument>> {
    let max_chunks = max_chunks.max(1);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chunks = 0_usize;
    for document in documents {
        let document_chunks = document.chunks.len().max(1);
        if !current.is_empty() && current_chunks + document_chunks > max_chunks {
            batches.push(std::mem::take(&mut current));
            current_chunks = 0;
        }
        current_chunks += document_chunks;
        current.push(document);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

pub(super) fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

pub(super) fn document_status(
    document: &PreparedDocument,
    vector_point_count: u64,
    status: DocumentLifecycleStatus,
    updated_at: Timestamp,
) -> DocumentStatus {
    DocumentStatus {
        document_id: document.document_id.clone(),
        source_id: document.source_id.clone(),
        source_item_key: document.source_item_key.clone(),
        generation: Some(document.generation.clone()),
        status,
        updated_at,
        chunk_count: document.chunks.len() as u32,
        vector_point_count: vector_point_count.min(u32::MAX as u64) as u32,
        error: None,
        cleanup_status: None,
    }
}
