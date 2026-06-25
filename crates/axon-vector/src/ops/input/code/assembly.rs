//! Declaration-driven chunk assembly.
//!
//! Replaces the old size-window splitter. The sorted symbol list from
//! [`extract_symbols`](super::extract::extract_symbols) is the source of truth:
//! each top-level **leaf** declaration becomes one chunk (split when oversized),
//! **containers** (impl/class/trait/mod) emit at most a small header chunk, and
//! the spans *between* declarations are run through the residual sweep so pure
//! structural punctuation (`}`, `;`, `() =>`) is dropped instead of indexed as a
//! sliver. A single advancing cursor walks the file once; no second parse.

mod oversized;
mod residual;

use super::chunk::{ChunkSource, CodeChunk, Symbol};
use super::extract::{DeclRole, SymbolInfo};
use oversized::{MAX_CODE_CHUNK_CHARS, emit_leaf};
use residual::{residual_gap_floor_chars, sweep_residual};

/// Newline-offset index over a file, giving O(log n) byte→line lookups. Local
/// equivalent of `source_doc::support::LineIndex` (which is `pub(super)` to that
/// module and not importable here).
pub(super) struct LineIndex {
    text_len: usize,
    newline_offsets: Vec<usize>,
}

impl LineIndex {
    pub(super) fn new(text: &str) -> Self {
        Self {
            text_len: text.len(),
            newline_offsets: text
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index))
                .collect(),
        }
    }

    /// 1-based inclusive `(start_line, end_line)` for the byte range. `byte_end`
    /// is treated as exclusive; an empty range collapses to a single line.
    pub(super) fn line_range_for_bytes(&self, byte_start: usize, byte_end: usize) -> (u32, u32) {
        let start = self.line_for_byte(byte_start);
        let last = byte_end.saturating_sub(1).max(byte_start);
        let end = self.line_for_byte(last);
        (start, end.max(start))
    }

    fn line_for_byte(&self, byte: usize) -> u32 {
        let capped = byte.min(self.text_len);
        self.newline_offsets.partition_point(|off| *off < capped) as u32 + 1
    }
}

/// Assemble declaration-driven chunks from the sorted symbol list.
///
/// Walks `symbols` (already sorted by `(byte_start, byte_end)`) with a single
/// advancing `cursor`. Returns an empty `Vec` when no chunks are produced — the
/// caller is responsible for the zero-declaration prose fallback.
pub(super) fn assemble(content: &str, symbols: &[SymbolInfo]) -> Vec<CodeChunk> {
    let lines = LineIndex::new(content);
    let floor = residual_gap_floor_chars();
    let mut out: Vec<CodeChunk> = Vec::with_capacity(symbols.len() + 1);

    let mut cursor = 0usize;
    // End byte of the most recent *leaf* declaration. Any later symbol that
    // starts before this is nested inside that leaf's body (a closure / nested
    // fn) and folds into the parent — it is skipped. Containers never raise this
    // (their methods are separate leaves and must still emit).
    let mut last_leaf_end = 0usize;

    for sym in symbols {
        // Nested inside an already-emitted leaf body → fold into parent.
        if sym.byte_start < last_leaf_end {
            continue;
        }
        // Residual run between the cursor and this declaration.
        if sym.byte_start > cursor {
            sweep_residual(content, cursor, sym.byte_start, floor, &lines, &mut out);
        }

        match sym.role {
            DeclRole::Container => {
                let header_end = emit_container_header(content, sym, &lines, &mut out);
                // Advance only past the HEADER, not the whole body. Body content
                // not captured as a child leaf (struct fields, enum variants,
                // class fields, doc comments) is then recovered by the residual
                // sweep instead of silently dropped. Child method leaves (later
                // in the sorted list, starting after header_end) still emit, and
                // last_leaf_end is deliberately NOT raised so they are not folded.
                cursor = cursor.max(header_end);
            }
            DeclRole::Leaf => {
                emit_leaf(
                    content,
                    sym.byte_start,
                    sym.byte_end,
                    symbol_of(sym),
                    sym.start_line,
                    sym.end_line,
                    &lines,
                    &mut out,
                );
                last_leaf_end = sym.byte_end;
                cursor = cursor.max(sym.byte_end);
            }
        }
    }

    // Trailing residual run after the last declaration.
    if cursor < content.len() {
        sweep_residual(content, cursor, content.len(), floor, &lines, &mut out);
    }

    out
}

/// Emit a container's header chunk (its signature line(s) through the opening
/// brace, carrying the container symbol) and return the **byte offset where the
/// header ends**. The caller advances the cursor only to this point — NOT past
/// the whole body — so the residual sweep recovers body content that is not
/// captured as a child leaf (struct fields, enum variants, class fields, and
/// their doc comments). Child method leaves still emit separately. The header
/// chunk is emitted only when a non-empty, non-trivial header exists.
fn emit_container_header(
    content: &str,
    sym: &SymbolInfo,
    lines: &LineIndex,
    out: &mut Vec<CodeChunk>,
) -> usize {
    let Some(full) = content.get(sym.byte_start..sym.byte_end) else {
        return sym.byte_end;
    };
    let header_end_rel = full
        .find('{')
        .map_or_else(|| full.find('\n').unwrap_or(full.len()), |brace| brace + 1);
    let header_end = (sym.byte_start + header_end_rel).min(content.len());
    if let Some(header) = content.get(sym.byte_start..header_end) {
        let trimmed = header.trim();
        if !trimmed.is_empty() {
            let text = take_chars(trimmed, MAX_CODE_CHUNK_CHARS);
            let (start_line, end_line) = lines.line_range_for_bytes(sym.byte_start, header_end);
            out.push(CodeChunk {
                text,
                byte_start: sym.byte_start,
                byte_end: header_end,
                start_line,
                end_line,
                declaration_start_line: sym.start_line,
                declaration_end_line: sym.end_line,
                symbol: symbol_of(sym),
                source: ChunkSource::TreeSitter,
            });
        }
    }
    header_end
}

fn symbol_of(sym: &SymbolInfo) -> Option<Symbol> {
    Some(Symbol {
        kind: sym.kind,
        name: sym.name.clone(),
    })
}

fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
#[path = "assembly_tests.rs"]
mod tests;
