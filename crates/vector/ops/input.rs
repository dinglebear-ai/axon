pub mod classify;
pub mod code;

use crate::crates::core::http::normalize_url;
use std::collections::HashSet;

pub fn chunk_text(text: &str) -> Vec<String> {
    const MAX: usize = 2000;
    const OVERLAP: usize = 200;

    // Fast-path: avoid the 800 KB Vec<usize> allocation for short documents.
    if text.chars().count() <= MAX {
        return vec![text.to_string()];
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_count = offsets.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < char_count {
        let end = (i + MAX).min(char_count);
        let byte_start = offsets[i];
        let byte_end = if end < char_count {
            offsets[end]
        } else {
            text.len()
        };
        out.push(text[byte_start..byte_end].to_string());
        if end == char_count {
            break;
        }
        i = end.saturating_sub(OVERLAP);
    }
    out
}

pub fn url_lookup_candidates(target: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let normalized = normalize_url(target);
    let variants = [
        target.to_string(),
        normalized.to_string(),
        normalized.trim_end_matches('/').to_string(),
        format!("{}/", normalized.trim_end_matches('/')),
    ];
    for variant in variants {
        if variant.is_empty() {
            continue;
        }
        if seen.insert(variant.clone()) {
            out.push(variant);
        }
    }
    out
}

#[cfg(test)]
#[path = "input_proptest.rs"]
mod input_proptest;

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK_SIZE: usize = 2000;
    const OVERLAP: usize = 200;

    fn make_text(char_count: usize) -> String {
        // ASCII 'a': single byte per char, so char_count == byte_count here.
        "a".repeat(char_count)
    }

    // ── chunk_text ──────────────────────────────────────────────────────────

    #[test]
    fn chunk_text_empty_returns_single_empty_chunk() {
        // The fast-path fires for text whose char count is <= MAX (including 0).
        // It wraps the whole string in a vec, so empty text → vec![""].
        let result = chunk_text("");
        assert_eq!(
            result.len(),
            1,
            "empty input triggers fast-path, producing 1 chunk"
        );
        assert_eq!(
            result[0], "",
            "the single chunk for empty input is itself empty"
        );
    }

    #[test]
    fn chunk_text_short_returns_single_chunk() {
        let text = make_text(CHUNK_SIZE - 1);
        let chunks = chunk_text(&text);
        assert_eq!(
            chunks.len(),
            1,
            "text under {CHUNK_SIZE} chars should produce 1 chunk"
        );
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn chunk_text_exactly_at_boundary_returns_single_chunk() {
        let text = make_text(CHUNK_SIZE);
        let chunks = chunk_text(&text);
        assert_eq!(
            chunks.len(),
            1,
            "text of exactly {CHUNK_SIZE} chars should produce 1 chunk (fast-path)"
        );
        assert_eq!(chunks[0].chars().count(), CHUNK_SIZE);
    }

    #[test]
    fn chunk_text_slightly_over_boundary_returns_two_chunks() {
        let n = CHUNK_SIZE + 1;
        let text = make_text(n);
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 2, "text of {n} chars should produce 2 chunks");
    }

    #[test]
    fn chunk_text_long_produces_overlap() {
        // A 4000-char text must produce multiple chunks with OVERLAP overlap.
        let text = make_text(4000);
        let chunks = chunk_text(&text);
        assert!(
            chunks.len() >= 2,
            "4000-char text must produce at least 2 chunks"
        );

        // For pure ASCII, char index == byte index.
        // chunk[0] covers [0..CHUNK_SIZE]; chunk[1] starts at CHUNK_SIZE - OVERLAP.
        let chunk1_first_chars: String = chunks[1].chars().take(OVERLAP).collect();
        let expected_overlap: String = text
            .chars()
            .skip(CHUNK_SIZE - OVERLAP)
            .take(OVERLAP)
            .collect();
        assert_eq!(
            chunk1_first_chars, expected_overlap,
            "chunk[1] should start {OVERLAP} chars before the end of chunk[0] (overlap region)"
        );
    }

    #[test]
    fn chunk_text_first_chunk_is_exactly_chunk_size_chars() {
        let text = make_text(CHUNK_SIZE * 2 + 100);
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2);
        let chunk0_chars = chunks[0].chars().count();
        assert_eq!(
            chunk0_chars, CHUNK_SIZE,
            "first chunk should be exactly CHUNK_SIZE={CHUNK_SIZE} chars"
        );
    }

    #[test]
    fn chunk_text_unicode_no_split_codepoints() {
        // 'é' is U+00E9 = 2 bytes in UTF-8.
        // Build a string that is CHUNK_SIZE+50 chars of 'é'.
        let base: String = "é".repeat(CHUNK_SIZE + 50);
        let chunks = chunk_text(&base);
        // Verify each chunk roundtrips through chars() — this would panic at the
        // slice boundary if any cut happened mid-codepoint.
        for (i, chunk) in chunks.iter().enumerate() {
            let round_trip: String = chunk.chars().collect();
            assert_eq!(
                *chunk, round_trip,
                "chunk {i} has invalid char boundaries (mid-codepoint split)"
            );
            assert!(
                chunk.chars().all(|c| c == 'é'),
                "chunk {i} contains unexpected characters"
            );
        }
    }

    #[test]
    fn chunk_text_covers_all_content() {
        // Reassemble: chunk[0] in full, then only the non-overlapping suffix of
        // each subsequent chunk.  The result must equal the original text exactly.
        let text = make_text(CHUNK_SIZE * 3 + 100);
        let chunks = chunk_text(&text);

        let mut reconstructed = chunks[0].clone();
        for chunk in chunks.iter().skip(1) {
            let novel: String = chunk.chars().skip(OVERLAP).collect();
            reconstructed.push_str(&novel);
        }
        assert_eq!(
            reconstructed, text,
            "reassembling chunks should reproduce the original text exactly"
        );
    }

    #[test]
    fn chunk_text_whitespace_only_short_returns_single_chunk() {
        let text = " ".repeat(100);
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }
}
