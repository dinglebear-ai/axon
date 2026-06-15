use super::super::super::chunk::SymbolKind;
use super::super::{Extractor, extract_symbols};

#[test]
fn go_captures_interface_type() {
    // type_spec wraps both struct_type and interface_type via its `type` field, so
    // the existing type_declaration rule captures interfaces too. Parity: the
    // type_declaration refine nulls the name (declaration node has no `name` field),
    // so the symbol is a Type with name: None spanning the whole `type R interface`.
    let src = "package demo\n\ntype R interface {\n\tRead() int\n}\n";
    let symbols = extract_symbols(src, Extractor::Go);
    let interface = symbols.iter().find(|s| s.kind == SymbolKind::Type);
    assert!(
        interface.is_some(),
        "interface type should capture as a Type decl: {symbols:?}"
    );
    let interface = interface.unwrap();
    // Range spans the interface body (declaration node, name nulled for parity).
    assert!(interface.end_line > interface.start_line);
    assert!(interface.name.is_none());
}

#[test]
fn go_captures_multi_spec_const_block() {
    // A `const ( A = 1; B = 2 )` block: both const_specs share the same outer
    // const_declaration @decl range, so dedup_by_exact_range collapses them to a
    // single Const symbol (the declaration node has no `name` field → name: None).
    let src = "package demo\n\nconst (\n\tA = 1\n\tB = 2\n)\n";
    let symbols = extract_symbols(src, Extractor::Go);
    let consts: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Const)
        .collect();
    assert_eq!(
        consts.len(),
        1,
        "multi-spec const block should yield one Const decl: {symbols:?}"
    );
    // The single decl spans the whole `const (...)` block.
    assert!(consts[0].end_line > consts[0].start_line);
}

#[test]
fn go_qualifies_pointer_receiver_method() {
    let src = "package demo\n\ntype R struct{}\n\nfunc (r *R) M() {}\n";
    let symbols = extract_symbols(src, Extractor::Go);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("R.M") && s.kind == SymbolKind::Method),
        "pointer-receiver method should qualify as R.M: {symbols:?}"
    );
}
