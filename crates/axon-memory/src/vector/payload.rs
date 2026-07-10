//! Qdrant collection spec and per-record vector payload construction for
//! [`super::VectorBackedMemoryStore`]. Split out of `vector.rs` to keep that
//! file under the monolith line cap — these are pure, `self`-free builders.

use axon_api::source::*;
use serde_json::json;

use super::{MEMORY_COLLECTION_ALIAS, MEMORY_VECTOR_NAMESPACE, MemoryVectorConfig};

pub(super) fn memory_collection_spec(config: &MemoryVectorConfig) -> CollectionSpec {
    CollectionSpec {
        collection: config.collection.clone(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: config.embedding_dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: memory_payload_indexes(),
        sparse: None,
        aliases: vec![MEMORY_COLLECTION_ALIAS.to_string()],
        metadata: MetadataMap::new(),
        distance: Some(VectorDistance::Cosine),
    }
}

pub(super) fn memory_payload_indexes() -> Vec<PayloadIndexSpec> {
    [
        ("vector_namespace", PayloadFieldSchema::Keyword),
        ("memory_id", PayloadFieldSchema::Keyword),
        ("memory_type", PayloadFieldSchema::Keyword),
        ("memory_status", PayloadFieldSchema::Keyword),
        ("memory_scope_kind", PayloadFieldSchema::Keyword),
        ("memory_scope_value", PayloadFieldSchema::Keyword),
        ("redaction_status", PayloadFieldSchema::Keyword),
        ("visibility", PayloadFieldSchema::Keyword),
    ]
    .into_iter()
    .map(|(field_name, field_schema)| PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema,
        required_for_filters: true,
    })
    .collect()
}

pub(super) fn memory_payload(
    record: &MemoryRecord,
    point_id: &VectorPointId,
    embedding: &EmbeddingResult,
    collection: &str,
) -> MetadataMap {
    let mut payload = MetadataMap::new();
    let canonical_uri = format!("memory://{}", record.memory_id.0);
    let chunk_id = format!("memory:{}", record.memory_id.0);
    let content_hash = stable_hash(&record.body);
    let source_range = json!({ "line_start": 1, "line_end": 1 });
    let chunk_locator = json!({
        "canonical_uri": canonical_uri,
        "path": null,
        "heading_path": [],
        "symbol": null,
        "range": source_range,
    });
    payload.insert(
        "payload_contract_version".to_string(),
        json!(axon_vectors::payload::VECTOR_PAYLOAD_CONTRACT_VERSION),
    );
    payload.insert("collection".to_string(), json!(collection));
    payload.insert("vector_point_id".to_string(), json!(point_id.0));
    payload.insert(
        "vector_namespace".to_string(),
        json!(MEMORY_VECTOR_NAMESPACE),
    );
    payload.insert("source_family".to_string(), json!("memory"));
    payload.insert("source_kind".to_string(), json!("memory"));
    payload.insert("source_adapter".to_string(), json!("axon-memory"));
    payload.insert("source_scope".to_string(), json!(record.scope.kind));
    payload.insert("source_id".to_string(), json!(record.memory_id.0));
    payload.insert("source_canonical_uri".to_string(), json!(canonical_uri));
    payload.insert("source_item_key".to_string(), json!(record.memory_id.0));
    payload.insert("item_canonical_uri".to_string(), json!(canonical_uri));
    payload.insert("source_generation".to_string(), json!(0));
    payload.insert("committed_generation".to_string(), json!(0));
    payload.insert("document_id".to_string(), json!(record.memory_id.0));
    payload.insert("chunk_id".to_string(), json!(chunk_id));
    // A memory record is a single atomic point, not a chunk of a larger
    // document — index 0, atomic profile/method. Required by the vector
    // payload shape since the chunking cluster made chunk_index/
    // chunking_profile/chunking_method mandatory (audit S2-18/27).
    payload.insert("chunk_index".to_string(), json!(0));
    payload.insert("chunking_profile".to_string(), json!("atomic_metadata"));
    payload.insert("chunking_method".to_string(), json!("atomic"));
    payload.insert("content_kind".to_string(), json!("plain_text"));
    payload.insert("content_hash".to_string(), json!(content_hash));
    payload.insert("chunk_hash".to_string(), json!(content_hash));
    payload.insert("chunk_locator".to_string(), chunk_locator);
    payload.insert("source_range".to_string(), source_range);
    payload.insert("memory_id".to_string(), json!(record.memory_id.0));
    payload.insert(
        "memory_type".to_string(),
        json!(memory_type_str(record.memory_type)),
    );
    payload.insert(
        "memory_status".to_string(),
        json!(memory_status_str(record.status)),
    );
    payload.insert(
        "memory_recallable".to_string(),
        json!(record.status == MemoryStatus::Active),
    );
    payload.insert("memory_scope_kind".to_string(), json!(record.scope.kind));
    payload.insert("memory_scope_value".to_string(), json!(record.scope.value));
    payload.insert("memory_confidence".to_string(), json!(record.confidence));
    payload.insert("memory_salience".to_string(), json!(record.salience));
    payload.insert("redaction_status".to_string(), json!("clean"));
    payload.insert("redaction_version".to_string(), json!("2026-07-04"));
    payload.insert("visibility".to_string(), json!("public"));
    payload.insert("redacted_field_count".to_string(), json!(0));
    payload.insert("dropped_field_count".to_string(), json!(0));
    payload.insert("detector_names".to_string(), json!([]));
    payload.insert("chunk_text".to_string(), json!(record.body));
    payload.insert(
        "embedding_provider".to_string(),
        json!(embedding.provider_id.0),
    );
    payload.insert(
        "embedding_batch_id".to_string(),
        json!(embedding.batch_id.0.to_string()),
    );
    payload.insert("embedding_model".to_string(), json!(embedding.model));
    payload.insert(
        "embedding_dimensions".to_string(),
        json!(embedding.dimensions),
    );
    payload.insert("embedding_profile".to_string(), json!("memory"));
    payload.insert("embedded_at".to_string(), json!("2026-07-04T00:00:00Z"));
    payload.insert("job_id".to_string(), json!(embedding.job_id.0.to_string()));
    payload.insert("document_status".to_string(), json!("published"));
    payload
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

fn stable_hash(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}
