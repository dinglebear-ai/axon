//! Splitting an oversized leaf declaration into bounded sub-chunks.
//!
//! A single declaration whose text exceeds [`MAX_CODE_CHUNK_CHARS`] is split at
//! line boundaries into sub-chunks of at most ~`MAX_CODE_CHUNK_CHARS` chars,
//! each carrying the parent symbol and the *whole* declaration's line range
//! (so a search hit still points at the full declaration). Adjacent sub-chunks
//! share [`CHUNK_OVERLAP`] chars. A pathological single line longer than the cap
//! (minified/generated code) falls back to multibyte-safe byte slicing.

use super::super::super::CHUNK_OVERLAP;
use super::super::chunk::{ChunkSource, CodeChunk, Symbol};
use super::LineIndex;

/// Hard upper bound on a single emitted chunk's char count.
pub(super) const MAX_CODE_CHUNK_CHARS: usize = 2000;

/// Emit `[byte_start, byte_end)` as one or more chunks. When the slice is within
/// the cap it becomes a single chunk; otherwise it is split at line boundaries
/// (with char-slice fallback for over-long lines). All sub-chunks inherit
/// `symbol` and the declaration's full line range.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_leaf(
    content: &str,
    byte_start: usize,
    byte_end: usize,
    symbol: Option<Symbol>,
    decl_start_line: u32,
    decl_end_line: u32,
    lines: &LineIndex,
    out: &mut Vec<CodeChunk>,
) {
    let Some(slice) = content.get(byte_start..byte_end) else {
        return;
    };
    if slice.trim().is_empty() {
        return;
    }
    if slice.chars().count() <= MAX_CODE_CHUNK_CHARS {
        push_chunk(
            content,
            byte_start,
            byte_end,
            symbol,
            decl_start_line,
            decl_end_line,
            lines,
            out,
        );
        return;
    }

    for (sub_start, sub_end) in split_offsets(slice, byte_start) {
        push_chunk(
            content,
            sub_start,
            sub_end,
            symbol.clone(),
            decl_start_line,
            decl_end_line,
            lines,
            out,
        );
    }
}

/// Compute absolute `[start, end)` byte ranges for the sub-chunks of an
/// oversized slice, splitting at line boundaries with a char-boundary-safe
/// fallback when a single line exceeds the cap. Consecutive ranges overlap by
/// roughly [`CHUNK_OVERLAP`] chars.
fn split_offsets(slice: &str, base: usize) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut cur_start = 0usize; // relative to `slice`
    let mut cur_end = 0usize;
    let mut cur_chars = 0usize;

    let flush = |ranges: &mut Vec<(usize, usize)>, start: usize, end: usize| {
        if end > start {
            ranges.push((base + start, base + end));
        }
    };

    for line in line_spans(slice) {
        let line_chars = slice[line.0..line.1].chars().count();
        // A single line longer than the cap: flush what we have, then byte-slice
        // the long line on char boundaries.
        if line_chars > MAX_CODE_CHUNK_CHARS {
            flush(&mut ranges, cur_start, cur_end);
            split_long_line(slice, line.0, line.1, base, &mut ranges);
            cur_start = line.1;
            cur_end = line.1;
            cur_chars = 0;
            continue;
        }
        if cur_chars + line_chars > MAX_CODE_CHUNK_CHARS && cur_end > cur_start {
            flush(&mut ranges, cur_start, cur_end);
            // Start the next window with overlap back into the previous one — but
            // only when the overlap still leaves room for this line under the cap.
            // Otherwise drop the overlap and start fresh at the line, so a flushed
            // window plus the incoming line can never breach MAX_CODE_CHUNK_CHARS.
            let overlapped_start = overlap_start(slice, cur_end);
            let overlap_chars = slice[overlapped_start..cur_end].chars().count();
            if overlap_chars + line_chars > MAX_CODE_CHUNK_CHARS {
                cur_start = line.0;
                cur_chars = 0;
            } else {
                cur_start = overlapped_start;
                cur_chars = overlap_chars;
            }
        }
        cur_end = line.1;
        cur_chars += line_chars;
    }
    flush(&mut ranges, cur_start, cur_end);
    ranges
}

/// Yield `(start, end)` byte spans of each line including its trailing newline.
fn line_spans(slice: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (idx, byte) in slice.bytes().enumerate() {
        if byte == b'\n' {
            spans.push((start, idx + 1));
            start = idx + 1;
        }
    }
    if start < slice.len() {
        spans.push((start, slice.len()));
    }
    spans
}

/// Char-boundary-safe slicing of one over-long line into cap-sized pieces.
fn split_long_line(
    slice: &str,
    line_start: usize,
    line_end: usize,
    base: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    let mut local = line_start;
    while local < line_end {
        let mut end = (local + MAX_CODE_CHUNK_CHARS).min(line_end);
        while end > local && !slice.is_char_boundary(end) {
            end -= 1;
        }
        if end == local {
            break;
        }
        ranges.push((base + local, base + end));
        local = end;
    }
}

/// Walk back ~[`CHUNK_OVERLAP`] chars from `end`, snapping to a char boundary.
fn overlap_start(slice: &str, end: usize) -> usize {
    let mut start = end.saturating_sub(CHUNK_OVERLAP);
    while start > 0 && !slice.is_char_boundary(start) {
        start -= 1;
    }
    start
}

#[allow(clippy::too_many_arguments)]
fn push_chunk(
    content: &str,
    byte_start: usize,
    byte_end: usize,
    symbol: Option<Symbol>,
    decl_start_line: u32,
    decl_end_line: u32,
    lines: &LineIndex,
    out: &mut Vec<CodeChunk>,
) {
    let Some(slice) = content.get(byte_start..byte_end) else {
        return;
    };
    if slice.trim().is_empty() {
        return;
    }
    let (start_line, end_line) = lines.line_range_for_bytes(byte_start, byte_end);
    out.push(CodeChunk {
        text: slice.to_string(),
        byte_start,
        byte_end,
        start_line,
        end_line,
        declaration_start_line: decl_start_line,
        declaration_end_line: decl_end_line,
        symbol,
        source: ChunkSource::TreeSitter,
    });
}
