use super::renumber_context_source_header;

#[test]
fn renumber_context_source_header_updates_existing_source_id() {
    let entry = "## Top Chunk [S11]: https://docs.example.com\n\nbody";
    assert_eq!(
        renumber_context_source_header(entry, 1),
        "## Top Chunk [S1]: https://docs.example.com\n\nbody"
    );
}

#[test]
fn renumber_context_source_header_leaves_malformed_header_unchanged() {
    let entry = "## Top Chunk [SX]: https://docs.example.com\n\nbody";
    assert_eq!(renumber_context_source_header(entry, 1), entry);
}
