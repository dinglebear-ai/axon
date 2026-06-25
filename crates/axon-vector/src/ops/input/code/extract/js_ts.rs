use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, node_name};

/// TypeScript / TSX declaration rules. Mirrors the original `js_ts_symbol_from_node`
/// node-kind mapping: function/generator → Function, class → Struct,
/// interface/type-alias → Type, method def/sig → Method (qualified `Class::name`),
/// plus declaration-parity rules (bead axon_rust-8rpa.2): name-bound
/// arrow-fn / function-expression / generator value bindings, `export`-wrapped
/// declarations, exported non-function consts, and TS enums.
///
/// TypeScript names class/interface/type-alias with `type_identifier`; methods use
/// `property_identifier`. The JavaScript grammar lacks `interface_declaration`,
/// `type_alias_declaration`, `method_signature`, and `enum_declaration` nodes and
/// names classes with a plain `identifier`, so JS uses the narrower [`JS_RULES`]
/// slice (a query that references a node kind absent from a grammar fails to
/// compile). Rules are ordered general → specific: `dedup_by_exact_range` keeps the
/// later (more specific) rule when two rules capture the identical `@decl` range, so
/// the exported-const rule precedes the exported-arrow-fn rule.
pub(super) static TS_RULES: &[DeclRule] = &[
    DeclRule {
        pattern: "(function_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(generator_function_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(class_declaration name: (type_identifier) @name) @decl",
        kind: SymbolKind::Struct,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(interface_declaration name: (type_identifier) @name) @decl",
        kind: SymbolKind::Type,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(type_alias_declaration name: (type_identifier) @name) @decl",
        kind: SymbolKind::Type,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(enum_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Enum,
        role: DeclRole::Container,
    },
    // Method rules are anchored to their container body (`class_body` /
    // `interface_body`) — NOT matched bare. In TSX, statement-level call and
    // control-flow constructs (`setRun(x)`, `if (x) { ... }`) share the
    // `name(args) { ... }` shape and tree-sitter types them as
    // `method_definition`/`method_signature` nodes; a bare rule captured them as
    // standalone `method` chunks with misleading symbols (bead axon_rust-2ykl).
    // Anchoring keeps real class/interface members and drops the impostors.
    DeclRule {
        pattern: "(class_body (method_definition name: (property_identifier) @name) @decl)",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(interface_body (method_signature name: (property_identifier) @name) @decl)",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
    // Name-bound arrow-fn / function-expression / generator value bindings.
    DeclRule {
        pattern: ARROW_LEXICAL,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: ARROW_VAR,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    // Exported `const`/`let`/`var` bindings — `@decl` covers the `export`
    // keyword. Only the lexical/variable export forms need a dedicated rule:
    // `export function`, `export class`, `export default function name`, and
    // `export class` are already captured at the inner-node range by the
    // function/generator/class rules above (the `export` keyword does not change
    // the inner declaration's node kind). The export-const rule (general) precedes
    // the export-arrow rule (specific) so the latter wins the exact-range dedup
    // tie and `export const Foo = () => {}` resolves to Function, not Const.
    DeclRule {
        pattern: "(export_statement (lexical_declaration (variable_declarator name: (identifier) @name))) @decl",
        kind: SymbolKind::Const,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: EXPORT_ARROW,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
];

/// JavaScript declaration rules — the subset of node kinds the JS grammar defines.
/// JS classes are named with `identifier` (not `type_identifier`), and JS has no
/// `enum_declaration`, `interface_declaration`, `type_alias_declaration`, or
/// `method_signature`.
pub(super) static JS_RULES: &[DeclRule] = &[
    DeclRule {
        pattern: "(function_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(generator_function_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(class_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Struct,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(class_body (method_definition name: (property_identifier) @name) @decl)",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
    // Name-bound arrow-fn / function-expression / generator value bindings.
    DeclRule {
        pattern: ARROW_LEXICAL,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: ARROW_VAR,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    // Exported `const`/`let`/`var` bindings — `@decl` covers the `export`
    // keyword. `export function`/`export class`/`export default function name`
    // are already captured at the inner-node range by the rules above. General
    // (export-const) before specific (export-arrow) for the exact-range dedup
    // tie-break.
    DeclRule {
        pattern: "(export_statement (lexical_declaration (variable_declarator name: (identifier) @name))) @decl",
        kind: SymbolKind::Const,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: EXPORT_ARROW,
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
];

/// `const Foo = () => {}` / `const Foo = function(){}` / `const Foo = function*(){}`.
const ARROW_LEXICAL: &str = "(lexical_declaration (variable_declarator \
    name: (identifier) @name \
    value: [(arrow_function) (function_expression) (generator_function)])) @decl";

/// `var Foo = () => {}` (and `function`/`function*` value forms).
const ARROW_VAR: &str = "(variable_declaration (variable_declarator \
    name: (identifier) @name \
    value: [(arrow_function) (function_expression) (generator_function)])) @decl";

/// `export const Foo = () => {}` — `@decl` covers the `export` keyword.
const EXPORT_ARROW: &str = "(export_statement (lexical_declaration (variable_declarator \
    name: (identifier) @name \
    value: [(arrow_function) (function_expression)]))) @decl";

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    if matches!(decl.kind(), "method_definition" | "method_signature") {
        let name = match js_ts_class_ancestor_name(decl, content) {
            Some(parent) => format!("{parent}::{raw_name}"),
            None => raw_name.to_string(),
        };
        return (SymbolKind::Method, Some(name));
    }
    (kind, Some(raw_name.to_string()))
}

fn js_ts_class_ancestor_name(mut node: Node<'_>, content: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "class_declaration" | "class") {
            return node_name(parent, content);
        }
        node = parent;
    }
    None
}

#[cfg(test)]
#[path = "js_ts_tests.rs"]
mod tests;
