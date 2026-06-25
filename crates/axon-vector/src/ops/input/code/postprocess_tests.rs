use super::*;
use crate::ops::input::code::chunk::CodeChunk;
use crate::ops::input::code::{ChunkSource, Symbol};

fn chunk(text: &str, start: u32, end: u32, kind: SymbolKind) -> CodeChunk {
    CodeChunk {
        text: text.to_string(),
        byte_start: 0,
        byte_end: text.len(),
        start_line: start,
        end_line: end,
        declaration_start_line: start,
        declaration_end_line: end,
        symbol: Some(Symbol {
            kind,
            name: Some("x".to_string()),
        }),
        source: ChunkSource::TreeSitter,
    }
}

#[test]
fn dedupe_keeps_last_same_kind_exact_declaration_range() {
    let first = chunk("const A: i32 = 1;", 10, 10, SymbolKind::Const);
    let mut second = first.clone();
    second.text = "const A: i32 = 2;".to_string();
    let out = dedupe_exact_ranges(vec![first, second]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "const A: i32 = 2;");
}

#[test]
fn dedupe_preserves_different_kinds_on_same_range() {
    let out = dedupe_exact_ranges(vec![
        chunk("impl A {}", 3, 5, SymbolKind::Impl),
        chunk("fn a() {}", 3, 5, SymbolKind::Method),
    ]);
    assert_eq!(out.len(), 2);
}

#[test]
fn rust_doc_comment_attaches_across_attribute() {
    let src = "/// docs\n#[derive(Debug)]\npub struct Thing;\n";
    let out = attach_leading_comments(
        vec![chunk("pub struct Thing;", 3, 3, SymbolKind::Struct)],
        src,
        "rs",
    );
    assert!(out[0].text.starts_with("/// docs\n"));
    assert_eq!(out[0].declaration_start_line, 3);
    assert_eq!(out[0].start_line, 2);
}

#[test]
fn tiny_consts_merge_and_clear_symbol_name() {
    let out = merge_tiny_declarations(vec![
        chunk("const A: i32 = 1;", 1, 1, SymbolKind::Const),
        chunk("const B: i32 = 2;", 2, 2, SymbolKind::Const),
    ]);
    assert_eq!(out.len(), 1);
    assert!(out[0].text.contains("const A"));
    assert!(out[0].text.contains("const B"));
    assert_eq!(out[0].symbol_kind(), Some(SymbolKind::Const));
    assert!(out[0].symbol_name().is_none());
}

#[test]
fn oversized_leading_comment_does_not_starve_declaration_body() {
    // Build a >2000-char leading `///` doc comment above a small fn. Before the
    // fix, the prefix length (> MAX_CODE_CHUNK_CHARS) saturated the body budget
    // to 0 and the body was truncated away, leaving a chunk that was only the
    // comment. The body must survive; the comment must be capped.
    // A non-comment line first so the comment block doesn't touch line 1
    // (leading_comment_prefix declines a block that reaches the file top).
    let mut src = String::from("use std::fmt;\n");
    let mut line_no = 1u32; // the `use` line
    // Each line is ~25 chars; 200 lines comfortably exceeds 2000 chars.
    for i in 0..200 {
        src.push_str(&format!("/// doc line number {i} here\n"));
        line_no += 1;
    }
    let body = "pub fn small() -> i32 { 42 }";
    let decl_line = line_no + 1;
    src.push_str(body);
    src.push('\n');

    let out = attach_leading_comments(
        vec![chunk(body, decl_line, decl_line, SymbolKind::Function)],
        &src,
        "rs",
    );

    assert_eq!(out.len(), 1);
    let text = &out[0].text;
    // The full declaration body survives.
    assert!(
        text.contains(body),
        "declaration body was starved/truncated: {text:?}"
    );
    // The leading comment is present but capped — far below the raw comment size.
    assert!(text.contains("/// doc line"), "comment missing: {text:?}");
    let comment_len = text.len() - body.len();
    assert!(
        comment_len <= MAX_HEADER_CHARS,
        "comment prefix not capped: {comment_len} chars"
    );
}
