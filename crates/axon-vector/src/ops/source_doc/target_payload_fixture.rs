use serde_json::{Map, Value};

use super::{LedgerPayload, PreparedDoc};
use axon_vectors::payload::VECTOR_PAYLOAD_CONTRACT_VERSION;

/// Locally-derived values needed to build the target payload for one chunk —
/// split out of `target_vector_payload_fixture_for_chunk` to keep it under
/// the monolith function-length cap.
struct TargetChunkFields<'a> {
    source_family: &'static str,
    source_generation: i64,
    source_id: String,
    source_item_key: String,
    chunk_id: String,
    chunk_index: usize,
    chunk_key: String,
    content_hash: String,
    chunk_locator: String,
    source_range: Value,
    chunk: &'a str,
}

fn derive_target_chunk_fields<'a>(
    doc: &'a PreparedDoc,
    chunk_index: usize,
    chunk: &'a str,
    chunk_extra: Option<&Map<String, Value>>,
) -> TargetChunkFields<'a> {
    // Integer-typed per the vector-payload contract (`PayloadFieldSchema::
    // Integer`) — never a string, even in this legacy-vector-bridge fixture.
    let source_generation: i64 = doc
        .ledger_payload
        .as_ref()
        .map(LedgerPayload::generation)
        .unwrap_or(1);
    let source_id = doc
        .ledger_payload
        .as_ref()
        .map(|ledger| ledger.source_id().to_string())
        .unwrap_or_else(|| format!("legacy-vector:{}", fnv1a64(&doc.url)));
    let source_item_key = doc
        .ledger_payload
        .as_ref()
        .map(|ledger| target_safe_uri(ledger.item_key()))
        .unwrap_or_else(|| target_safe_uri(&doc.url));
    let chunk_id = chunk_extra
        .and_then(|extra| extra.get("prepared_chunk_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("chunk_{chunk_index}"));
    let chunk_key = chunk_extra
        .and_then(|extra| extra.get("prepared_chunk_key"))
        .and_then(Value::as_str)
        .map(target_safe_locator)
        .unwrap_or_else(|| format!("{}:{chunk_index}", target_safe_uri(&doc.url)));
    let content_hash = chunk_extra
        .and_then(|extra| extra.get("prepared_content_hash"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fnv1a64(chunk));
    let chunk_locator = chunk_extra
        .and_then(|extra| extra.get("chunk_locator"))
        .and_then(Value::as_str)
        .map(target_safe_locator)
        .unwrap_or_else(|| format!("{}#chunk-{chunk_index}", target_safe_uri(&doc.url)));
    let source_range = chunk_extra
        .and_then(|extra| extra.get("source_range"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    TargetChunkFields {
        source_family: target_source_family(doc, chunk_extra),
        source_generation,
        source_id,
        source_item_key,
        chunk_id,
        chunk_index,
        chunk_key,
        content_hash,
        chunk_locator,
        source_range,
        chunk,
    }
}

fn build_target_payload(
    doc: &PreparedDoc,
    collection: &str,
    f: TargetChunkFields<'_>,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert(
        "payload_contract_version".to_string(),
        VECTOR_PAYLOAD_CONTRACT_VERSION.into(),
    );
    payload.insert("collection".to_string(), collection.into());
    payload.insert(
        "vector_point_id".to_string(),
        format!(
            "legacy-vector:{}",
            fnv1a64(&format!("{}#{}", doc.url, f.chunk_id))
        )
        .into(),
    );
    payload.insert(
        "vector_namespace".to_string(),
        // Memory's namespace is the bare literal "memory" (see
        // `axon_retrieval::memory::MEMORY_VECTOR_NAMESPACE`); every other
        // source family namespaces as "source:<family>".
        if f.source_family == "memory" {
            "memory".to_string()
        } else {
            format!("source:{}", f.source_family)
        }
        .into(),
    );
    payload.insert("source_family".to_string(), f.source_family.into());
    payload.insert("source_type".to_string(), doc.source_type.clone().into());
    payload.insert("source_kind".to_string(), target_source_kind(doc).into());
    payload.insert(
        "source_adapter".to_string(),
        target_source_adapter(doc).into(),
    );
    payload.insert("source_scope".to_string(), target_source_scope(doc).into());
    payload.insert("source_id".to_string(), f.source_id.into());
    payload.insert("source_item_key".to_string(), f.source_item_key.into());
    payload.insert(
        "source_canonical_uri".to_string(),
        target_safe_uri(&doc.url).into(),
    );
    payload.insert(
        "item_canonical_uri".to_string(),
        target_safe_uri(&doc.url).into(),
    );
    payload.insert("source_generation".to_string(), f.source_generation.into());
    payload.insert(
        "committed_generation".to_string(),
        f.source_generation.into(),
    );
    payload.insert(
        "document_id".to_string(),
        format!("legacy-vector:{}", fnv1a64(&doc.url)).into(),
    );
    payload.insert("chunk_id".to_string(), f.chunk_id.into());
    payload.insert("chunk_index".to_string(), (f.chunk_index as i64).into());
    payload.insert("chunking_profile".to_string(), "markdown_sections".into());
    payload.insert("chunking_method".to_string(), "heading_sections".into());
    payload.insert("chunk_text".to_string(), f.chunk.into());
    payload.insert("chunk_key".to_string(), f.chunk_key.clone().into());
    payload.insert("content_hash".to_string(), f.content_hash.clone().into());
    payload.insert("chunk_hash".to_string(), f.content_hash.into());
    payload.insert(
        "chunk_locator".to_string(),
        serde_json::json!({
            "canonical_uri": f.chunk_locator,
            "path": f.chunk_key,
            "heading_path": [],
            "symbol": Value::Null,
            "range": f.source_range.clone(),
        }),
    );
    payload.insert("source_range".to_string(), f.source_range);
    payload.insert("visibility".to_string(), "internal".into());
    payload.insert("redaction_status".to_string(), "clean".into());
    payload.insert(
        "job_id".to_string(),
        "00000000-0000-0000-0000-000000000000".into(),
    );
    payload.insert(
        "embedding_batch_id".to_string(),
        "00000000-0000-0000-0000-000000000001".into(),
    );
    payload.insert("document_status".to_string(), "prepared".into());
    payload.insert("embedding_model".to_string(), "legacy-vector-bridge".into());
    payload.insert("embedding_dimensions".to_string(), 1.into());
    payload.insert("embedding_provider".to_string(), "legacy-vector".into());
    payload.insert("embedding_profile".to_string(), doc.content_type.into());
    payload.insert("embedded_at".to_string(), "1970-01-01T00:00:00Z".into());
    payload
}

pub(in crate::ops) fn target_vector_payload_fixture_for_chunk(
    doc: &PreparedDoc,
    chunk_index: usize,
    collection: &str,
) -> Result<Value, String> {
    let chunk = doc
        .chunks
        .get(chunk_index)
        .ok_or_else(|| format!("chunk index {chunk_index} out of bounds"))?;
    let chunk_extra = doc.chunk_extra.get(chunk_index).and_then(Value::as_object);
    let doc_extra = doc.extra.as_ref().and_then(Value::as_object);

    let fields = derive_target_chunk_fields(doc, chunk_index, chunk, chunk_extra);
    let content_kind = chunk_extra
        .and_then(|extra| extra.get("chunk_content_kind"))
        .and_then(Value::as_str)
        .unwrap_or("plain_text")
        .to_string();
    let mut payload = build_target_payload(doc, collection, fields);
    payload.insert("content_kind".to_string(), content_kind.into());

    enrich_source_specific_fields(&mut payload, doc, chunk_extra, doc_extra);
    validate_target_payload(payload)
}

fn enrich_source_specific_fields(
    payload: &mut Map<String, Value>,
    doc: &PreparedDoc,
    chunk_extra: Option<&Map<String, Value>>,
    doc_extra: Option<&Map<String, Value>>,
) {
    match payload
        .get("source_family")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "code" => {
            insert_target_code_field(payload, doc_extra, "code_language");
            insert_target_code_field(payload, doc_extra, "code_file_type");
            if let Some(symbol) = chunk_extra
                .and_then(|extra| extra.get("symbol_name"))
                .and_then(Value::as_str)
            {
                payload.insert("code_symbol_name".to_string(), symbol.into());
            }
            if let Some(kind) = chunk_extra
                .and_then(|extra| extra.get("symbol_kind"))
                .and_then(Value::as_str)
            {
                payload.insert("code_symbol_kind".to_string(), kind.into());
            }
        }
        "memory" => {
            payload.insert("memory_id".to_string(), doc.url.clone().into());
            payload.insert("memory_status".to_string(), "active".into());
        }
        "web" => {
            insert_source_field(payload, doc_extra, "web_title");
            insert_source_field(payload, doc_extra, "web_domain");
            insert_source_field(payload, doc_extra, "web_status_code");
            insert_source_field(payload, doc_extra, "web_depth");
        }
        _ => {}
    }
}

fn validate_target_payload(payload: Map<String, Value>) -> Result<Value, String> {
    let metadata = axon_api::source::MetadataMap(payload.clone().into_iter().collect());
    axon_vectors::payload::VectorPayload::try_from_metadata(metadata)
        .map_err(|err| format!("target vector payload validation failed: {err}"))?;
    Ok(Value::Object(payload))
}

fn target_source_family(
    doc: &PreparedDoc,
    chunk_extra: Option<&Map<String, Value>>,
) -> &'static str {
    if doc.source_type == "memory" {
        return "memory";
    }
    if doc.source_type == "session" {
        return "session";
    }
    if matches!(
        doc.source_type.as_str(),
        "package" | "crates" | "npm" | "pypi"
    ) {
        return "package";
    }
    if chunk_extra
        .and_then(|extra| extra.get("chunk_content_kind"))
        .and_then(Value::as_str)
        == Some("code")
    {
        return "code";
    }
    "web"
}

fn target_source_kind(doc: &PreparedDoc) -> &'static str {
    match doc.source_type.as_str() {
        "local_code" => "local",
        "github" | "gitlab" | "gitea" | "forgejo" | "git" => "repo",
        "memory" => "memory",
        "session" => "session",
        "package" | "crates" | "npm" | "pypi" => "package",
        _ if doc.url.starts_with("file://") => "local",
        _ if doc.url.starts_with("http://") || doc.url.starts_with("https://") => "web",
        _ => "unknown",
    }
}

fn target_source_adapter(doc: &PreparedDoc) -> &'static str {
    match doc.source_type.as_str() {
        "local_code" => "local",
        "github" | "gitlab" | "gitea" | "forgejo" | "git" => "git",
        "memory" => "memory",
        "session" => "session",
        "package" | "crates" | "npm" | "pypi" => "package",
        "crawl" | "scrape" => "web",
        _ => "legacy-vector",
    }
}

fn target_source_scope(doc: &PreparedDoc) -> &'static str {
    match doc.source_type.as_str() {
        "local_code" => "file",
        "github" | "gitlab" | "gitea" | "forgejo" | "git" => "repo",
        "memory" => "item",
        "session" => "session",
        "package" | "crates" | "npm" | "pypi" => "package",
        "crawl" => "site",
        "scrape" => "page",
        _ => "item",
    }
}

fn insert_target_code_field(
    payload: &mut Map<String, Value>,
    doc_extra: Option<&Map<String, Value>>,
    field: &str,
) {
    insert_source_field(payload, doc_extra, field);
}

fn insert_source_field(
    payload: &mut Map<String, Value>,
    doc_extra: Option<&Map<String, Value>>,
    field: &str,
) {
    if let Some(value) = doc_extra.and_then(|extra| extra.get(field)).cloned() {
        payload.insert(field.to_string(), value);
    }
}

fn target_safe_locator(locator: &str) -> String {
    let (path, fragment) = locator.split_once('#').unwrap_or((locator, ""));
    let safe_path = target_safe_uri(path);
    if safe_path == path {
        return locator.to_string();
    }
    if fragment.is_empty() {
        safe_path
    } else {
        format!("{safe_path}#{fragment}")
    }
}

fn target_safe_uri(uri: &str) -> String {
    if let Some(path) = uri.strip_prefix("file://") {
        return target_safe_path_uri(path);
    }
    if looks_like_absolute_path(uri) {
        return target_safe_path_uri(uri);
    }
    uri.to_string()
}

fn target_safe_path_uri(path: &str) -> String {
    let file_name = path
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or("local-file");
    format!("file://local/{}/{file_name}", fnv1a64(path))
}

fn looks_like_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with('~')
        || value.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

fn fnv1a64(value: &str) -> String {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut hash = FNV_OFFSET;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("fnv1a64:{hash:016x}")
}
