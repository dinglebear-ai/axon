use super::*;

#[test]
fn plain_text_windows_splits_single_long_paragraph_into_bounded_chunks() {
    let text = "a".repeat(MAX_PLAIN_TEXT_CHUNK_BYTES * 2 + 17);
    let chunks = plain_text_windows(&text);
    assert!(chunks.len() > 2);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.content.as_str())
            .collect::<String>(),
        text
    );
    for chunk in chunks {
        assert!(chunk.content.len() <= MAX_PLAIN_TEXT_CHUNK_BYTES);
        assert!(chunk.content.chars().count() <= MAX_PLAIN_TEXT_CHUNK_CHARS);
    }
}

#[test]
fn plain_text_windows_preserves_original_crlf_ranges() {
    let text = " alpha\r\n\r\nbeta ";
    let chunks = plain_text_windows(text);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].content, "alpha");
    assert_eq!(chunks[0].range.byte_start, Some(1));
    assert_eq!(chunks[0].range.byte_end, Some(6));
    assert_eq!(chunks[1].content, "beta");
    assert_eq!(chunks[1].range.byte_start, Some(10));
    assert_eq!(chunks[1].range.byte_end, Some(14));
}

#[test]
fn indexed_ranges_match_reference_for_unicode_and_crlf() {
    let text = "αβ\r\nemoji 😀\nlast";
    let positions = SourcePositions::new(text);
    for start in text.char_indices().map(|(offset, _)| offset) {
        for end in text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([text.len()])
        {
            if end >= start {
                assert_eq!(
                    positions.source_range(start, end),
                    source_range(text, start, end)
                );
            }
        }
    }
}
