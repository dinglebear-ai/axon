use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, node_name};

/// TypeScript / TSX declaration rules. Mirrors the original `js_ts_symbol_from_node`
/// node-kind mapping: function/generator → Function, class → Struct,
/// interface/type-alias → Type, method def/sig → Method (qualified `Class::name`).
///
/// TypeScript names class/interface/type-alias with `type_identifier`; methods use
/// `property_identifier`. The JavaScript grammar lacks `interface_declaration`,
/// `type_alias_declaration`, and `method_signature` nodes and names classes with a
/// plain `identifier`, so JS uses the narrower [`JS_RULES`] slice (a query that
/// references a node kind absent from a grammar fails to compile).
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
        pattern: "(method_definition name: (property_identifier) @name) @decl",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(method_signature name: (property_identifier) @name) @decl",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
];

/// JavaScript declaration rules — the subset of node kinds the JS grammar defines.
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
        pattern: "(method_definition name: (property_identifier) @name) @decl",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
];

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
