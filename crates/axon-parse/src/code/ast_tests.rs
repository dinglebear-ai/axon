use super::CharOffsets;

#[test]
fn sparse_offsets_preserve_unicode_with_bounded_lookup_scans() {
    let source = "fn café() { let 界 = \"🙂\"; }\r\n".repeat(4000);
    let offsets = CharOffsets::new(&source);
    assert!(offsets.checkpoints.len() <= source.len() / 256 + 1);
    for (chars, (byte, _)) in source.char_indices().enumerate() {
        assert_eq!(offsets.at(byte), chars as u64);
        let index = offsets
            .checkpoints
            .partition_point(|&(offset, _)| offset <= byte)
            - 1;
        assert!(byte - offsets.checkpoints[index].0 < 260);
    }
    assert_eq!(offsets.at(source.len()), source.chars().count() as u64);
}
