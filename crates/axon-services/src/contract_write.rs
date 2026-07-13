//! Ad-hoc (ledger-free) contract write path.
//!
//! Builds a [`SourceDocument`], runs it through [`DocumentPreparer`], embeds
//! the resulting chunks via the configured [`EmbeddingProvider`], and upserts
//! the resulting points via the configured [`VectorStore`].
//!
//! This is the "one-shot, not source-tracked" sibling of the ledger-backed
//! write path in [`crate::local_source`]: callers here embed content that
//! isn't diffed/generationed across runs (a single `scrape` result, a
//! `sessions_legacy` transcript export) — see `docs/pipeline-unification/`
//! and issue #298 for the target-architecture split between ledger-tracked
//! "sources" and this simpler one-shot path.
//!
//! Field-name note: the vector payload contract
//! (`axon_vectors::payload::VectorPayload`) validates every non-shared
//! metadata field against a fixed per-`source_family` allowlist
//! (`axon_vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`). Fields the
//! legacy `axon-vector` pipeline used to stamp freely (e.g. a bare `seed_url`,
//! or the rich `session_*` metadata `sessions_legacy` used to attach) have no
//! slot in that allowlist and are dropped here — the same accepted limitation
//! already documented by `sessions_source_adapter::remap_to_vector_payload_contract`.
//! Origin tracking is instead expressed via `source_canonical_uri` /
//! `item_canonical_uri`, which `axon-vectors` derives automatically from
//! `PreparedDocument::canonical_uri` when the document doesn't set its own
//! `source_canonical_uri` — the correct behavior for these single-item,
//! non-ledgered sources (source identity == item identity).

use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::provider::EmbeddingProvider;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};
use axon_vectors::store::VectorStore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use axon_core::config::Config;

use crate::context::build_read_stores_from_config;

/// Result of an ad-hoc embed+upsert call. Unlike the legacy
/// `axon_vector::ops::EmbedSummary`, this path treats one call as an
/// all-or-nothing batch (matching `local_source`'s own vectorize-batch
/// precedent) rather than tracking per-document partial failures within a
/// single call — a batch either fully embeds or the call returns `Err`, so
/// there is no `docs_failed` counter to carry (callers that need partial
/// per-document failure tolerance, like `sessions_legacy`, track their own
/// preparation failures before calling this and combine the two).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ContractWriteSummary {
    pub(crate) docs_embedded: usize,
    pub(crate) chunks_embedded: usize,
}

/// Stable, deterministic 24-hex-char token derived from `value` (sha256,
/// first 12 bytes). Used to build stable `SourceId`/`DocumentId`/
/// `SourceItemKey` values for content that isn't ledger-tracked, mirroring
/// `sessions_source_adapter::stable_token`.
pub(crate) fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

/// Fixed single-shot generation id for ad-hoc (non-ledgered) documents.
/// `axon-vectors` requires `source_generation` to parse as a non-negative
/// integer (see `generation_payload_i64`); every ad-hoc write uses the same
/// placeholder generation since there is no ledger tracking successive
/// generations for these one-shot sources.
pub(crate) fn adhoc_generation() -> SourceGenerationId {
    SourceGenerationId::new("gen_1")
}

/// Dense+sparse collection spec matching the shape every other adapter uses
/// (`local_source_adapter::collection_spec`, `sessions_source_adapter::collection_spec`).
pub(crate) fn collection_spec(collection: &str, dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![
            payload_index("source_id"),
            payload_index("source_generation"),
            payload_index("source_item_key"),
            payload_index("document_id"),
            payload_index("chunk_id"),
        ],
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

/// Prepare `document` (chunk it) via [`DocumentPreparer`]. Pure/CPU-bound —
/// cheap enough for the single-document call sites here that it doesn't need
/// `spawn_blocking` the way the legacy markdown/plain-text chunkers did.
pub(crate) fn prepare_document(
    document: SourceDocument,
    generation: SourceGenerationId,
) -> Result<PreparedDocument, String> {
    let preparer = DocumentPreparer::default();
    let request = PrepareSourceDocumentRequest {
        document,
        generation,
        profile: None,
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };
    Ok(preparer.prepare(request)?.document)
}

/// Embed and upsert a batch of already-prepared documents into `collection`,
/// building the embedding provider and vector store fresh from `cfg` (no
/// caching — matches the legacy `embed_prepared_docs(cfg, docs, ...)` call
/// shape, which also built fresh TEI/Qdrant clients per call).
pub(crate) async fn embed_and_upsert_documents(
    cfg: &Config,
    collection: &str,
    documents: Vec<PreparedDocument>,
) -> anyhow::Result<ContractWriteSummary> {
    if documents.is_empty() {
        return Ok(ContractWriteSummary::default());
    }
    let stores = build_read_stores_from_config(cfg).await;
    let job_id = JobId::new(Uuid::new_v4());
    let collection_spec = collection_spec(collection, stores.embedding_dimensions);
    stores
        .vector_store
        .ensure_collection(collection_spec.clone())
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let chunks_total: usize = documents.iter().map(|doc| doc.chunks.len()).sum();
    let embeddings = embed_documents(
        stores.embedding_provider.as_ref(),
        &documents,
        job_id,
        &stores.embedding_provider_id,
        &stores.embedding_model,
    )
    .await?;
    let batch = vector_point_batch(collection_spec, &documents, &embeddings)?;
    stores
        .vector_store
        .upsert(batch)
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    Ok(ContractWriteSummary {
        docs_embedded: documents.len(),
        chunks_embedded: chunks_total,
    })
}

async fn embed_documents(
    embedding_provider: &dyn EmbeddingProvider,
    documents: &[PreparedDocument],
    job_id: JobId,
    provider_id: &ProviderId,
    model: &str,
) -> anyhow::Result<EmbeddingResult> {
    let batch_id = BatchId::new(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        documents
            .iter()
            .map(|document| document.document_id.0.as_str())
            .collect::<Vec<_>>()
            .join(":")
            .as_bytes(),
    ));
    let mut builder = EmbeddingBatchBuilder::new(batch_id, job_id, provider_id.clone(), model)
        .priority(JobPriority::Background);
    for document in documents {
        for chunk in &document.chunks {
            builder = builder.push_input(EmbeddingInput {
                chunk_id: chunk.chunk_id.clone(),
                text: chunk
                    .embedding_text
                    .clone()
                    .unwrap_or_else(|| chunk.content.clone()),
                content_kind: chunk.content_kind,
                metadata: chunk.metadata.clone(),
            });
        }
    }
    embedding_provider
        .embed(
            builder
                .build()
                .map_err(|err| anyhow::anyhow!(err.to_string()))?,
        )
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))
}

fn vector_point_batch(
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &EmbeddingResult,
) -> anyhow::Result<VectorPointBatch> {
    let vectors_by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
    for document in documents {
        let document_embeddings = EmbeddingResult {
            batch_id: embeddings.batch_id.clone(),
            job_id: embeddings.job_id,
            provider_id: embeddings.provider_id.clone(),
            model: embeddings.model.clone(),
            dimensions: embeddings.dimensions,
            vectors: document
                .chunks
                .iter()
                .filter_map(|chunk| vectors_by_chunk.get(&chunk.chunk_id).cloned())
                .collect(),
            usage: embeddings.usage.clone(),
            warnings: embeddings.warnings.clone(),
        };
        let batch = VectorPointBatchBuilder::new(
            collection.clone(),
            document.clone(),
            document_embeddings,
            VectorPointBatchBuildContext {
                embedded_at: timestamp(),
            },
        )
        .build()
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        points.extend(batch.points);
    }
    Ok(VectorPointBatch {
        batch_id: embeddings.batch_id.clone(),
        collection: collection.collection,
        points,
        model: embeddings.model.clone(),
        dimensions: embeddings.dimensions,
        sparse_vectors: None,
        payload_indexes: collection.payload_indexes,
    })
}

/// Shared field names every source family may use regardless of its own
/// specific allowlist (`axon_vectors::payload::VECTOR_SHARED_FIELDS`), plus
/// the small per-family allowlists this module's callers rely on
/// (`axon_vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`).
pub(crate) use axon_vectors::payload::VECTOR_SHARED_FIELDS;

/// Retain only metadata fields the vector payload contract accepts: the
/// always-allowed shared fields plus `extra_allowed` (the caller's own
/// source-family-specific allowlist). Any other field would otherwise trip
/// `VectorPayload::try_from_metadata`'s `UnknownSourceSpecificField` check —
/// see the module doc comment for why this drops some legacy metadata.
pub(crate) fn retain_contract_fields(metadata: &mut MetadataMap, extra_allowed: &[&str]) {
    metadata.retain(|field, _| {
        VECTOR_SHARED_FIELDS.contains(&field.as_str()) || extra_allowed.contains(&field.as_str())
    });
}
