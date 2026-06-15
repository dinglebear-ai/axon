use super::super::super::chunk::SymbolKind;
use super::super::{Extractor, extract_symbols};

#[test]
fn python_captures_async_function() {
    // `async def` is not a distinct node in tree-sitter-python 0.25; the bare
    // function_definition rule must still capture it.
    let src = "async def fetch():\n    return 1\n";
    let symbols = extract_symbols(src, Extractor::Python);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("fetch") && s.kind == SymbolKind::Function),
        "async def should capture as Function: {symbols:?}"
    );
}

#[test]
fn python_captures_decorated_function_spanning_decorator() {
    let src = "@deco\ndef h():\n    return 2\n";
    let symbols = extract_symbols(src, Extractor::Python);
    // The decorated capture spans the @deco line (starts at line 1).
    let decorated = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("h") && s.start_line == 1);
    assert!(
        decorated.is_some(),
        "decorated def should produce a decl spanning the decorator: {symbols:?}"
    );
    assert_eq!(decorated.unwrap().kind, SymbolKind::Function);
}

#[test]
fn python_captures_decorated_class() {
    let src = "@register\nclass Widget:\n    pass\n";
    let symbols = extract_symbols(src, Extractor::Python);
    let decorated = symbols
        .iter()
        .find(|s| s.name.as_deref() == Some("Widget") && s.start_line == 1);
    assert!(
        decorated.is_some(),
        "decorated class should produce a decl spanning the decorator: {symbols:?}"
    );
    assert_eq!(decorated.unwrap().kind, SymbolKind::Struct);
}

#[test]
fn python_captures_lambda_assignment() {
    let src = "g = lambda x: x\n";
    let symbols = extract_symbols(src, Extractor::Python);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("g") && s.kind == SymbolKind::Function),
        "lambda assignment should capture as Function: {symbols:?}"
    );
}

#[test]
fn python_qualifies_method_in_class() {
    let src = "class C:\n    def m(self):\n        return self\n";
    let symbols = extract_symbols(src, Extractor::Python);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("C::m") && s.kind == SymbolKind::Method),
        "method should qualify as C::m: {symbols:?}"
    );
}
