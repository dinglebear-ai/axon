use super::super::super::chunk::SymbolKind;
use super::super::{Extractor, extract_symbols};

#[test]
fn bash_captures_both_function_forms() {
    // `f() {}` POSIX form and `function f {}` keyword form both parse to
    // function_definition in tree-sitter-bash 0.25.
    let src = "posix_form() {\n  echo hi\n}\n\nfunction keyword_form {\n  echo bye\n}\n";
    let symbols = extract_symbols(src, Extractor::Bash);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("posix_form") && s.kind == SymbolKind::Function),
        "f() {{}} form should capture: {symbols:?}"
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("keyword_form") && s.kind == SymbolKind::Function),
        "function f {{}} form should capture: {symbols:?}"
    );
}
