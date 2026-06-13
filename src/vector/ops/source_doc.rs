use serde_json::{Map, Value};

use super::file_ingest::{chunk_file, chunking_method};
use super::input::classify::{classify_file_type, is_test_path, language_name};
use super::input::code::code_symbol_extraction_status;
use super::input::{chunk_markdown, chunk_text, chunk_text_with_offsets};
use super::tei::{PreparedDoc, StructuredPayload};

mod support;

use support::{LineIndex, domain_for_origin, domain_from_web_url, file_locator, locate_chunk};

const PLANNER_OWNED_PAYLOAD_KEYS: &[&str] = &[
    "content_kind",
    "chunk_content_kind",
    "chunk_locator",
    "source_range",
    "chunking_fallback",
    "code_chunk_source",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceOrigin {
    LocalFile,
    GitFile,
    CrawlManifest,
    ScrapeResult,
    PlainIngest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceChunkHint {
    File { path: String, extension: String },
    MarkdownOrPlainText,
    PlainText,
}

#[derive(Debug, Clone)]
pub(crate) struct SourceDocument {
    origin: SourceOrigin,
    url: String,
    domain: String,
    text: String,
    source_type: String,
    title: Option<String>,
    extra: Option<Value>,
    extractor_name: Option<String>,
    structured: Option<StructuredPayload>,
    chunk_hint: SourceChunkHint,
}

impl SourceDocument {
    #[allow(clippy::too_many_arguments)]
    fn new(
        origin: SourceOrigin,
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
        extractor_name: Option<String>,
        structured: Option<StructuredPayload>,
        chunk_hint: SourceChunkHint,
    ) -> Self {
        Self {
            origin,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name,
            structured,
            chunk_hint,
        }
    }

    pub(crate) fn try_new_web_markdown(
        url: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
        extractor_name: Option<String>,
        structured: Option<StructuredPayload>,
    ) -> Result<Self, String> {
        let domain = domain_from_web_url(&url)?;
        Ok(Self::new(
            SourceOrigin::ScrapeResult,
            url,
            domain,
            text,
            source_type,
            title,
            extra,
            extractor_name,
            structured,
            SourceChunkHint::MarkdownOrPlainText,
        ))
    }

    pub(crate) fn try_new_crawl_manifest(
        url: String,
        text: String,
        title: Option<String>,
        structured: Option<StructuredPayload>,
    ) -> Result<Self, String> {
        let domain = domain_from_web_url(&url)?;
        Ok(Self::new(
            SourceOrigin::CrawlManifest,
            url,
            domain,
            text,
            "crawl",
            title,
            None,
            None,
            structured,
            SourceChunkHint::MarkdownOrPlainText,
        ))
    }

    pub(crate) fn new_local_markdown(
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
    ) -> Self {
        Self::new(
            SourceOrigin::LocalFile,
            url,
            domain,
            text,
            source_type,
            title,
            extra,
            None,
            None,
            SourceChunkHint::MarkdownOrPlainText,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_new_file(
        origin: SourceOrigin,
        url: String,
        path: String,
        extension: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
    ) -> Result<Self, String> {
        if !matches!(origin, SourceOrigin::LocalFile | SourceOrigin::GitFile) {
            return Err("file chunking is only allowed for local and git file origins".to_string());
        }
        let domain = domain_for_origin(origin, &url);
        Ok(Self::new(
            origin,
            url,
            domain,
            text,
            source_type,
            title,
            extra,
            None,
            None,
            SourceChunkHint::File { path, extension },
        ))
    }

    pub(crate) fn new_plain_text(
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
    ) -> Self {
        Self::new(
            SourceOrigin::PlainIngest,
            url,
            domain,
            text,
            source_type,
            title,
            extra,
            None,
            None,
            SourceChunkHint::PlainText,
        )
    }

    fn into_prepared(
        self,
        chunks: Vec<String>,
        content_type: &'static str,
        chunk_extra: Vec<Value>,
    ) -> PreparedDoc {
        PreparedDoc::from_planned_chunks(
            self.url,
            self.domain,
            chunks,
            self.source_type,
            content_type,
            self.title,
            self.extra,
            self.extractor_name,
            self.structured,
            chunk_extra,
        )
    }
}

pub(crate) async fn prepare_source_document(doc: SourceDocument) -> Result<PreparedDoc, String> {
    match doc.chunk_hint.clone() {
        SourceChunkHint::File { path, extension } => {
            prepare_file_source(doc, path, extension).await
        }
        SourceChunkHint::MarkdownOrPlainText => Ok(prepare_markdown_source(doc)),
        SourceChunkHint::PlainText => Ok(prepare_plain_source(doc)),
    }
}

pub(crate) fn prepare_plain_text_source(
    url: String,
    domain: String,
    text: String,
    source_type: impl Into<String>,
    title: Option<String>,
    extra: Option<Value>,
) -> Result<PreparedDoc, String> {
    let source = SourceDocument::new_plain_text(url, domain, text, source_type, title, extra);
    Ok(prepare_plain_source(source))
}

pub(crate) fn structured_payload_from_vertical_summary(
    extractor_name: &str,
    value: Value,
    max_bytes: usize,
) -> Option<StructuredPayload> {
    let blob_bytes = serde_json::to_vec(&value).ok()?;
    if blob_bytes.len() > max_bytes {
        return None;
    }
    let schema_type = value
        .get("@type")
        .or_else(|| value.get("type"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let schema_id = value
        .get("@id")
        .or_else(|| value.get("id"))
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Some(StructuredPayload {
        kind: "vertical",
        schema_type: Some(schema_type.unwrap_or_else(|| format!("{extractor_name}_structured"))),
        schema_id,
        blob: value,
    })
}

async fn prepare_file_source(
    doc: SourceDocument,
    path: String,
    extension: String,
) -> Result<PreparedDoc, String> {
    if !matches!(doc.origin, SourceOrigin::LocalFile | SourceOrigin::GitFile) {
        return Err("file chunking is only allowed for local and git file origins".to_string());
    }
    let text = doc.text.clone();
    let ext = extension.to_ascii_lowercase();
    let chunks = tokio::task::spawn_blocking({
        let text = text.clone();
        let ext = ext.clone();
        move || chunk_file(&text, &ext)
    })
    .await
    .map_err(|e| format!("chunk_file panicked for {}: {e}", doc.url))?;

    let symbol_status = code_symbol_extraction_status(&text, &ext, &chunks);
    let mut chunk_texts = Vec::with_capacity(chunks.len());
    let mut chunk_extra = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let method = chunking_method(&ext, &chunk);
        let content_kind = match method {
            "tree_sitter" => "code",
            "markdown" => "markdown",
            "prose" => "plain_text",
            _ => "plain_text",
        };
        let mut extra = base_chunk_metadata(
            content_kind,
            &file_locator(&path, chunk.start_line, chunk.end_line),
            chunk.start_line,
            chunk.end_line,
            chunk.byte_start,
            chunk.byte_end,
        );
        extra.insert("code_line_start".into(), chunk.start_line.into());
        extra.insert("code_line_end".into(), chunk.end_line.into());
        extra.insert("code_chunking_method".into(), method.into());
        extra.insert("code_chunk_source".into(), method.into());
        if let Some(name) = chunk.symbol_name() {
            extra.insert("symbol_name".into(), name.into());
        }
        if let Some(kind) = chunk.symbol_kind_str() {
            extra.insert("symbol_kind".into(), kind.into());
        }
        chunk_texts.push(chunk.text);
        chunk_extra.push(chunk_metadata(extra));
    }
    let local_cleanup = doc.origin == SourceOrigin::LocalFile;
    let extra = ensure_file_doc_extra(doc.extra, &path, &ext, symbol_status);
    let doc = SourceDocument { extra, ..doc };
    let prepared = doc.into_prepared(chunk_texts, "text", chunk_extra);
    Ok(if local_cleanup {
        prepared.with_local_legacy_fragment_cleanup()
    } else {
        prepared
    })
}

fn prepare_markdown_source(doc: SourceDocument) -> PreparedDoc {
    let (chunks, fallback) = safe_markdown_chunks(&doc.text);
    let line_index = LineIndex::new(&doc.text);
    let chunk_extra = chunks
        .iter()
        .scan(0usize, |cursor, chunk| {
            let (byte_start, byte_end) = locate_chunk(&doc.text, chunk, *cursor);
            *cursor = byte_end;
            let (line_start, line_end) = line_index.line_range_for_bytes(byte_start, byte_end);
            let mut extra = base_chunk_metadata(
                "markdown",
                &format!("{}#chunk-{}", doc.url, byte_start),
                line_start,
                line_end,
                byte_start,
                byte_end,
            );
            if fallback {
                extra.insert(
                    "chunking_fallback".into(),
                    "plain_text_control_chars".into(),
                );
            }
            Some(chunk_metadata(extra))
        })
        .collect();
    doc.into_prepared(chunks, "markdown", chunk_extra)
}

fn prepare_plain_source(doc: SourceDocument) -> PreparedDoc {
    let chunks_with_offsets = chunk_text_with_offsets(&doc.text);
    let line_index = LineIndex::new(&doc.text);
    let mut chunks = Vec::with_capacity(chunks_with_offsets.len());
    let mut chunk_extra = Vec::with_capacity(chunks_with_offsets.len());
    for (byte_start, chunk) in chunks_with_offsets {
        let byte_end = byte_start + chunk.len();
        let (line_start, line_end) = line_index.line_range_for_bytes(byte_start, byte_end);
        chunk_extra.push(chunk_metadata(base_chunk_metadata(
            "plain_text",
            &format!("{}#chunk-{}", doc.url, byte_start),
            line_start,
            line_end,
            byte_start,
            byte_end,
        )));
        chunks.push(chunk);
    }
    doc.into_prepared(chunks, "text", chunk_extra)
}

fn safe_markdown_chunks(text: &str) -> (Vec<String>, bool) {
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        (chunk_text(text), true)
    } else {
        (chunk_markdown(text), false)
    }
}

fn base_chunk_metadata(
    content_kind: &str,
    locator: &str,
    line_start: u32,
    line_end: u32,
    byte_start: usize,
    byte_end: usize,
) -> Map<String, Value> {
    let mut range = Map::new();
    range.insert("line_start".into(), line_start.into());
    range.insert("line_end".into(), line_end.into());
    range.insert("byte_start".into(), byte_start.into());
    range.insert("byte_end".into(), byte_end.into());

    let mut extra = Map::new();
    extra.insert("chunk_content_kind".into(), content_kind.into());
    extra.insert("chunk_locator".into(), locator.into());
    extra.insert("source_range".into(), Value::Object(range));
    extra
}

fn chunk_metadata(metadata: Map<String, Value>) -> Value {
    Value::Object(metadata)
}

fn ensure_file_doc_extra(
    extra: Option<Value>,
    path: &str,
    ext: &str,
    symbol_status: &str,
) -> Option<Value> {
    let mut map = match sanitize_doc_extra(extra) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    insert_missing_or_null(&mut map, "code_file_path", path.to_string().into());
    insert_missing_or_null(&mut map, "code_language", language_name(ext).into());
    insert_missing_or_null(&mut map, "code_file_type", classify_file_type(path).into());
    insert_missing_or_null(&mut map, "code_is_test", is_test_path(path).into());
    insert_missing_or_null(&mut map, "symbol_extraction_status", symbol_status.into());
    Some(Value::Object(map))
}

fn insert_missing_or_null(map: &mut Map<String, Value>, key: &str, value: Value) {
    if !map.contains_key(key) || map.get(key).is_some_and(Value::is_null) {
        map.insert(key.to_string(), value);
    }
}

fn sanitize_doc_extra(extra: Option<Value>) -> Option<Value> {
    match extra {
        Some(Value::Object(mut map)) => {
            for key in PLANNER_OWNED_PAYLOAD_KEYS {
                map.remove(*key);
            }
            Some(Value::Object(map))
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "source_doc_tests.rs"]
mod tests;
