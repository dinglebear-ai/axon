//! Profile-dispatched chunk building, including the structured-parse and
//! size/adapter-fallback paths. Split out of `preparer.rs` to keep that file
//! under the repo's 500-line monolith cap.

use axon_api::source::{Severity, SourceItemKey, SourceParseFacts, SourceWarning};

use crate::chunk::DocumentChunk;
use crate::markdown::MarkdownChunkLimits;
use crate::profile::ChunkingProfile;
use crate::{code, markdown, metadata, schema, session, text, transcript};

pub(super) struct ChunkBuild {
    pub(super) chunks: Vec<DocumentChunk>,
    pub(super) warnings: Vec<SourceWarning>,
}

pub(super) struct BoundedChunks {
    pub(super) chunks: Vec<DocumentChunk>,
    pub(super) size_backstop: bool,
    pub(super) empty_fallback: bool,
}

pub(super) fn bound_or_fallback(chunks: Vec<DocumentChunk>, source: &str) -> BoundedChunks {
    let (mut chunks, size_backstop) = bound_embedding_chunks(chunks, source);
    let empty_fallback = chunks.is_empty();
    if empty_fallback {
        // Structural chunkers can find no records in otherwise useful text.
        chunks = text::plain_text_windows(source)
            .into_iter()
            .filter(|chunk| !chunk.content.trim().is_empty())
            .map(|chunk| {
                chunk
                    .with_metadata("chunking_fallback", "empty_structural_result".into())
                    .with_metadata("actual_chunking_method", "plain_text_windows".into())
            })
            .collect();
    }
    BoundedChunks {
        chunks,
        size_backstop,
        empty_fallback,
    }
}

pub(super) fn empty_fallback_warning(source_item_key: &SourceItemKey) -> SourceWarning {
    warning(
        "chunk.empty_fallback",
        "structural chunker produced no chunks; indexed bounded source text instead",
        source_item_key,
    )
}

pub(super) fn parsed_code_method(
    profile: ChunkingProfile,
    use_fallback: bool,
    chunks: &[DocumentChunk],
) -> Option<String> {
    (profile == ChunkingProfile::CodeSymbol && !use_fallback)
        .then(|| {
            chunks.first().and_then(|chunk| {
                chunk
                    .metadata
                    .get("actual_chunking_method")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
        })
        .flatten()
}

/// Keep every prepared text fragment below the embedding provider's per-input
/// limit, including records produced by structural parsers and atomic profiles.
/// Source-backed fragments retain exact positions; synthetic JSON records keep
/// their JSON pointer because they have no literal source byte span.
pub(super) fn bound_embedding_chunks(
    chunks: Vec<DocumentChunk>,
    source: &str,
) -> (Vec<DocumentChunk>, bool) {
    let mut bounded = Vec::new();
    let mut split_any = false;
    for chunk in chunks {
        if chunk.content.len() <= text::MAX_PLAIN_TEXT_CHUNK_BYTES
            && chunk.content.chars().count() <= text::MAX_PLAIN_TEXT_CHUNK_CHARS
        {
            bounded.push(chunk);
            continue;
        }
        split_any = true;
        split_chunk_streaming(&chunk, source, &mut bounded);
    }
    (bounded, split_any)
}

fn split_chunk_streaming(chunk: &DocumentChunk, source: &str, output: &mut Vec<DocumentChunk>) {
    let literal_start = chunk
        .range
        .byte_start
        .map(|start| start as usize)
        .filter(|&start| {
            chunk.range.char_start.is_some()
                && chunk.range.line_start.is_some()
                && source.get(start..start.saturating_add(chunk.content.len()))
                    == Some(chunk.content.as_str())
        });
    let mut start_byte = 0;
    let mut start_char = 0_u64;
    let mut start_line = 1_u32;
    let mut line = 1_u32;
    let mut last_line = 1_u32;
    let mut char_index = 0_u64;
    for (byte, ch) in chunk.content.char_indices() {
        if byte > start_byte
            && (byte + ch.len_utf8() - start_byte > text::MAX_PLAIN_TEXT_CHUNK_BYTES
                || char_index - start_char >= text::MAX_PLAIN_TEXT_CHUNK_CHARS as u64)
        {
            push_bounded_window(
                chunk,
                literal_start,
                start_byte,
                byte,
                start_char,
                char_index,
                start_line,
                last_line,
                output,
            );
            start_byte = byte;
            start_char = char_index;
            start_line = line;
        }
        last_line = line;
        if ch == '\n' {
            line = line.saturating_add(1);
        }
        char_index += 1;
    }
    push_bounded_window(
        chunk,
        literal_start,
        start_byte,
        chunk.content.len(),
        start_char,
        char_index,
        start_line,
        last_line,
        output,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_bounded_window(
    chunk: &DocumentChunk,
    literal_start: Option<usize>,
    start_byte: usize,
    end_byte: usize,
    start_char: u64,
    end_char: u64,
    start_line: u32,
    end_line: u32,
    output: &mut Vec<DocumentChunk>,
) {
    if start_byte == end_byte || chunk.content[start_byte..end_byte].trim().is_empty() {
        return;
    }
    let mut part = DocumentChunk::new(&chunk.content[start_byte..end_byte], chunk.range.clone());
    part.title = chunk.title.clone();
    part.heading_path = chunk.heading_path.clone();
    part.symbol = chunk.symbol.clone();
    part.metadata = chunk.metadata.clone();
    part.metadata
        .insert("chunking_fallback".into(), "embedding_size_backstop".into());
    part.metadata
        .insert("actual_chunking_method".into(), "plain_text_windows".into());
    if let Some(base) = literal_start {
        part.range.byte_start = Some((base + start_byte) as u64);
        part.range.byte_end = Some((base + end_byte) as u64);
        part.range.char_start = chunk.range.char_start.map(|n| n + start_char);
        part.range.char_end = chunk.range.char_start.map(|n| n + end_char);
        part.range.line_start = chunk.range.line_start.map(|n| n + start_line - 1);
        part.range.line_end = chunk.range.line_start.map(|n| n + end_line - 1);
    }
    output.push(part);
}

/// Profiles whose primary chunker is a structural parser (tree-sitter,
/// markdown heading walker) with a generic windowed-text fallback in its
/// chain. When the router decided a size/adapter fallback applies
/// (`use_fallback`), these dispatch to `text::plain_text_windows` instead of
/// the structural chunker, tagged with the fallback method name, so
/// `chunking_method` never reports a method that did not actually run.
/// Profiles left out (transcript/tool-output/session/structured/atomic)
/// already dispatch to a single implementation regardless of size, or handle
/// their own parse-failure fallback via `structured_or_fallback`.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_chunks(
    profile: ChunkingProfile,
    text: &str,
    structured_payload: Option<&serde_json::Value>,
    source_item_key: &SourceItemKey,
    path: Option<&str>,
    language_hint: Option<&str>,
    content_kind: axon_api::source::ContentKind,
    parse_facts: &[SourceParseFacts],
    use_fallback: bool,
    markdown_limits: MarkdownChunkLimits,
) -> ChunkBuild {
    let chunks = match profile {
        ChunkingProfile::CodeSymbol if use_fallback => {
            size_fallback_chunks(text, "code_blocks", None)
        }
        ChunkingProfile::CodeSymbol => {
            code::code_symbols_with_facts(text, path, language_hint, parse_facts)
        }
        ChunkingProfile::CodeManifest => code::code_manifest(text, path),
        ChunkingProfile::MarkdownSections if use_fallback => {
            size_fallback_chunks(text, "plain_text_windows", Some(markdown_limits))
        }
        ChunkingProfile::MarkdownSections => {
            markdown::markdown_sections_with_limits(text, markdown_limits)
        }
        ChunkingProfile::HtmlArticle => markdown::html_article(text),
        ChunkingProfile::PlainTextWindows => text::plain_text_windows(text),
        ChunkingProfile::TranscriptSegments => transcript::transcript_segments(text),
        ChunkingProfile::StructuredRecords => {
            return structured_or_fallback(
                profile,
                metadata::structured_records(text, structured_payload, content_kind, path),
                text,
                source_item_key,
            );
        }
        ChunkingProfile::ApiSchema => {
            return structured_or_fallback(
                profile,
                schema::api_schema(text, structured_payload, content_kind, path),
                text,
                source_item_key,
            );
        }
        ChunkingProfile::ToolOutput => transcript::tool_output_records(text),
        ChunkingProfile::SessionTurns => session::session_turns(text),
        ChunkingProfile::AtomicMetadata => metadata::atomic_metadata(text),
    };
    ChunkBuild {
        chunks,
        warnings: Vec::new(),
    }
}

/// Generic windowed-text split used as the actual implementation behind a
/// size/adapter-triggered structural-parser fallback. Tags each chunk with
/// the same `chunking_fallback`/`actual_chunking_method` fields
/// `code::split_if_huge` uses for its own huge-symbol fallback, so the
/// dispatch decision is inspectable on the resulting chunks, not just in the
/// document-level `chunking_method` field, and stays within the vector
/// payload's known-field allowlist.
/// When the routed profile carries injected limits (Markdown), the fallback
/// windows honor them instead of the hardcoded plain-text caps; the byte cap
/// is derived from the char cap at the UTF-8 worst case of 4 bytes per char.
fn size_fallback_chunks(
    text: &str,
    fallback_method: &'static str,
    limits: Option<MarkdownChunkLimits>,
) -> Vec<DocumentChunk> {
    let chunks = match limits {
        Some(limits) => text::plain_text_windows_with_limits(
            text,
            limits.max_chars().saturating_mul(4),
            limits.max_chars(),
        ),
        None => text::plain_text_windows(text),
    };
    chunks
        .into_iter()
        .map(|chunk| {
            chunk
                .with_metadata("chunking_fallback", "size_or_adapter".into())
                .with_metadata("actual_chunking_method", fallback_method.into())
        })
        .collect()
}

fn structured_or_fallback(
    profile: ChunkingProfile,
    result: Result<Vec<DocumentChunk>, String>,
    text: &str,
    source_item_key: &SourceItemKey,
) -> ChunkBuild {
    match result {
        Ok(chunks) => ChunkBuild {
            chunks,
            warnings: Vec::new(),
        },
        Err(error) => ChunkBuild {
            chunks: metadata::atomic_metadata(text)
                .into_iter()
                .map(|chunk| {
                    chunk
                        .with_metadata("chunking_fallback", "atomic_text".into())
                        .with_metadata("chunking_fallback_from", profile.as_str().into())
                        .with_metadata("structured_parse_error", error.clone().into())
                })
                .collect(),
            warnings: vec![warning(
                "chunk.structured_parse_failed",
                format!(
                    "structured chunk parse failed for {}: {error}",
                    profile.as_str()
                ),
                source_item_key,
            )],
        },
    }
}

pub(super) fn warning(
    code: impl Into<String>,
    message: impl Into<String>,
    source_item_key: &SourceItemKey,
) -> SourceWarning {
    SourceWarning {
        code: code.into(),
        severity: Severity::Warning,
        message: message.into(),
        source_item_key: Some(source_item_key.clone()),
        retryable: false,
    }
}
