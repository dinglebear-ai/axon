//! Payload-merge helpers for the ingest job runner.
//!
//! Extracted from the parent module to keep `ingest.rs` within the monolith
//! 500-line limit. The split is tracked in `.monolith-allowlist`.

// --- Canonical payload field names (B-H3: typed key constants, no stringly-keyed literals) ---

pub(super) const KEY_CHUNKS_EMBEDDED: &str = "chunks_embedded";
/// Legacy alias kept for backward-compatible API consumers; mirrors `chunks_embedded`.
pub(super) const KEY_CHUNKS: &str = "chunks";
pub(super) const KEY_PHASE: &str = "phase";
pub(super) const KEY_PROGRESS_WARNING: &str = "progress_warning";
pub(super) const KEY_RESULT_JSON_WARNING: &str = "result_json_warning";
/// Quality counters threaded from ingest services (O-M4).
pub(super) const KEY_PAGES_TOTAL: &str = "pages_total";
pub(super) const KEY_PAGES_DROPPED_THIN: &str = "pages_dropped_thin";
pub(super) const KEY_FILES_AST_CHUNKED: &str = "files_ast_chunked";
pub(super) const KEY_FILES_PROSE_FALLBACK: &str = "files_prose_fallback";

pub(super) fn merge_progress(
    current: &mut serde_json::Value,
    progress: serde_json::Value,
    job_id: uuid::Uuid,
    source_type: &str,
    target: &str,
) {
    if let serde_json::Value::Object(progress) = progress
        && let Some(current) = current.as_object_mut()
    {
        for (key, value) in progress {
            current.insert(key, value);
        }
        return;
    }

    let warning = progress_warning(job_id, source_type, "progress update was not a JSON object");
    tracing::warn!(
        job_id = %job_id,
        source_type,
        target,
        "ignoring malformed ingest progress update: progress update was not a JSON object"
    );
    if let Some(current) = current.as_object_mut() {
        current.insert(
            KEY_PROGRESS_WARNING.to_string(),
            serde_json::Value::String(warning),
        );
    } else {
        let mut replacement = serde_json::Map::new();
        replacement.insert(
            KEY_PROGRESS_WARNING.to_string(),
            serde_json::Value::String(warning),
        );
        *current = serde_json::Value::Object(replacement);
    }
}

pub(super) fn current_progress_from_result_json(
    job_id: uuid::Uuid,
    source_type: &str,
    target: &str,
    result_json: Option<String>,
) -> serde_json::Value {
    let Some(result_json) = result_json else {
        return serde_json::Value::Object(serde_json::Map::new());
    };

    match serde_json::from_str::<serde_json::Value>(&result_json) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        Ok(value) => {
            let detail = format!(
                "stored result_json was {}, not a JSON object",
                json_type(&value)
            );
            tracing::warn!(
                job_id = %job_id,
                source_type,
                target,
                value_type = json_type(&value),
                "ignoring malformed ingest result_json: stored result_json was not a JSON object"
            );
            warning_object(
                KEY_RESULT_JSON_WARNING,
                result_json_warning(job_id, source_type, &detail),
            )
        }
        Err(e) => {
            let detail = format!("stored result_json was invalid JSON: {e}");
            tracing::warn!(
                job_id = %job_id,
                source_type,
                target,
                error = %e,
                "ignoring malformed ingest result_json: stored result_json was invalid JSON"
            );
            warning_object(
                KEY_RESULT_JSON_WARNING,
                result_json_warning(job_id, source_type, &detail),
            )
        }
    }
}

pub(super) fn merge_final_payload(
    current_progress: serde_json::Value,
    final_payload: serde_json::Value,
) -> serde_json::Value {
    let mut merged = current_progress
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);

    // Snapshot fields that must survive the merge.
    let progress_warning = merged.get(KEY_PROGRESS_WARNING).cloned();
    let result_json_warning = merged.get(KEY_RESULT_JSON_WARNING).cloned();
    let pages_total = merged.get(KEY_PAGES_TOTAL).cloned();
    let pages_dropped_thin = merged.get(KEY_PAGES_DROPPED_THIN).cloned();
    let files_ast_chunked = merged.get(KEY_FILES_AST_CHUNKED).cloned();
    let files_prose_fallback = merged.get(KEY_FILES_PROSE_FALLBACK).cloned();

    if let serde_json::Value::Object(final_object) = final_payload {
        for (key, value) in final_object {
            merged.insert(key, value);
        }
    }

    // Restore fields that must not be clobbered by the final result payload.
    restore_field(&mut merged, KEY_PROGRESS_WARNING, progress_warning);
    restore_field(&mut merged, KEY_RESULT_JSON_WARNING, result_json_warning);
    // Ingestion-quality counters (O-M4) come from progress events; final payload may not carry them.
    restore_field(&mut merged, KEY_PAGES_TOTAL, pages_total);
    restore_field(&mut merged, KEY_PAGES_DROPPED_THIN, pages_dropped_thin);
    restore_field(&mut merged, KEY_FILES_AST_CHUNKED, files_ast_chunked);
    restore_field(&mut merged, KEY_FILES_PROSE_FALLBACK, files_prose_fallback);

    // Ensure both chunk-count aliases are present for backward-compatible callers.
    if !merged.contains_key(KEY_CHUNKS_EMBEDDED)
        && let Some(chunks) = merged.get(KEY_CHUNKS).cloned()
    {
        merged.insert(KEY_CHUNKS_EMBEDDED.to_string(), chunks);
    }
    if !merged.contains_key(KEY_CHUNKS)
        && let Some(chunks) = merged.get(KEY_CHUNKS_EMBEDDED).cloned()
    {
        merged.insert(KEY_CHUNKS.to_string(), chunks);
    }
    merged.insert(
        KEY_PHASE.to_string(),
        serde_json::Value::String("completed".to_string()),
    );

    serde_json::Value::Object(merged)
}

fn restore_field(
    merged: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<serde_json::Value>,
) {
    if let Some(v) = value {
        merged.insert(key.to_string(), v);
    }
}

fn warning_object(field: &str, warning: String) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(field.to_string(), serde_json::Value::String(warning));
    serde_json::Value::Object(object)
}

fn progress_warning(job_id: uuid::Uuid, source_type: &str, detail: &str) -> String {
    format!("job_id={job_id} source={source_type}: {detail}")
}

fn result_json_warning(job_id: uuid::Uuid, source_type: &str, detail: &str) -> String {
    format!("job_id={job_id} source={source_type}: {detail}")
}

pub(super) fn json_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}
