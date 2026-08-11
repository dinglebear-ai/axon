use std::{env, fs, process};

use axon_api::source::{
    ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument, SourceGenerationId, SourceId,
    SourceItemKey,
};
use axon_core::redact::secret_value_detector;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use serde_json::json;

struct ArtifactSidecar {
    content_kind: ContentKind,
    metadata: MetadataMap,
}

struct ProbeInput {
    content_path: String,
    document_id: String,
    display_path: Option<String>,
    content_kind: ContentKind,
    metadata: MetadataMap,
}

type ProbeResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("redaction probe failed: {error}");
        process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let mut args = env::args().skip(1);
    let first = args
        .next()
        .ok_or("missing probe mode or sidecar JSON path")?;
    let input = if first == "--synthetic-web" || first == "--synthetic-web-html" {
        synthetic_web_input(first, &mut args)?
    } else {
        sidecar_input(first, &mut args)?
    };
    let canonical_uri = required_metadata(&input.metadata, "canonical_uri")
        .or_else(|_| required_metadata(&input.metadata, "item_canonical_uri"))?;
    let prepared = DocumentPreparer::default()
        .prepare(prepare_request(input)?)?
        .document;
    report_rejections(&canonical_uri, &prepared);
    Ok(())
}

fn synthetic_web_input(
    mode: String,
    args: &mut impl Iterator<Item = String>,
) -> ProbeResult<ProbeInput> {
    let content_kind = if mode == "--synthetic-web-html" {
        ContentKind::Html
    } else {
        ContentKind::Markdown
    };
    let canonical_uri = args.next().ok_or("missing canonical URI")?;
    let content_path = args.next().ok_or("missing normalized content path")?;
    let source_item_key = args.next().ok_or("missing source item key")?;
    let document_id = args.next().ok_or("missing document id")?;
    let display_path = args.next();
    let web_path = canonical_uri
        .strip_prefix("https://gofastmcp.com")
        .unwrap_or(canonical_uri.as_str());
    let mut metadata = MetadataMap::new();
    for (key, value) in [
        ("source_family", "web"),
        ("source_kind", "web"),
        ("source_adapter", "web"),
        ("source_scope", "site"),
        ("source_id", "src_38bb96f3814aa5ba"),
        ("source_canonical_uri", "https://gofastmcp.com/"),
        ("source_item_key", source_item_key.as_str()),
        ("item_canonical_uri", canonical_uri.as_str()),
        ("source_generation", "gen_6"),
        ("committed_generation", "uncommitted"),
        ("normalization_version", "web-url-v1"),
        ("web_url", canonical_uri.as_str()),
        ("web_seed_url", "https://gofastmcp.com/"),
        ("web_domain", "gofastmcp.com"),
        ("web_origin", "https://gofastmcp.com"),
        ("web_path", web_path),
        ("web_normalized_url", canonical_uri.as_str()),
        ("web_fetch_method", "auto_switch_http"),
        ("web_render_mode", "http"),
    ] {
        metadata.insert(key.to_string(), json!(value));
    }
    Ok(ProbeInput {
        content_path,
        document_id,
        display_path,
        content_kind,
        metadata,
    })
}

fn sidecar_input(
    sidecar_path: String,
    args: &mut impl Iterator<Item = String>,
) -> ProbeResult<ProbeInput> {
    let content_path = args.next().ok_or("missing normalized content path")?;
    let document_id = args.next().ok_or("missing document id")?;
    let display_path = args.next();
    let sidecar_value: serde_json::Value = serde_json::from_slice(&fs::read(sidecar_path)?)?;
    let sidecar = ArtifactSidecar {
        content_kind: serde_json::from_value(
            sidecar_value
                .get("content_kind")
                .cloned()
                .ok_or("sidecar missing content_kind")?,
        )?,
        metadata: serde_json::from_value(
            sidecar_value
                .get("metadata")
                .cloned()
                .ok_or("sidecar missing metadata")?,
        )?,
    };
    Ok(ProbeInput {
        content_path,
        document_id,
        display_path,
        content_kind: sidecar.content_kind,
        metadata: sidecar.metadata,
    })
}

fn prepare_request(input: ProbeInput) -> ProbeResult<PrepareSourceDocumentRequest> {
    let content = fs::read_to_string(input.content_path)?;
    let source_id = required_metadata(&input.metadata, "source_id")?;
    let source_item_key = required_metadata(&input.metadata, "source_item_key")?;
    let canonical_uri = required_metadata(&input.metadata, "canonical_uri")
        .or_else(|_| required_metadata(&input.metadata, "item_canonical_uri"))?;
    let generation = required_metadata(&input.metadata, "source_generation")?;
    Ok(PrepareSourceDocumentRequest {
        document: SourceDocument {
            document_id: DocumentId::new(input.document_id),
            source_id: SourceId::new(source_id),
            source_item_key: SourceItemKey::new(source_item_key),
            canonical_uri,
            content_kind: input.content_kind,
            content: ContentRef::InlineText { text: content },
            metadata: input.metadata,
            title: None,
            language: None,
            path: input.display_path,
            mime_type: Some("text/markdown".to_string()),
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
        generation: SourceGenerationId::new(generation),
        profile: None,
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    })
}

fn report_rejections(canonical_uri: &str, prepared: &axon_api::source::PreparedDocument) {
    let mut rejected = 0usize;
    for chunk in &prepared.chunks {
        let triggers = body_triggers(&chunk.content);
        if triggers.detector.is_none() {
            continue;
        }
        rejected += 1;
        println!(
            "{}",
            json!({
                "canonical_uri": canonical_uri,
                "chunk_id": chunk.chunk_id.0,
                "chunk_index": chunk.chunk_index,
                "line_start": chunk.source_range.line_start,
                "line_end": chunk.source_range.line_end,
                "heading_path": chunk.chunk_locator.heading_path,
                "content_bytes": chunk.content.len(),
                "detector": triggers.detector,
            })
        );
    }
    eprintln!(
        "probe summary: uri={canonical_uri} chunks={} content_bytes={} rejected={rejected}",
        prepared.chunks.len(),
        prepared
            .chunks
            .iter()
            .map(|chunk| chunk.content.len())
            .sum::<usize>()
    );
}

fn required_metadata(
    metadata: &MetadataMap,
    field: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    metadata
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing string metadata field `{field}`").into())
}

#[derive(Default)]
struct BodyTriggers {
    detector: Option<&'static str>,
}

fn body_triggers(value: &str) -> BodyTriggers {
    BodyTriggers {
        detector: secret_value_detector(value),
    }
}
