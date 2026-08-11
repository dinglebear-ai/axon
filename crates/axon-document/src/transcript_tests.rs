use super::*;

#[test]
fn line_chunks_preserve_utf8_and_crlf_source_ranges() {
    let text = "alpha\r\nβeta\r\n\r\ngamma";

    let chunks = split_on_nonempty_lines(text, "session_turn");

    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.content.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "βeta", "gamma"]
    );
    assert_eq!(chunks[0].range.line_start, Some(1));
    assert_eq!(chunks[0].range.line_end, Some(1));
    assert_eq!(chunks[0].range.byte_start, Some(0));
    assert_eq!(chunks[0].range.byte_end, Some(5));
    assert_eq!(chunks[0].range.char_start, Some(0));
    assert_eq!(chunks[0].range.char_end, Some(5));

    assert_eq!(chunks[1].range.line_start, Some(2));
    assert_eq!(chunks[1].range.line_end, Some(2));
    assert_eq!(chunks[1].range.byte_start, Some(7));
    assert_eq!(chunks[1].range.byte_end, Some(12));
    assert_eq!(chunks[1].range.char_start, Some(7));
    assert_eq!(chunks[1].range.char_end, Some(11));

    assert_eq!(chunks[2].range.line_start, Some(4));
    assert_eq!(chunks[2].range.line_end, Some(4));
    assert_eq!(chunks[2].range.byte_start, Some(16));
    assert_eq!(chunks[2].range.byte_end, Some(21));
    assert_eq!(chunks[2].range.char_start, Some(15));
    assert_eq!(chunks[2].range.char_end, Some(20));
}
