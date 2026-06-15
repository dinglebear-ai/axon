use super::super::*;

/// Count symbols matching a given name + kind across the extracted set.
fn count(symbols: &[SymbolInfo], name: &str, kind: SymbolKind) -> usize {
    symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some(name) && s.kind == kind)
        .count()
}

fn has(symbols: &[SymbolInfo], name: &str, kind: SymbolKind) -> bool {
    count(symbols, name, kind) >= 1
}

#[test]
fn js_name_bound_arrow_is_function() {
    let src = "const Foo = () => {};\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "Foo", SymbolKind::Function),
        "arrow-bound const Foo must be a Function: {symbols:?}"
    );
}

#[test]
fn ts_name_bound_arrow_is_function() {
    let src = "const Foo = () => {};\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Foo", SymbolKind::Function),
        "arrow-bound const Foo must be a Function: {symbols:?}"
    );
}

#[test]
fn js_exported_function_expression_is_function() {
    let src = "export const Bar = function () { return 1; };\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "Bar", SymbolKind::Function),
        "exported function-expression Bar must be a Function: {symbols:?}"
    );
    // Must not also surface as a non-function const.
    assert!(
        !has(&symbols, "Bar", SymbolKind::Const),
        "Bar must not surface as a Const too: {symbols:?}"
    );
}

#[test]
fn ts_exported_function_expression_is_function() {
    let src = "export const Bar = function () { return 1; };\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Bar", SymbolKind::Function),
        "exported function-expression Bar must be a Function: {symbols:?}"
    );
    assert!(
        !has(&symbols, "Bar", SymbolKind::Const),
        "Bar must not surface as a Const too: {symbols:?}"
    );
}

#[test]
fn js_exported_non_function_const_is_const() {
    let src = "export const NUM = 42;\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "NUM", SymbolKind::Const),
        "exported NUM must be a Const: {symbols:?}"
    );
}

#[test]
fn ts_exported_non_function_const_is_const() {
    let src = "export const NUM = 42;\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "NUM", SymbolKind::Const),
        "exported NUM must be a Const: {symbols:?}"
    );
}

#[test]
fn js_exported_function_declaration_is_function() {
    let src = "export function baz() { return 1; }\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "baz", SymbolKind::Function),
        "exported baz must be a Function: {symbols:?}"
    );
}

#[test]
fn ts_exported_function_declaration_is_function() {
    let src = "export function baz() { return 1; }\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "baz", SymbolKind::Function),
        "exported baz must be a Function: {symbols:?}"
    );
}

#[test]
fn js_export_default_named_function_is_function() {
    let src = "export default function qux() { return 1; }\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "qux", SymbolKind::Function),
        "export default function qux must be a Function: {symbols:?}"
    );
}

#[test]
fn ts_export_default_named_function_is_function() {
    let src = "export default function qux() { return 1; }\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "qux", SymbolKind::Function),
        "export default function qux must be a Function: {symbols:?}"
    );
}

#[test]
fn js_exported_class_is_struct() {
    let src = "export class Panel {}\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "Panel", SymbolKind::Struct),
        "exported class Panel must be a Struct: {symbols:?}"
    );
}

#[test]
fn ts_exported_class_is_struct() {
    let src = "export class Panel {}\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Panel", SymbolKind::Struct),
        "exported class Panel must be a Struct: {symbols:?}"
    );
}

#[test]
fn ts_enum_is_enum() {
    let src = "enum Color { Red }\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Color", SymbolKind::Enum),
        "TS enum Color must be an Enum: {symbols:?}"
    );
}

#[test]
fn tsx_exported_arrow_component_is_single_function() {
    // React-component shape: typed destructured props on an exported arrow fn.
    // `extract_symbols(.., TypeScript)` routes to the plain-TS grammar (no tsx
    // ext hint), which rejects raw JSX, so the return value here is a non-JSX
    // expression — the captured declaration shape is identical to a TSX component.
    let src = "export const Widget = ({ x }: Props) => null;\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Widget", SymbolKind::Function),
        "TSX arrow component Widget must be a Function: {symbols:?}"
    );
    // Must not surface as a non-function const, and must not produce `()`/`=>`
    // identifier slivers.
    assert!(
        !has(&symbols, "Widget", SymbolKind::Const),
        "Widget must not also surface as a Const: {symbols:?}"
    );
    assert!(
        symbols
            .iter()
            .all(|s| s.name.as_deref() != Some("(") && s.name.as_deref() != Some("=>")),
        "no `(`/`=>` slivers: {symbols:?}"
    );
}

#[test]
fn js_generator_value_binding_is_function() {
    let src = "const gen = function* () { yield 1; };\n";
    let symbols = extract_symbols(src, Extractor::JavaScript);
    assert!(
        has(&symbols, "gen", SymbolKind::Function),
        "generator value binding gen must be a Function: {symbols:?}"
    );
}
/// `export function` / `export class` / `export default function name` are
/// captured exactly once (at the inner-node range) — the trimmed rule set does
/// not add a redundant `export`-wrapping duplicate for these forms.
#[test]
fn exported_fn_and_class_decls_are_not_double_counted() {
    let cases = [
        ("export function foo(){}\n", "foo", SymbolKind::Function),
        ("export class P {}\n", "P", SymbolKind::Struct),
        (
            "export default function qux(){}\n",
            "qux",
            SymbolKind::Function,
        ),
        ("export const NUM = 42;\n", "NUM", SymbolKind::Const),
    ];
    for (src, name, kind) in cases {
        for extractor in [Extractor::TypeScript, Extractor::JavaScript] {
            let symbols = extract_symbols(src, extractor);
            assert_eq!(
                count(&symbols, name, kind),
                1,
                "{src:?} under {extractor:?} must yield exactly one {name}: {symbols:?}"
            );
        }
    }
}
