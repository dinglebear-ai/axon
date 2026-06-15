use super::super::super::chunk::SymbolKind;
use super::super::{Extractor, extract_symbols};

#[test]
fn go_captures_interface_type() {
    // type_spec wraps both struct_type and interface_type via its `type` field, so
    // the type_declaration rule captures interfaces too. `@decl` is anchored on the
    // type_spec, so the name is kept (no longer nulled): a Type named `R` spanning
    // the interface body.
    let src = "package demo\n\ntype R interface {\n\tRead() int\n}\n";
    let symbols = extract_symbols(src, Extractor::Go);
    let interface = symbols.iter().find(|s| s.kind == SymbolKind::Type);
    assert!(
        interface.is_some(),
        "interface type should capture as a Type decl: {symbols:?}"
    );
    let interface = interface.unwrap();
    assert!(interface.end_line > interface.start_line);
    assert_eq!(
        interface.name.as_deref(),
        Some("R"),
        "interface type must be named R: {symbols:?}"
    );
}

#[test]
fn go_captures_multi_spec_const_block_per_spec() {
    // A `const ( A = 1; B = 2 )` block: `@decl` anchors on each const_spec, so the
    // block yields one named Const per spec (go/ast-style per-spec naming).
    let src = "package demo\n\nconst (\n\tA = 1\n\tB = 2\n)\n";
    let symbols = extract_symbols(src, Extractor::Go);
    let consts: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Const)
        .collect();
    assert_eq!(
        consts.len(),
        2,
        "multi-spec const block should yield one Const per spec: {symbols:?}"
    );
    let names: Vec<_> = consts.iter().filter_map(|s| s.name.as_deref()).collect();
    assert!(
        names.contains(&"A") && names.contains(&"B"),
        "consts named A and B: {symbols:?}"
    );
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
