use super::*;

#[test]
fn rust_extracts_function_and_method_symbols() {
    let src = r#"
struct Response;

impl Response {
    pub fn parse(&self) {}
}

fn free_fn() {}
"#;

    let symbols = extract_symbols(src, Extractor::Rust);
    assert!(symbols.iter().any(|sym| {
        sym.name.as_deref() == Some("Response::parse") && sym.kind == SymbolKind::Method
    }));
    assert!(
        symbols.iter().any(|sym| {
            sym.name.as_deref() == Some("free_fn") && sym.kind == SymbolKind::Function
        })
    );
}

#[test]
fn go_extracts_function_and_receiver_method_symbols() {
    let src = r#"
package demo

type Response struct {}

func (r *Response) Parse() {}

func Free() {}
"#;

    let symbols = extract_symbols(src, Extractor::Go);
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Response.Parse")
                && sym.kind == SymbolKind::Method)
    );
    assert!(
        symbols
            .iter()
            .any(|sym| sym.name.as_deref() == Some("Free") && sym.kind == SymbolKind::Function)
    );
}
