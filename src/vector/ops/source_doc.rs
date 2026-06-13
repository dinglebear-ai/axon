use futures_util::{StreamExt, stream};
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn estimated_bytes(&self) -> usize {
        self.text.len()
            + self.url.len()
            + self.title.as_ref().map_or(0, String::len)
            + self.extra.as_ref().map_or(0, |v| v.to_string().len())
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
        Ok(Self {
            origin: SourceOrigin::ScrapeResult,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name,
            structured,
            chunk_hint: SourceChunkHint::MarkdownOrPlainText,
        })
    }

    pub(crate) fn try_new_crawl_manifest(
        url: String,
        text: String,
        title: Option<String>,
        structured: Option<StructuredPayload>,
    ) -> Result<Self, String> {
        let domain = domain_from_web_url(&url)?;
        Ok(Self {
            origin: SourceOrigin::CrawlManifest,
            url,
            domain,
            text,
            source_type: "crawl".to_string(),
            title,
            extra: None,
            extractor_name: None,
            structured,
            chunk_hint: SourceChunkHint::MarkdownOrPlainText,
        })
    }

    pub(crate) fn try_new_local_markdown(
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
    ) -> Result<Self, String> {
        Ok(Self {
            origin: SourceOrigin::LocalFile,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name: None,
            structured: None,
            chunk_hint: SourceChunkHint::MarkdownOrPlainText,
        })
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
        Ok(Self {
            origin,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name: None,
            structured: None,
            chunk_hint: SourceChunkHint::File { path, extension },
        })
    }

    pub(crate) fn try_new_plain_text(
        url: String,
        domain: String,
        text: String,
        source_type: impl Into<String>,
        title: Option<String>,
        extra: Option<Value>,
    ) -> Result<Self, String> {
        Ok(Self {
            origin: SourceOrigin::PlainIngest,
            url,
            domain,
            text,
            source_type: source_type.into(),
            title,
            extra: sanitize_doc_extra(extra),
            extractor_name: None,
            structured: None,
            chunk_hint: SourceChunkHint::PlainText,
        })
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

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn prepare_source_documents_bounded<I>(
    docs: I,
    concurrency: usize,
    max_inflight_bytes: usize,
) -> Result<Vec<PreparedDoc>, String>
where
    I: IntoIterator<Item = SourceDocument>,
{
    let concurrency = concurrency.clamp(1, 32);
    let max_inflight_bytes = max_inflight_bytes.max(1);
    let mut buffered = stream::iter(docs)
        .map(|doc| async move {
            if doc.estimated_bytes() > max_inflight_bytes {
                tracing::debug!(
                    url = %doc.url(),
                    estimated_bytes = doc.estimated_bytes(),
                    max_inflight_bytes,
                    "source_doc: single document exceeds planning byte budget"
                );
            }
            prepare_source_document(doc).await
        })
        .buffer_unordered(concurrency);
    let mut out = Vec::new();
    while let Some(item) = buffered.next().await {
        out.push(item?);
    }
    Ok(out)
}

pub(crate) fn prepare_plain_text_source(
    url: String,
    domain: String,
    text: String,
    source_type: impl Into<String>,
    title: Option<String>,
    extra: Option<Value>,
) -> Result<PreparedDoc, String> {
    let source = SourceDocument::try_new_plain_text(url, domain, text, source_type, title, extra)?;
    Ok(prepare_plain_source(source))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn should_flush_prepared_batch(
    docs: &[PreparedDoc],
    next_estimated_bytes: usize,
    max_docs: usize,
    max_bytes: usize,
) -> bool {
    if docs.is_empty() {
        return false;
    }
    if docs.len() >= max_docs.max(1) {
        return true;
    }
    let current_bytes: usize = docs
        .iter()
        .map(|doc| {
            doc.chunks.iter().map(String::len).sum::<usize>()
                + doc.url.len()
                + doc.extra.as_ref().map_or(0, |v| v.to_string().len())
        })
        .sum();
    current_bytes.saturating_add(next_estimated_bytes) >= max_bytes.max(1)
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
        schema_type: schema_type.or_else(|| Some(format!("{extractor_name}_structured"))),
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
        let mut extra = base_chunk_metadata(
            "code",
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
        chunk_extra.push(Value::Object(extra));
    }
    let extra = ensure_file_doc_extra(doc.extra, &path, &ext, symbol_status);
    Ok(PreparedDoc::from_planned_chunks(
        doc.url,
        doc.domain,
        chunk_texts,
        doc.source_type,
        "text",
        doc.title,
        extra,
        doc.extractor_name,
        doc.structured,
        chunk_extra,
    ))
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
            Some(Value::Object(extra))
        })
        .collect();
    PreparedDoc::from_planned_chunks(
        doc.url,
        doc.domain,
        chunks,
        doc.source_type,
        "markdown",
        doc.title,
        doc.extra,
        doc.extractor_name,
        doc.structured,
        chunk_extra,
    )
}

fn prepare_plain_source(doc: SourceDocument) -> PreparedDoc {
    let chunks_with_offsets = chunk_text_with_offsets(&doc.text);
    let line_index = LineIndex::new(&doc.text);
    let mut chunks = Vec::with_capacity(chunks_with_offsets.len());
    let mut chunk_extra = Vec::with_capacity(chunks_with_offsets.len());
    for (byte_start, chunk) in chunks_with_offsets {
        let byte_end = byte_start + chunk.len();
        let (line_start, line_end) = line_index.line_range_for_bytes(byte_start, byte_end);
        chunk_extra.push(Value::Object(base_chunk_metadata(
            "plain_text",
            &format!("{}#chunk-{}", doc.url, byte_start),
            line_start,
            line_end,
            byte_start,
            byte_end,
        )));
        chunks.push(chunk);
    }
    PreparedDoc::from_planned_chunks(
        doc.url,
        doc.domain,
        chunks,
        doc.source_type,
        "text",
        doc.title,
        doc.extra,
        doc.extractor_name,
        doc.structured,
        chunk_extra,
    )
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
    extra.insert("content_kind".into(), content_kind.into());
    extra.insert("chunk_locator".into(), locator.into());
    extra.insert("source_range".into(), Value::Object(range));
    extra
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
    map.entry("code_file_path")
        .or_insert_with(|| path.to_string().into());
    map.entry("code_language")
        .or_insert_with(|| language_name(ext).into());
    map.entry("code_file_type")
        .or_insert_with(|| classify_file_type(path).into());
    map.entry("code_is_test")
        .or_insert_with(|| is_test_path(path).into());
    map.entry("symbol_extraction_status")
        .or_insert_with(|| symbol_status.into());
    Some(Value::Object(map))
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
