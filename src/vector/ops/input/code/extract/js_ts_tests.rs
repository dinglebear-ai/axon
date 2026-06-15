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

/// Regression for bead axon_rust-2ykl. Mirrors apps/palette-tauri/src/App.tsx:414
/// — a JSX-prop arrow whose body contains a nested `if` and bare call
/// expressions. In TSX these statement-level constructs share the shape
/// `name(args) { ... }` / `name(args)` and tree-sitter types them as
/// `method_definition` / `method_signature` nodes; the unscoped method rules used
/// to capture them as `method` declarations (`if`, `setRun`, `runStateFromHistory`
/// — each its own noisy chunk with a misleading symbol). Scoping the method rules
/// to `class_body` / `interface_body` makes that impossible while preserving real
/// class methods (epic axon_rust-8rpa: a declaration is the chunk unit).
#[test]
fn tsx_jsx_prop_arrow_body_does_not_emit_statement_methods() {
    let src = r#"
const App = () => {
  return (
    <Widget
      onOpen={(item) => {
        const historyRun = runStateFromHistory(item);
        if (historyRun) {
          setRun(historyRun);
        }
        const label = item.label.toLowerCase();
        return label.slice(0, 3);
      }}
    />
  );
};
"#;
    let symbols = extract_symbols(src, Extractor::TypeScript);
    // The real declaration is still captured.
    assert!(
        has(&symbols, "App", SymbolKind::Function),
        "the arrow-bound component App must still be a Function: {symbols:?}"
    );
    // No statement-level call / control-flow node leaks in as a method.
    let methods: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method)
        .map(|s| s.name.clone())
        .collect();
    assert!(
        methods.is_empty(),
        "no statement-level method symbols expected, got: {methods:?}\nall: {symbols:?}"
    );
    for bad in [
        "if",
        "toLowerCase",
        "setRun",
        "slice",
        "runStateFromHistory",
    ] {
        assert!(
            !has(&symbols, bad, SymbolKind::Method),
            "{bad:?} must not be captured as a Method: {symbols:?}"
        );
    }
}

/// Guards the other side of bead axon_rust-2ykl: scoping the method rules must
/// NOT drop genuine class methods (a direct `class_body` member).
#[test]
fn ts_class_method_is_still_captured() {
    let src = "class Engine {\n  run(req: number): number {\n    return req;\n  }\n}\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "Engine::run", SymbolKind::Method),
        "class method Engine::run must still be captured: {symbols:?}"
    );
}

/// Guards the interface side: a real `interface_body` method signature is still
/// captured even though the unscoped `method_signature` rule was removed.
#[test]
fn ts_interface_method_signature_is_still_captured() {
    let src = "interface Transport {\n  start(): void;\n}\n";
    let symbols = extract_symbols(src, Extractor::TypeScript);
    assert!(
        has(&symbols, "start", SymbolKind::Method),
        "interface method signature start must still be captured: {symbols:?}"
    );
}

/// The `.tsx` route uses the JSX grammar (`Extractor::Tsx` → `LANGUAGE_TSX`), so a
/// real component with a JSX body and inline event-handler calls parses cleanly:
/// the component is captured and no statement-level call leaks in as a method
/// (CodeRabbit #3 / bead axon_rust-2ykl — the same source feeding the plain-TS
/// grammar is what fabricated the spurious method nodes).
#[test]
fn tsx_extractor_parses_jsx_body_without_spurious_methods() {
    let src = r#"
export const Row = ({ label }: { label: string }) => {
  return (
    <button onClick={() => onClear()}>
      {label.toUpperCase()}
    </button>
  );
};
"#;
    let symbols = extract_symbols(src, Extractor::Tsx);
    assert!(
        has(&symbols, "Row", SymbolKind::Function),
        "the arrow component Row must be captured on the tsx route: {symbols:?}"
    );
    for bad in ["onClick", "onClear", "toUpperCase"] {
        assert!(
            !has(&symbols, bad, SymbolKind::Method),
            "{bad:?} must not be captured as a Method on the tsx route: {symbols:?}"
        );
    }
}
