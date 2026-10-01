use super::*;
fn chunk(source: &str, start: usize, end: usize) -> DocumentChunk {
    DocumentChunk::new(&source[start..end], text::source_range(source, start, end))
}
#[test]
fn junk_and_short_documents_have_no_chunks() {
    for source in ["{}();\n---".repeat(20), "small useful identifier".into()] {
        assert!(useful_chunks(text::plain_text_windows(&source), &source).is_empty());
    }
}
#[test]
fn adjacent_symbols_pack_with_exact_ranges_and_aliases() {
    let source = "fn first() { useful(); }\n\nfn second() { other_useful(); }";
    let split = source.find("fn second").unwrap();
    let chunks = useful_chunks(
        vec![
            chunk(source, 0, split - 2).with_symbol("first"),
            chunk(source, split, source.len()).with_symbol("second"),
        ],
        source,
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, source);
    assert_eq!(
        chunks[0].metadata["code_symbol_aliases"],
        serde_json::json!(["first", "second"])
    );
}
#[test]
fn headings_prevent_unrelated_short_fragments_from_packing() {
    let source = "first small section\n\nsecond small section";
    let split = source.find("second").unwrap();
    assert!(
        useful_chunks(
            vec![
                chunk(source, 0, split - 2).with_heading_path(vec!["one".into()]),
                chunk(source, split, source.len()).with_heading_path(vec!["two".into()])
            ],
            source
        )
        .is_empty()
    );
}
#[test]
fn unicode_oversized_tail_gets_literal_context() {
    let source = "界".repeat(1370);
    let chunks = useful_chunks(text::plain_text_windows(&source), &source);
    assert!(chunks.len() > 1);
    for chunk in chunks {
        assert!(chunk.content.chars().count() >= MIN_CHUNK_CHARS);
        assert!(fits(&chunk.content, 2000));
        assert!(literal(&chunk, &source).is_some());
    }
}

#[test]
fn synthetic_record_tail_keeps_its_pointer_and_content() {
    let mut range = text::source_range("", 0, 0);
    range.json_pointer = Some("/payload".into());
    let first = DocumentChunk::new("a".repeat(2000), range.clone())
        .with_metadata("chunking_fallback", "embedding_size_backstop".into());
    let tail = DocumentChunk::new("useful tail", range)
        .with_metadata("chunking_fallback", "embedding_size_backstop".into());
    let chunks = useful_chunks(vec![first, tail], "{}");
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[1].content.chars().count(), 50);
    assert!(chunks[1].content.ends_with("useful tail"));
    assert_eq!(chunks[1].range.json_pointer.as_deref(), Some("/payload"));
}

#[test]
fn isolated_short_leading_reference_borrows_compatible_following_context() {
    let source = format!("src/lib.rs\n{}", "useful ".repeat(285));
    let split = source.find('\n').unwrap() + 1;
    let chunks = useful_chunks(
        vec![
            chunk(&source, 0, split),
            chunk(&source, split, source.len()),
        ],
        &source,
    );
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].content.starts_with("src/lib.rs"));
    assert_eq!(chunks[0].content.chars().count(), 50);
    assert!(literal(&chunks[0], &source).is_some());
}

#[test]
fn module_prelude_survives_packing_with_symbol_method() {
    let source =
        "use crate::useful;\nfn useful() { println!(\"complete useful context for ingestion\"); }";
    let split = source.find("fn useful").unwrap();
    let symbol = chunk(source, split, source.len())
        .with_symbol("useful")
        .with_metadata("actual_chunking_method", "tree_sitter".into())
        .with_metadata("symbol_extraction_status", "parsed".into());
    let chunks = useful_chunks(
        vec![
            chunk(source, 0, split).with_metadata("symbol_extraction_status", "none".into()),
            symbol,
        ],
        source,
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, source);
    assert_eq!(chunks[0].symbol.as_deref(), Some("useful"));
    assert_eq!(chunks[0].metadata["symbol_extraction_status"], "parsed");
    assert_eq!(chunks[0].metadata["actual_chunking_method"], "tree_sitter");
}

#[test]
fn short_symbol_and_size_backstop_can_pack_with_truthful_method() {
    let source =
        "fn short() {}\nfn long() { println!(\"complete useful context for ingestion\"); }";
    let split = source.find("fn long").unwrap();
    let first = chunk(source, 0, split)
        .with_symbol("short")
        .with_metadata("actual_chunking_method", "tree_sitter".into());
    let second = chunk(source, split, source.len())
        .with_symbol("long")
        .with_metadata("actual_chunking_method", "plain_text_windows".into());
    let chunks = useful_chunks(vec![first, second], source);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, source);
    assert_eq!(
        chunks[0].metadata["actual_chunking_method"],
        "source_adjacent_packing"
    );
    assert_eq!(
        chunks[0].metadata["code_symbol_aliases"],
        serde_json::json!(["long", "short"])
    );
}

#[test]
fn punctuation_between_related_fragments_is_retained_as_context() {
    let source = "useful first reference\n};\nuseful second reference with details";
    let first_end = source.find("\n};").unwrap();
    let second_start = source.find("useful second").unwrap();
    let chunks = useful_chunks(
        vec![
            chunk(source, 0, first_end),
            chunk(source, first_end, second_start),
            chunk(source, second_start, source.len()),
        ],
        source,
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, source);
}
