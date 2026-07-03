//! Deterministic fixtures for vector boundary tests.

use axon_api::source::*;
use serde_json::json;

use crate::point::VectorPointBatchBuildContext;

pub const MODULE_NAME: &str = "testing";

pub fn test_collection_spec(dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![
            PayloadIndexSpec {
                field_name: "source_id".to_string(),
                field_schema: PayloadFieldSchema::Keyword,
                required_for_filters: true,
            },
            PayloadIndexSpec {
                field_name: "source_generation".to_string(),
                field_schema: PayloadFieldSchema::Keyword,
                required_for_filters: true,
            },
            PayloadIndexSpec {
                field_name: "chunk_id".to_string(),
                field_schema: PayloadFieldSchema::Keyword,
                required_for_filters: true,
            },
        ],
        sparse: None,
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

/// Like [`test_collection_spec`] but declares a `bm42` sparse namespace so
/// hybrid (dense + sparse) search requests are accepted. Production named-mode
/// collections carry this; the retrieval engine issues hybrid searches.
pub fn test_collection_spec_hybrid(dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        ..test_collection_spec(dimensions)
    }
}

/// Fields a caller supplies to [`test_clean_point`]. The batch-lineage fields
/// (`batch_id`, `model`, `dimensions`, `job_id`) must match the enclosing
/// [`VectorPointBatch`] or upsert validation rejects the point.
pub struct TestPointSpec<'a> {
    pub collection: &'a str,
    pub point_id: &'a str,
    pub chunk_id: &'a str,
    pub vector: &'a [f32],
    pub text: &'a str,
    pub namespace: &'a str,
    pub batch_id: &'a str,
    pub model: &'a str,
    pub dimensions: u32,
    pub job_id: &'a str,
}

/// Build a fully payload-valid, `redaction_status = clean` point for upsert +
/// retrieval tests. Populates every field in `VECTOR_REQUIRED_FIELDS` so the
/// shared upsert validator accepts it, and stamps the canonical URI at
/// `https://example.com/{chunk_id}`.
pub fn test_clean_point(spec: TestPointSpec<'_>) -> VectorPoint {
    let canonical_uri = format!("https://example.com/{}", spec.chunk_id);
    let mut payload = MetadataMap::new();
    payload.insert("payload_contract_version".to_string(), json!("2026-07-01"));
    payload.insert("collection".to_string(), json!(spec.collection));
    payload.insert("source_family".to_string(), json!("web"));
    payload.insert("source_kind".to_string(), json!("web"));
    payload.insert("source_adapter".to_string(), json!("web"));
    payload.insert("source_scope".to_string(), json!("page"));
    payload.insert("source_id".to_string(), json!("src-docs"));
    payload.insert("source_item_key".to_string(), json!(canonical_uri.clone()));
    payload.insert(
        "item_canonical_uri".to_string(),
        json!(canonical_uri.clone()),
    );
    payload.insert("source_generation".to_string(), json!("7"));
    payload.insert("committed_generation".to_string(), json!("7"));
    payload.insert(
        "document_id".to_string(),
        json!(format!("doc-{}", spec.chunk_id)),
    );
    payload.insert("chunk_id".to_string(), json!(spec.chunk_id));
    payload.insert("chunk_text".to_string(), json!(spec.text));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": canonical_uri,
            "path": format!("/{}", spec.chunk_id),
            "heading_path": [],
            "symbol": null,
            "range": { "line_start": 1, "line_end": 3 }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({ "line_start": 1, "line_end": 3 }),
    );
    payload.insert("visibility".to_string(), json!("internal"));
    payload.insert("redaction_status".to_string(), json!("clean"));
    payload.insert("job_id".to_string(), json!(spec.job_id));
    payload.insert("document_status".to_string(), json!("prepared"));
    payload.insert("embedding_batch_id".to_string(), json!(spec.batch_id));
    payload.insert("embedding_model".to_string(), json!(spec.model));
    payload.insert("embedding_dimensions".to_string(), json!(spec.dimensions));
    payload.insert("embedding_provider".to_string(), json!("fake-embedding"));
    payload.insert("embedding_profile".to_string(), json!("test"));
    payload.insert("embedded_at".to_string(), json!("2026-07-01T00:00:00Z"));
    payload.insert("vector_namespace".to_string(), json!(spec.namespace));
    payload.insert("content_kind".to_string(), json!("markdown"));

    VectorPoint {
        point_id: VectorPointId::new(spec.point_id),
        chunk_id: ChunkId::new(spec.chunk_id),
        vector: spec.vector.to_vec(),
        sparse_vector: None,
        payload,
    }
}

pub fn test_prepared_document() -> PreparedDocument {
    PreparedDocument {
        document_id: DocumentId::new("doc-web"),
        source_id: SourceId::new("src-web"),
        source_item_key: SourceItemKey::new("https://example.com/docs"),
        generation: SourceGenerationId::new("7"),
        canonical_uri: "https://example.com/docs".to_string(),
        prepare_version: "test-preparer".to_string(),
        chunking_profile: "markdown_sections".to_string(),
        chunking_method: "test".to_string(),
        chunks: vec![
            test_prepared_chunk("chunk-web-1", 0, "Intro", 1),
            test_prepared_chunk("chunk-web-2", 1, "Install", 12),
        ],
        metadata: MetadataMap(
            [
                (
                    "embedding_batch_id".to_string(),
                    json!(uuid::Uuid::from_u128(42).to_string()),
                ),
                ("embedding_provider_id".to_string(), json!("fake-embedding")),
                ("embedding_model".to_string(), json!("text-embedding-test")),
                ("source_family".to_string(), json!("web")),
                ("source_kind".to_string(), json!("web")),
                ("source_adapter".to_string(), json!("web")),
                ("source_scope".to_string(), json!("page")),
                (
                    "item_canonical_uri".to_string(),
                    json!("https://example.com/docs"),
                ),
                ("web_title".to_string(), json!("Example Docs")),
                ("web_domain".to_string(), json!("example.com")),
                ("web_status_code".to_string(), json!(200)),
                ("web_depth".to_string(), json!(1)),
            ]
            .into_iter()
            .collect(),
        ),
        cleanup_keys: Vec::new(),
        graph_refs: Vec::new(),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

pub fn test_embedding_result_for(
    document: &PreparedDocument,
    model: impl Into<String>,
    dimensions: u32,
) -> EmbeddingResult {
    EmbeddingResult {
        batch_id: BatchId::new(uuid::Uuid::from_u128(42)),
        job_id: JobId::new(uuid::Uuid::from_u128(43)),
        provider_id: ProviderId::new("fake-embedding"),
        model: model.into(),
        dimensions,
        vectors: document
            .chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| EmbeddingVector {
                chunk_id: chunk.chunk_id.clone(),
                values: test_vector(dimensions, index as f32 + 1.0),
            })
            .collect(),
        usage: ProviderUsage {
            input_tokens: Some(10),
            output_tokens: None,
            requests: 1,
            duration_ms: 1,
        },
        warnings: Vec::new(),
    }
}

pub fn test_embedding_result_with_vectors(
    model: impl Into<String>,
    dimensions: u32,
    vectors: Vec<(&str, Vec<f32>)>,
) -> EmbeddingResult {
    EmbeddingResult {
        batch_id: BatchId::new(uuid::Uuid::from_u128(42)),
        job_id: JobId::new(uuid::Uuid::from_u128(43)),
        provider_id: ProviderId::new("fake-embedding"),
        model: model.into(),
        dimensions,
        vectors: vectors
            .into_iter()
            .map(|(chunk_id, values)| EmbeddingVector {
                chunk_id: ChunkId::new(chunk_id),
                values,
            })
            .collect(),
        usage: ProviderUsage {
            input_tokens: Some(10),
            output_tokens: None,
            requests: 1,
            duration_ms: 1,
        },
        warnings: Vec::new(),
    }
}

pub fn test_vector_build_context() -> VectorPointBatchBuildContext {
    VectorPointBatchBuildContext {
        embedded_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
    }
}

fn test_prepared_chunk(
    chunk_id: impl Into<String>,
    chunk_index: u32,
    title: impl Into<String>,
    line_start: u32,
) -> PreparedChunk {
    let range = SourceRange {
        line_start: Some(line_start),
        line_end: Some(line_start + 3),
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    };
    let chunk_id = ChunkId::new(chunk_id.into());
    PreparedChunk {
        chunk_id: chunk_id.clone(),
        chunk_key: chunk_id.0.clone(),
        document_id: DocumentId::new("doc-web"),
        chunk_index,
        content: format!("{} content", chunk_id.0),
        content_hash: format!("hash-{chunk_index}"),
        embedding_text: None,
        chunk_locator: ChunkLocator {
            canonical_uri: "https://example.com/docs".to_string(),
            path: Some("/docs".to_string()),
            heading_path: vec![title.into()],
            symbol: None,
            range: range.clone(),
        },
        source_range: range,
        content_kind: ContentKind::Markdown,
        title: None,
        graph_refs: Vec::new(),
        parent_chunk_id: None,
        previous_chunk_id: None,
        next_chunk_id: None,
        metadata: MetadataMap::new(),
    }
}

fn test_vector(dimensions: u32, seed: f32) -> Vec<f32> {
    (0..dimensions).map(|index| seed + index as f32).collect()
}
