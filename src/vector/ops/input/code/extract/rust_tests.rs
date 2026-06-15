use super::super::super::chunk::SymbolKind;
use super::super::{Extractor, extract_symbols};

#[test]
fn rust_captures_macro_rules() {
    let src = "macro_rules! foo {\n    () => {};\n}\n";
    let symbols = extract_symbols(src, Extractor::Rust);
    // No dedicated Macro variant; mapped to Mod for searchability.
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("foo") && s.kind == SymbolKind::Mod),
        "macro_rules! foo should capture as Mod: {symbols:?}"
    );
}

#[test]
fn rust_qualifies_impl_method() {
    let src = "struct T;\nimpl T {\n    fn m(&self) {}\n}\n";
    let symbols = extract_symbols(src, Extractor::Rust);
    assert!(
        symbols
            .iter()
            .any(|s| s.name.as_deref() == Some("T::m") && s.kind == SymbolKind::Method),
        "impl method should qualify as T::m: {symbols:?}"
    );
}
