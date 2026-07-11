//! Profile-dispatched chunk building, including the structured-parse and
//! size/adapter-fallback paths. Split out of `preparer.rs` to keep that file
//! under the repo's 500-line monolith cap.

use axon_api::source::{Severity, SourceItemKey, SourceWarning};

use crate::chunk::DocumentChunk;
use crate::profile::ChunkingProfile;
use crate::{code, markdown, metadata, schema, session, text, transcript};

pub(super) struct ChunkBuild {
    pub(super) chunks: Vec<DocumentChunk>,
    pub(super) warnings: Vec<SourceWarning>,
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
    use_fallback: bool,
) -> ChunkBuild {
    let chunks = match profile {
        ChunkingProfile::CodeSymbol if use_fallback => size_fallback_chunks(text, "code_blocks"),
        ChunkingProfile::CodeSymbol => code::code_symbols(text, path, language_hint),
        ChunkingProfile::CodeManifest => code::code_manifest(text, path),
        ChunkingProfile::MarkdownSections if use_fallback => {
            size_fallback_chunks(text, "plain_text_windows")
        }
        ChunkingProfile::MarkdownSections => markdown::markdown_sections(text),
        ChunkingProfile::HtmlArticle if use_fallback => {
            size_fallback_chunks(text, "plain_text_windows")
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
        ChunkingProfile::ToolOutput => transcript::split_on_nonempty_lines(text, "tool_output"),
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
fn size_fallback_chunks(text: &str, fallback_method: &'static str) -> Vec<DocumentChunk> {
    text::plain_text_windows(text)
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
