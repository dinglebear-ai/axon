//! Residual gap sweep: the byte ranges *between* declarations.
//!
//! A gap is the source span the cursor walk skips over between one declaration
//! and the next (and the leading/trailing spans before the first / after the
//! last declaration). Most gaps are pure structural punctuation — closing
//! braces, semicolons, blank lines, dangling `() =>` fragments — which would
//! otherwise become symbol-less slivers in the index. The sweep measures the
//! *stripped* length (after removing whitespace and punctuation-only lines) and
//! drops the gap entirely when it falls below the floor; only gaps carrying real
//! prose/code survive as a single [`ChunkSource::Prose`] chunk.

use super::super::chunk::{ChunkSource, CodeChunk};
use super::LineIndex;

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

/// Sweep the gap `[start, end)`; push one prose chunk when it carries enough
/// real content, otherwise drop it. Multibyte-safe: slices via `get`.
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
    // Keep the trimmed region; recompute byte offsets so line accounting and the
    // embedded text both refer to the meaningful span, not the surrounding noise.
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
