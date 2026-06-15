// AST-aware code chunking via tree-sitter.

use crate::core::logging::log_warn;

use super::chunk_text_with_offsets;

mod assembly;
pub mod chunk;
mod extract;
mod postprocess;

use assembly::assemble;
pub use chunk::{ChunkSource, CodeChunk, Symbol, SymbolKind};
use extract::{Extractor, extract_symbols, language_for_extension};
use postprocess::{
    attach_leading_comments, dedupe_exact_ranges, inject_declaration_headers,
    merge_tiny_declarations,
};

const MAX_CODE_CHUNK_BYTES_DEFAULT: usize = 2 * 1024 * 1024;
/// A supported file larger than this (bytes) but yielding zero symbols is logged
/// as suspected grammar drift rather than silently degrading to prose.
const GRAMMAR_DRIFT_WARN_MIN_BYTES: usize = 200;

/// Split source code into AST-aware chunks.
///
/// Returns `None` for unsupported file extensions. Empty chunks are filtered
/// from the output. Chunk sizes target 500–2000 characters with 200-char
/// overlap between adjacent chunks, splitting at structural boundaries
/// (functions, blocks, statements) when possible. The overlap ensures that
/// a function signature split across chunk boundaries appears in both chunks,
/// matching the 200-char overlap used by `chunk_text()`.
#[must_use]
pub fn chunk_code(content: &str, file_extension: &str) -> Option<Vec<String>> {
    chunk_code_chunks(content, file_extension)
        .map(|chunks| chunks.into_iter().map(|chunk| chunk.text).collect())
}

#[must_use]
/// Split source code into AST-aware [`CodeChunk`]s carrying per-chunk symbol
/// metadata (name, kind, declaration line range).
///
/// Returns `None` for unsupported extensions (callers fall back to prose
/// chunking) and `Some(vec![])` for a supported file that splits into no
/// non-empty chunks. After the tree-sitter split, the chunks pass through the
/// post-processing pipeline: leading comments are attached, exact-range
/// duplicates dropped, tiny adjacent declarations merged, and declaration
/// headers injected for split bodies. `chunk_code` is a thin text-only wrapper
/// over this function.
pub fn chunk_code_chunks(content: &str, file_extension: &str) -> Option<Vec<CodeChunk>> {
    let spec = language_for_extension(file_extension)?;

    // Empty / whitespace-only content → no chunks (callers treat as nothing to
    // index). A genuinely empty supported file is not a fallback case.
    if content.trim().is_empty() {
        return Some(Vec::new());
    }

    // Files over the tree-sitter byte ceiling skip parsing entirely and degrade
    // to whole-file prose chunking rather than producing zero chunks.
    if content.len() > max_tree_sitter_file_bytes() {
        return Some(prose_fallback(content));
    }

    // Single parse: extract_symbols parses internally; we never parse again.
    let symbols = extract_symbols(content, spec.extractor);

    if symbols.is_empty() {
        // A non-trivial supported file with zero symbols is suspected grammar
        // drift (node-kind names changed upstream) — surface it before degrading
        // the whole file to prose.
        if content.len() > GRAMMAR_DRIFT_WARN_MIN_BYTES {
            log_warn(&format!(
                "command=chunk_code grammar_drift_zero_symbols ext={file_extension} bytes={}",
                content.len()
            ));
        }
        return Some(prose_fallback(content));
    }

    let out = assemble(content, &symbols);
    if out.is_empty() {
        // Assembly dropped everything (e.g. all residual slivers, no leaves
        // emitted) on non-empty content → never emit zero chunks.
        return Some(prose_fallback(content));
    }

    // Postprocess pipeline. Kept on declaration-driven input:
    //  - attach_leading_comments: prepend a declaration's doc-comment block.
    //  - dedupe_exact_ranges: drop identical (decl-range, line-range, kind) chunks.
    //  - merge_tiny_declarations: coalesce adjacent tiny const/static/type leaves.
    //  - inject_declaration_headers: re-stamp a header on continuation chunks of
    //    an oversized declaration that was split across several chunks.
    // (Bead .5 originally planned to retire merge_tiny_declarations /
    // inject_declaration_headers as obsolete under declaration-driven assembly.
    // That plan is superseded: both still do useful work and are tested.
    // merge_tiny_declarations coalesces adjacent tiny const/static/type leaf
    // chunks that assembly still emits separately (postprocess_tests::
    // tiny_consts_merge_and_clear_symbol_name). inject_declaration_headers
    // re-stamps a declaration header onto the continuation sub-chunks of an
    // oversized declaration that assembly split across several chunks
    // (code_tests::oversized_rust_function_continuations_include_header). They
    // are kept deliberately, not by inertia.)
    let out = attach_leading_comments(out, content, file_extension);
    let out = dedupe_exact_ranges(out);
    let out = merge_tiny_declarations(out);
    let out = inject_declaration_headers(out);
    Some(out)
}

/// Whole-file prose chunking, mirroring `file_ingest`'s prose branch: synthetic
/// `CodeChunk`s with no symbol, `ChunkSource::Prose`, and correct line ranges.
/// Used for the zero-declaration / oversized-file fallbacks so a non-empty
/// supported file is never indexed as zero chunks.
fn prose_fallback(content: &str) -> Vec<CodeChunk> {
    let lines = assembly::LineIndex::new(content);
    chunk_text_with_offsets(content)
        .into_iter()
        .map(|(byte_start, text)| {
            let byte_end = byte_start + text.len();
            let (start_line, end_line) = lines.line_range_for_bytes(byte_start, byte_end);
            CodeChunk {
                text,
                byte_start,
                byte_end,
                start_line,
                end_line,
                declaration_start_line: start_line,
                declaration_end_line: end_line,
                symbol: None,
                source: ChunkSource::Prose,
            }
        })
        .filter(|chunk| !chunk.text.trim().is_empty())
        .collect()
}

#[must_use]
pub fn supports_tree_sitter_chunking(file_extension: &str) -> bool {
    language_for_extension(file_extension).is_some()
}

/// Classify how symbol extraction went for a file, for the
/// `symbol_extraction_status` payload field and ingest logging. Returns one of a
/// closed set of values that callers branch on:
/// - `"prose"`         — extension has no tree-sitter grammar
/// - `"unsupported"`   — grammar exists but no symbol extractor for the language
/// - `"skipped_large"` — file exceeded `AXON_MAX_TREE_SITTER_FILE_BYTES`
/// - `"ok"`            — at least one chunk carries a symbol
/// - `"none_found"`    — extractor ran but found no symbols
#[must_use]
pub fn code_symbol_extraction_status(
    content: &str,
    file_extension: &str,
    chunks: &[CodeChunk],
) -> &'static str {
    let Some(spec) = language_for_extension(file_extension) else {
        return "prose";
    };
    if spec.extractor == Extractor::None {
        return "unsupported";
    }
    if content.len() > max_tree_sitter_file_bytes() {
        return "skipped_large";
    }
    if chunks.iter().any(|chunk| chunk.symbol.is_some()) {
        "ok"
    } else {
        "none_found"
    }
}

fn max_tree_sitter_file_bytes() -> usize {
    std::env::var("AXON_MAX_TREE_SITTER_FILE_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(MAX_CODE_CHUNK_BYTES_DEFAULT)
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "code/parity_matrix_tests.rs"]
mod parity_matrix_tests;
