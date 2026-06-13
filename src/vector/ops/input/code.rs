// AST-aware code chunking via tree-sitter.

use super::CHUNK_OVERLAP;
use text_splitter::{ChunkConfig, CodeSplitter};

pub mod chunk;
mod extract;
mod postprocess;

pub use chunk::{ChunkSource, CodeChunk, Symbol, SymbolKind};
use extract::{Extractor, extract_symbols, find_symbol_for_chunk, language_for_extension};
use postprocess::{
    attach_leading_comments, dedupe_exact_ranges, inject_declaration_headers,
    merge_tiny_declarations,
};

const MAX_CODE_CHUNK_BYTES_DEFAULT: usize = 2 * 1024 * 1024;

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
    let chunks = split_code_indices(content, spec.grammar)?;
    if chunks.is_empty() {
        return Some(Vec::new());
    }

    let symbols = if content.len() <= max_tree_sitter_file_bytes() {
        extract_symbols(content, spec.extractor)
    } else {
        Vec::new()
    };

    let mut out = Vec::with_capacity(chunks.len());
    for (byte_start, chunk) in chunks {
        let byte_end = byte_start + chunk.len();
        let start_line = line_for_byte(content, byte_start);
        // Inclusive end: a chunk that ends on a newline must not be attributed to
        // the following line, so derive from the last byte rather than the
        // exclusive end.
        let end_line = if byte_end > byte_start {
            line_for_byte(content, byte_end - 1)
        } else {
            line_for_byte(content, byte_end)
        };
        let symbol = find_symbol_for_chunk(&symbols, byte_start, byte_end);
        out.push(CodeChunk {
            text: chunk.to_string(),
            byte_start,
            byte_end,
            start_line,
            end_line,
            declaration_start_line: symbol.map_or(start_line, |sym| sym.start_line),
            declaration_end_line: symbol.map_or(end_line, |sym| sym.end_line),
            symbol: symbol.map(|sym| Symbol {
                kind: sym.kind,
                name: sym.name.clone(),
            }),
            source: ChunkSource::TreeSitter,
        });
    }

    let out = attach_leading_comments(out, content, file_extension);
    let out = dedupe_exact_ranges(out);
    let out = merge_tiny_declarations(out);
    let out = inject_declaration_headers(out);
    Some(out)
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

fn split_code_indices(
    content: &str,
    lang: tree_sitter_language::LanguageFn,
) -> Option<Vec<(usize, &str)>> {
    let config = ChunkConfig::new(500..2000)
        .with_overlap(CHUNK_OVERLAP)
        .expect("CHUNK_OVERLAP < max chunk size");
    // Use `.ok()?` rather than `.expect`: a tree-sitter ABI mismatch would
    // panic on every file instead of gracefully falling back to prose chunking.
    let splitter = CodeSplitter::new(lang, config).ok()?;

    let mut chunks: Vec<(usize, &str)> = Vec::new();
    for (offset, chunk) in splitter
        .chunk_indices(content)
        .filter(|(_, chunk)| !chunk.trim().is_empty())
    {
        push_bounded_chunks(content, offset, chunk, &mut chunks);
    }

    Some(chunks)
}

fn push_bounded_chunks<'a>(
    content: &'a str,
    offset: usize,
    chunk: &'a str,
    out: &mut Vec<(usize, &'a str)>,
) {
    if chunk.len() <= 2000 {
        out.push((offset, chunk));
        return;
    }

    let mut local_start = 0usize;
    while local_start < chunk.len() {
        let mut local_end = (local_start + 2000).min(chunk.len());
        while local_end > local_start && !chunk.is_char_boundary(local_end) {
            local_end -= 1;
        }
        if local_end == local_start {
            break;
        }
        let abs_start = offset + local_start;
        let abs_end = offset + local_end;
        out.push((abs_start, &content[abs_start..abs_end]));
        if local_end == chunk.len() {
            break;
        }
        local_start = local_end.saturating_sub(CHUNK_OVERLAP);
        while local_start > 0 && !chunk.is_char_boundary(local_start) {
            local_start -= 1;
        }
    }
}

fn max_tree_sitter_file_bytes() -> usize {
    std::env::var("AXON_MAX_TREE_SITTER_FILE_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(MAX_CODE_CHUNK_BYTES_DEFAULT)
}

fn line_for_byte(content: &str, byte: usize) -> u32 {
    // Snap to a char boundary: callers may pass an inclusive end that lands
    // inside a multibyte character, and slicing on a non-boundary panics.
    let mut capped = byte.min(content.len());
    while capped > 0 && !content.is_char_boundary(capped) {
        capped -= 1;
    }
    content[..capped].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
