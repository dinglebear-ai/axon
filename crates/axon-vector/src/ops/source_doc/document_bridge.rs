use axon_api::source::{
    ContentKind, ContentRef, DocumentId, MetadataMap, SourceGenerationId, SourceId, SourceItemKey,
    SourceRange,
};
use axon_document::{ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest};
use serde_json::Value;

use super::support::{base_chunk_metadata, chunk_metadata};
use super::{PreparedDoc, SourceDocument};

pub(super) fn prepare_atomic_source(
    doc: SourceDocument,
    point_id: uuid::Uuid,
) -> Result<PreparedDoc, String> {
    let prepared = prepare_with_document_crate(
        &doc,
        ContentKind::BinaryMetadata,
        ChunkingProfile::AtomicMetadata,
        None,
    )?;
    let mut chunks = Vec::with_capacity(prepared.chunks.len());
    let mut chunk_extra = Vec::with_capacity(prepared.chunks.len());
    for (idx, chunk) in prepared.chunks.into_iter().enumerate() {
        let chunk_id = chunk.chunk_id.0;
        let chunk_key = chunk.chunk_key;
        let content_hash = chunk.content_hash;
        chunks.push(chunk.content);
        let mut metadata = base_chunk_metadata_from_range(
            "plain_text",
            &format!("{}#chunk-{idx}", doc.url),
            &chunk.source_range,
        );
        metadata.insert("prepared_chunk_id".to_string(), chunk_id.into());
        metadata.insert("prepared_chunk_key".to_string(), chunk_key.into());
        metadata.insert("prepared_content_hash".to_string(), content_hash.into());
        chunk_extra.push(chunk_metadata(metadata));
    }
    Ok(doc
        .into_prepared(chunks, "text", chunk_extra)
        .with_chunk_point_ids(vec![point_id]))
}

fn prepare_with_document_crate(
    doc: &SourceDocument,
    content_kind: ContentKind,
    profile: ChunkingProfile,
    path: Option<String>,
) -> Result<axon_api::source::PreparedDocument, String> {
    let result = DocumentPreparer::default().prepare(PrepareSourceDocumentRequest {
        document: to_document_source(doc, content_kind, path),
        generation: SourceGenerationId::from(format!(
            "legacy-vector-adapter:{}",
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, doc.url.as_bytes())
        )),
        profile: Some(profile),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    })?;
    Ok(result.document)
}

fn to_document_source(
    doc: &SourceDocument,
    content_kind: ContentKind,
    path: Option<String>,
) -> axon_api::source::SourceDocument {
    axon_api::source::SourceDocument {
        document_id: DocumentId::from(format!(
            "legacy-vector:{}",
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, doc.url.as_bytes())
        )),
        source_id: SourceId::from(format!(
            "legacy-vector:{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("{}:{}", doc.source_type, doc.url).as_bytes()
            )
        )),
        source_item_key: SourceItemKey::from(doc.url.clone()),
        canonical_uri: doc.url.clone(),
        content_kind,
        content: ContentRef::InlineText {
            text: doc.text.clone(),
        },
        metadata: metadata_from_extra(&doc.extra),
        title: doc.title.clone(),
        language: None,
        path,
        mime_type: None,
        structured_payload: doc.structured.as_ref().map(|payload| payload.blob.clone()),
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn metadata_from_extra(extra: &Option<Value>) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    if let Some(Value::Object(map)) = extra {
        metadata.extend(map.iter().map(|(key, value)| (key.clone(), value.clone())));
    }
    metadata
}

fn base_chunk_metadata_from_range(
    content_kind: &str,
    locator: &str,
    range: &SourceRange,
) -> serde_json::Map<String, Value> {
    let line_start = range.line_start.unwrap_or(1);
    base_chunk_metadata(
        content_kind,
        locator,
        line_start,
        range.line_end.unwrap_or(line_start),
        range.byte_start.unwrap_or(0) as usize,
        range.byte_end.unwrap_or(0) as usize,
    )
}
