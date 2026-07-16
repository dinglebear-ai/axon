//! Memory-specific `SourceDocument` preparation for vector writes.
//!
//! Memory is not a public source adapter, but vector payloads still need the
//! same prepared-document and payload-validation path as source-authored
//! documents. This bridge preserves one-record/one-point semantics by forcing
//! the `atomic_metadata` profile.

use std::collections::BTreeSet;

use axon_api::source::*;
use axon_core::redact::REDACTION_VERSION;
use axon_document::{ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest};
use axon_vectors::payload::VectorPayload;
use axon_vectors::point::{
    VectorPointBatchBuildContext, VectorPointBatchBuildError, VectorPointBatchBuilder,
};
use serde_json::json;

use super::payload::memory_collection_spec;
use super::{MEMORY_VECTOR_NAMESPACE, MemoryVectorConfig};
use crate::store::Result;

pub(super) fn prepare_memory_document(record: &MemoryRecord) -> Result<PreparedDocument> {
    let canonical_uri = memory_canonical_uri(&record.memory_id);
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("memory"));
    metadata.insert("source_kind".to_string(), json!("memory"));
    metadata.insert("source_adapter".to_string(), json!("axon-memory"));
    metadata.insert("source_scope".to_string(), json!(record.scope.kind));
    metadata.insert("source_canonical_uri".to_string(), json!(canonical_uri));
    metadata.insert("memory_id".to_string(), json!(record.memory_id.0));
    metadata.insert(
        "memory_type".to_string(),
        json!(memory_type_str(record.memory_type)),
    );
    metadata.insert(
        "memory_status".to_string(),
        json!(memory_status_str(record.status)),
    );
    metadata.insert(
        "memory_recallable".to_string(),
        json!(record.status == MemoryStatus::Active),
    );
    metadata.insert("memory_scope_kind".to_string(), json!(record.scope.kind));
    metadata.insert("memory_scope_value".to_string(), json!(record.scope.value));
    metadata.insert("memory_confidence".to_string(), json!(record.confidence));
    metadata.insert("memory_salience".to_string(), json!(record.salience));
    metadata.insert(
        "visibility".to_string(),
        json!(visibility_str(record.visibility)),
    );
    metadata.insert("redaction_version".to_string(), json!(REDACTION_VERSION));

    let document = SourceDocument {
        document_id: DocumentId::new(record.memory_id.0.clone()),
        source_id: SourceId::new(record.memory_id.0.clone()),
        source_item_key: SourceItemKey::new(record.memory_id.0.clone()),
        canonical_uri: canonical_uri.clone(),
        content_kind: ContentKind::PlainText,
        content: ContentRef::InlineText {
            text: record.body.clone(),
        },
        metadata,
        title: record.title.clone(),
        language: None,
        path: None,
        mime_type: Some("text/plain".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    };

    DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document,
            generation: SourceGenerationId::new("0"),
            profile: Some(ChunkingProfile::AtomicMetadata),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .map(|result| result.document)
        .map_err(|err| {
            ApiError::new(
                "memory.prepare_failed",
                axon_error::ErrorStage::Preparing,
                err,
            )
            .with_context("memory_id", record.memory_id.0.clone())
            .with_context("source_kind", "memory")
        })
}

pub(super) fn embedding_inputs(document: &PreparedDocument) -> Vec<EmbeddingInput> {
    document
        .chunks
        .iter()
        .map(|chunk| EmbeddingInput {
            chunk_id: chunk.chunk_id.clone(),
            text: chunk
                .embedding_text
                .clone()
                .unwrap_or_else(|| chunk.content.clone()),
            content_kind: chunk.content_kind,
            metadata: chunk.metadata.clone(),
        })
        .collect()
}

pub(super) fn build_memory_vector_batch(
    config: &MemoryVectorConfig,
    document: PreparedDocument,
    embeddings: EmbeddingResult,
) -> Result<VectorPointBatch> {
    let memory_id = document
        .metadata
        .get("memory_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| document.document_id.0.clone());
    let mut batch = VectorPointBatchBuilder::new(
        memory_collection_spec(config),
        document,
        embeddings,
        VectorPointBatchBuildContext {
            embedded_at: Timestamp::from(chrono::Utc::now()),
        },
    )
    .build()
    .map_err(|error| memory_vector_build_error(&memory_id, error))?;
    normalize_memory_payloads(&memory_id, &mut batch)?;
    if batch.points.is_empty() {
        tracing::warn!(
            event = "security_audit.redaction_decision",
            memory_id = %memory_id,
            "memory vector payload redaction skipped the atomic chunk"
        );
        return Err(ApiError::new(
            "redaction.failed",
            axon_error::ErrorStage::Vectorizing,
            "memory vector payload redaction rejected the atomic memory body",
        )
        .with_context("memory_id", memory_id)
        .with_context("source_kind", "memory"));
    }
    Ok(batch)
}

pub(super) fn embedding_for_document(
    embeddings: &EmbeddingResult,
    document: &PreparedDocument,
) -> EmbeddingResult {
    let chunk_ids = document
        .chunks
        .iter()
        .map(|chunk| chunk.chunk_id.clone())
        .collect::<BTreeSet<_>>();
    EmbeddingResult {
        batch_id: embeddings.batch_id,
        job_id: embeddings.job_id,
        provider_id: embeddings.provider_id.clone(),
        model: embeddings.model.clone(),
        dimensions: embeddings.dimensions,
        vectors: embeddings
            .vectors
            .iter()
            .filter(|vector| chunk_ids.contains(&vector.chunk_id))
            .cloned()
            .collect(),
        usage: embeddings.usage.clone(),
        warnings: embeddings.warnings.clone(),
    }
}

fn normalize_memory_payloads(memory_id: &str, batch: &mut VectorPointBatch) -> Result<()> {
    for point in &mut batch.points {
        point.payload.insert(
            "vector_namespace".to_string(),
            json!(MEMORY_VECTOR_NAMESPACE),
        );
        point
            .payload
            .insert("source_family".to_string(), json!("memory"));
        point
            .payload
            .insert("source_kind".to_string(), json!("memory"));
        point
            .payload
            .insert("source_adapter".to_string(), json!("axon-memory"));
        VectorPayload::try_from_metadata(point.payload.clone()).map_err(|source| {
            ApiError::new(
                "memory.vector_payload_failed",
                axon_error::ErrorStage::Vectorizing,
                format!("invalid memory vector payload: {source}"),
            )
            .with_context("memory_id", memory_id.to_string())
            .with_context("source_kind", "memory")
            .with_context("chunk_id", point.chunk_id.0.clone())
        })?;
    }
    Ok(())
}

fn memory_vector_build_error(memory_id: &str, error: VectorPointBatchBuildError) -> ApiError {
    match &error {
        VectorPointBatchBuildError::Payload {
            chunk_id,
            source: axon_vectors::payload::VectorPayloadValidationError::ForbiddenValue { field },
        } => ApiError::new(
            "redaction.failed",
            axon_error::ErrorStage::Vectorizing,
            format!("memory vector payload contains forbidden retrievable value under {field}"),
        )
        .with_context("memory_id", memory_id.to_string())
        .with_context("source_kind", "memory")
        .with_context("chunk_id", chunk_id.0.clone()),
        VectorPointBatchBuildError::Payload { chunk_id, .. } => ApiError::new(
            "memory.vector_payload_failed",
            axon_error::ErrorStage::Vectorizing,
            error.to_string(),
        )
        .with_context("memory_id", memory_id.to_string())
        .with_context("source_kind", "memory")
        .with_context("chunk_id", chunk_id.0.clone()),
        _ => ApiError::new(
            "memory.vector_payload_failed",
            axon_error::ErrorStage::Vectorizing,
            error.to_string(),
        )
        .with_context("memory_id", memory_id.to_string())
        .with_context("source_kind", "memory"),
    }
}

fn memory_canonical_uri(memory_id: &MemoryId) -> String {
    format!("memory://{}", memory_id.0)
}

fn memory_type_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Decision => "decision",
        MemoryType::Fact => "fact",
        MemoryType::Preference => "preference",
        MemoryType::Task => "task",
        MemoryType::Bug => "bug",
        MemoryType::Procedure => "procedure",
        MemoryType::Incident => "incident",
        MemoryType::Entity => "entity",
        MemoryType::Episode => "episode",
        MemoryType::Working => "working",
    }
}

fn memory_status_str(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Active => "active",
        MemoryStatus::Review => "review",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Contradicted => "contradicted",
        MemoryStatus::Archived => "archived",
        MemoryStatus::Forgotten => "forgotten",
        MemoryStatus::Working => "working",
    }
}

fn visibility_str(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Sensitive => "sensitive",
        Visibility::Redacted => "redacted",
        Visibility::Derived => "derived",
    }
}
