//! Residual gap sweep: the byte ranges *between* declarations.
//!
//! A gap is the source span the cursor walk skips over between one declaration
//! and the next (and the leading/trailing spans before the first / after the
//! last declaration). Most gaps are pure structural punctuation — closing
//! braces, semicolons, blank lines, dangling `() =>` fragments — which would
//! otherwise become symbol-less slivers in the index. The sweep measures the
//! *stripped* length (after removing whitespace and punctuation-only lines) and
//! drops the gap entirely when it falls below the floor; only gaps carrying real
//! prose/code survive as [`ChunkSource::Prose`] chunks — split into bounded
//! pieces when the gap is larger than [`MAX_CODE_CHUNK_CHARS`] so a big
//! non-declaration span (a top-level const object, a test file's `describe`
//! blocks) never becomes one oversized chunk that dilutes retrieval.

use super::super::chunk::{ChunkSource, CodeChunk};
use super::LineIndex;
use super::oversized::{MAX_CODE_CHUNK_CHARS, split_offsets};

/// Default minimum stripped-char count for a residual gap to be kept.
const RESIDUAL_GAP_FLOOR_CHARS_DEFAULT: usize = 80;

/// Resolve the residual-gap floor, honoring the `AXON_RESIDUAL_GAP_FLOOR`
/// override (parsed as `usize`; malformed values fall back to the default).
pub(super) fn residual_gap_floor_chars() -> usize {
    std::env::var("AXON_RESIDUAL_GAP_FLOOR")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(RESIDUAL_GAP_FLOOR_CHARS_DEFAULT)
}

/// Sweep the gap `[start, end)`; emit one or more bounded prose chunks when it
/// carries enough real content, otherwise drop it. A gap larger than
/// [`MAX_CODE_CHUNK_CHARS`] is split at line boundaries (via the shared
/// [`split_offsets`]) so no residual chunk exceeds the cap. Multibyte-safe.
pub(super) fn sweep_residual(
    content: &str,
    start: usize,
    end: usize,
    floor: usize,
    lines: &LineIndex,
    out: &mut Vec<CodeChunk>,
) {
    if start >= end {
        return;
    }
    let Some(slice) = content.get(start..end) else {
        return;
    };
    if stripped_len_reaches(slice, floor) < floor {
        return;
    }
    // Trim to the meaningful region so line accounting and the embedded text both
    // refer to real content, not the surrounding noise.
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return;
    }
    let lead = slice.len() - slice.trim_start().len();
    let chunk_start = start + lead;
    let chunk_end = chunk_start + trimmed.len();

    // Fits in one chunk → emit directly. Otherwise split the span at line
    // boundaries (overlapping, cap-bounded) exactly like an oversized declaration.
    if trimmed.chars().count() <= MAX_CODE_CHUNK_CHARS {
        push_prose(content, chunk_start, chunk_end, lines, out);
        return;
    }
    let Some(span) = content.get(chunk_start..chunk_end) else {
        return;
    };
    for (sub_start, sub_end) in split_offsets(span, chunk_start) {
        push_prose(content, sub_start, sub_end, lines, out);
    }
}

/// Emit one trimmed [`ChunkSource::Prose`] chunk for the byte range
/// `[start, end)`, recomputing offsets/line range from the trimmed text. Drops
/// the chunk if it trims to empty.
fn push_prose(
    content: &str,
    start: usize,
    end: usize,
    lines: &LineIndex,
    out: &mut Vec<CodeChunk>,
) {
    let Some(slice) = content.get(start..end) else {
        return;
    };
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return;
    }
    let lead = slice.len() - slice.trim_start().len();
    let chunk_start = start + lead;
    let chunk_end = chunk_start + trimmed.len();
    let (start_line, end_line) = lines.line_range_for_bytes(chunk_start, chunk_end);
    out.push(CodeChunk {
        text: trimmed.to_string(),
        byte_start: chunk_start,
        byte_end: chunk_end,
        start_line,
        end_line,
        declaration_start_line: start_line,
        declaration_end_line: end_line,
        symbol: None,
        source: ChunkSource::Prose,
    });
}

/// Count "meaningful" characters in `slice`, short-circuiting once `floor` is
/// reached. A line is meaningful only if, after trimming, it contains at least
/// one character that is not structural punctuation (`{ } ( ) ; , = >` and
/// whitespace). This is what kills `}`, `;`, and `() =>` slivers.
fn stripped_len_reaches(slice: &str, floor: usize) -> usize {
    let mut total = 0usize;
    for line in slice.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_punctuation_only(trimmed) {
            continue;
        }
        total += trimmed.chars().count();
        if total >= floor {
            return total;
        }
    }
    total
}

/// Whether every character of an already-trimmed, non-empty line is structural
/// punctuation or whitespace (so the line carries no real content).
fn is_punctuation_only(trimmed: &str) -> bool {
    trimmed
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '{' | '}' | '(' | ')' | ';' | ',' | '=' | '>'))
}
