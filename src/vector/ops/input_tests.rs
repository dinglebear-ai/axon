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

#[test]
fn chunk_markdown_repeats_heading_context_for_split_sections() {
    let body = "Pool risk reason details explain why special vdev redundancy matters. ".repeat(80);
    let text = format!(
        "# Storage Guide\n\n## Special vdev redundancy\n\n{body}\n\n## Next topic\n\nunrelated"
    );

    let chunks = chunk_markdown(&text);
    let section_chunks = chunks
        .iter()
        .filter(|chunk| chunk.contains("Pool risk reason"))
        .collect::<Vec<_>>();

    assert!(
        section_chunks.len() > 1,
        "test fixture should force the section across multiple chunks: {chunks:?}"
    );
    for chunk in section_chunks {
        assert!(
            chunk.contains("# Storage Guide") && chunk.contains("## Special vdev redundancy"),
            "split section chunk should carry heading breadcrumb: {chunk:?}"
        );
    }
}
