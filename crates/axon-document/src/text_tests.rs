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
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, "alpha\r\n\r\nbeta");
    assert_eq!(chunks[0].range.byte_start, Some(1));
    assert_eq!(chunks[0].range.byte_end, Some(14));
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

#[test]
fn paragraphs_pack_without_exceeding_limits() {
    let source = (0..80)
        .map(|_| "a useful paragraph with several words")
        .collect::<Vec<_>>()
        .join("\n\n");
    let chunks = plain_text_windows(&source);
    assert!(chunks.len() < 4);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.content.len() <= 4096 && chunk.content.chars().count() <= 2000)
    );
}

#[test]
fn position_index_is_sparse_for_large_unicode_documents() {
    let source = "Unicode 界 with reference context\n".repeat(10000);
    let positions = SourcePositions::new(&source);
    assert!(positions.checkpoints.len() <= source.len() / 256 + 1);
    let start = source.len() - "Unicode 界 with reference context\n".len();
    assert_eq!(
        positions.source_range(start, source.len()),
        source_range(&source, start, source.len())
    );
}
