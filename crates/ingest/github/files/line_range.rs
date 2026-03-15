/// Compute line range (1-indexed, inclusive) for a chunk within content.
///
/// Finds the chunk's byte offset via substring search, then counts newlines
/// preceding the start and within the chunk to derive start/end lines.
pub(super) fn line_range_for_chunk(content: &str, chunk: &str) -> (u32, u32) {
    let byte_offset = content.find(chunk).unwrap_or(0);
    // Lines before this chunk (1-indexed).
    let start_line = content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1;
    let lines_in_chunk = chunk.bytes().filter(|&b| b == b'\n').count() as u32;
    let end_line = start_line + lines_in_chunk;
    (start_line, end_line)
}

#[cfg(test)]
mod tests {
    use super::line_range_for_chunk;

    #[test]
    fn line_range_first_line() {
        let content = "hello world";
        let (start, end) = line_range_for_chunk(content, "hello world");
        assert_eq!(start, 1);
        assert_eq!(end, 1);
    }

    #[test]
    fn line_range_multi_line_content() {
        let content = "line1\nline2\nline3\nline4\nline5";
        // Chunk spanning lines 3-4
        let (start, end) = line_range_for_chunk(content, "line3\nline4");
        assert_eq!(start, 3);
        assert_eq!(end, 4);
    }

    #[test]
    fn line_range_chunk_not_found_defaults_to_start() {
        let content = "fn main() {}";
        let (start, end) = line_range_for_chunk(content, "not_in_content");
        // Falls back to byte_offset=0, so line 1
        assert_eq!(start, 1);
        assert_eq!(end, 1);
    }
}
