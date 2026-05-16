use super::line_range_for_chunk;

#[test]
fn line_range_first_line() {
    let content = "hello world";
    let (start, end) = line_range_for_chunk(content, "hello world", 0);
    assert_eq!(start, 1);
    assert_eq!(end, 1);
}

#[test]
fn line_range_multi_line_content() {
    let content = "line1\nline2\nline3\nline4\nline5";
    let chunk = "line3\nline4";
    let offset = content.find(chunk).unwrap();
    let (start, end) = line_range_for_chunk(content, chunk, offset);
    assert_eq!(start, 3);
    assert_eq!(end, 4);
}

#[test]
fn line_range_offset_beyond_content_clamps() {
    let content = "fn main() {}";
    let (start, end) = line_range_for_chunk(content, "fn main() {}", 9999);
    // Clamped to content.len(), so start_line = newlines in entire content + 1
    assert_eq!(start, 1);
    assert_eq!(end, 1);
}

#[test]
fn line_range_duplicate_chunks_resolved_by_offset() {
    let content = "dup\ndup\ndup";
    // All three lines are "dup". With the old find()-based approach,
    // all would resolve to line 1. With byte_offset, each is correct.
    let (s1, e1) = line_range_for_chunk(content, "dup", 0);
    assert_eq!((s1, e1), (1, 1));

    let (s2, e2) = line_range_for_chunk(content, "dup", 4); // after "dup\n"
    assert_eq!((s2, e2), (2, 2));

    let (s3, e3) = line_range_for_chunk(content, "dup", 8); // after "dup\ndup\n"
    assert_eq!((s3, e3), (3, 3));
}
