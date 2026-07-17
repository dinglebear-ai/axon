//! Structured record and metadata chunk builders.

use axon_api::source::ContentKind;

use crate::chunk::DocumentChunk;
use crate::structured_formats::{csv_records, toml_records, xml_records, yaml_records};
use crate::text::{atomic_text, source_range};

/// Detected structured format, from an explicit content kind or a path
/// extension. `Json` is the default/fallback (also covers `Structured`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuredFormat {
    Json,
    Yaml,
    Toml,
    Csv,
    Xml,
}

fn detect_format(content_kind: ContentKind, path: Option<&str>) -> StructuredFormat {
    match content_kind {
        ContentKind::Yaml => return StructuredFormat::Yaml,
        ContentKind::Toml => return StructuredFormat::Toml,
        ContentKind::Xml => return StructuredFormat::Xml,
        _ => {}
    }
    let Some(path) = path else {
        return StructuredFormat::Json;
    };
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "yaml" | "yml" => StructuredFormat::Yaml,
        "toml" => StructuredFormat::Toml,
        "csv" | "tsv" => StructuredFormat::Csv,
        "xml" => StructuredFormat::Xml,
        _ => StructuredFormat::Json,
    }
}

pub(crate) fn structured_records(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
    content_kind: ContentKind,
    path: Option<&str>,
) -> Result<Vec<DocumentChunk>, String> {
    if let Some(value) = structured_payload {
        let mut chunks = Vec::new();
        if !text.trim().is_empty() {
            // No `structured_payload_*` marker metadata here: chunk metadata
            // flows into the vector payload, whose family allowlists reject
            // unknown keys fail-closed (same class as the `json_key` break —
            // see `chunks_from_json_value`). Nothing reads these markers.
            chunks.extend(atomic_text(text));
        }
        chunks.extend(chunks_from_json_value(value));
        return Ok(chunks);
    }

    match detect_format(content_kind, path) {
        StructuredFormat::Yaml => non_empty_or_err(yaml_records(text), "yaml"),
        StructuredFormat::Toml => non_empty_or_err(toml_records(text), "toml"),
        StructuredFormat::Csv => non_empty_or_err(csv_records(text), "csv"),
        StructuredFormat::Xml => non_empty_or_err(xml_records(text), "xml"),
        StructuredFormat::Json => serde_json::from_str::<serde_json::Value>(text)
            .map(|value| chunks_from_json_value(&value))
            .map_err(|error| error.to_string()),
    }
}

fn non_empty_or_err(
    chunks: Vec<DocumentChunk>,
    format: &str,
) -> Result<Vec<DocumentChunk>, String> {
    if chunks.is_empty() {
        Err(format!("no {format} records recognized"))
    } else {
        Ok(chunks)
    }
}

pub(crate) fn atomic_metadata(text: &str) -> Vec<DocumentChunk> {
    atomic_text(text)
}

/// Structured-JSON chunks carry their identity in the chunk's `SourceRange`
/// (`json_pointer`), which is a contract-legal anchor field. Do NOT attach
/// chunker-internal keys (`json_key`/`json_index`/`json_pointer`/
/// `synthetic_source_range`) as chunk METADATA: chunk metadata flows into the
/// vector payload, whose per-source-family field allowlist fail-closes on
/// unknown keys — one structured chunk then fails the whole source (seen live
/// as "unknown vector payload field `json_key` for source family").
fn chunks_from_json_value(value: &serde_json::Value) -> Vec<DocumentChunk> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let pointer = format!("/{idx}");
                DocumentChunk::new(item.to_string(), json_range(pointer))
            })
            .collect(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, item)| {
                let pointer = format!("/{}", pointer_escape(key));
                DocumentChunk::new(item.to_string(), json_range(pointer))
            })
            .collect(),
        _ => vec![DocumentChunk::new(
            value.to_string(),
            json_range(String::new()),
        )],
    }
}

fn json_range(pointer: String) -> axon_api::source::SourceRange {
    let mut range = source_range("", 0, 0);
    range.json_pointer = Some(pointer);
    range
}

fn pointer_escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
