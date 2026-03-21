pub mod classify;
pub mod code;

use crate::crates::core::http::normalize_url;
use std::collections::HashSet;
use text_splitter::{ChunkConfig, MarkdownSplitter};

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

/// Split markdown content at structural boundaries (headers, paragraphs).
///
/// Uses `MarkdownSplitter` from the `text_splitter` crate. Chunks target
/// 500–2000 characters, splitting on `##`/`###` headers and `\n\n` paragraph
/// breaks before falling back to sentence or word boundaries. Empty and
/// whitespace-only chunks are filtered.
///
/// Use this for all markdown content (web crawl, GitHub READMEs, wikis).
/// Use `chunk_text()` for plain text (Reddit posts, YouTube transcripts).
pub fn chunk_markdown(text: &str) -> Vec<String> {
    let config = ChunkConfig::new(500..2000)
        .with_overlap(200)
        .expect("overlap 200 < min capacity 500");
    MarkdownSplitter::new(config)
        .chunks(text)
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty())
        .collect()
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

    // ── chunk_markdown ───────────────────────────────────────────────────────

    #[test]
    fn chunk_markdown_splits_on_headers() {
        // Each section needs >500 chars to exceed the minimum chunk size.
        let filler_a = "content about topic A ".repeat(30); // ~660 chars
        let filler_b = "content about topic B ".repeat(30); // ~660 chars
        let text = format!("# Section One\n\n{filler_a}\n\n# Section Two\n\n{filler_b}");
        let chunks = chunk_markdown(&text);
        assert!(
            chunks.len() >= 2,
            "markdown with two headers should produce at least 2 chunks, got: {chunks:?}"
        );
    }

    #[test]
    fn chunk_markdown_no_empty_chunks() {
        let text = "# Title\n\n\n\n## Subtitle\n\nSome content here.\n\n";
        let chunks = chunk_markdown(text);
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                !chunk.trim().is_empty(),
                "chunk {i} must not be empty or whitespace-only"
            );
        }
    }

    #[test]
    fn chunk_markdown_empty_input_returns_no_chunks() {
        let chunks = chunk_markdown("");
        assert!(chunks.is_empty(), "empty markdown must produce no chunks");
    }

    #[test]
    fn chunk_markdown_short_returns_single_chunk() {
        let text = "Just a short paragraph with no headers.";
        let chunks = chunk_markdown(text);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("short paragraph"));
    }
}
