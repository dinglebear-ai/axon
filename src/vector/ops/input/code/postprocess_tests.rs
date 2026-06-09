use super::*;
use crate::vector::ops::input::code::Symbol;
use crate::vector::ops::input::code::chunk::CodeChunk;

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
