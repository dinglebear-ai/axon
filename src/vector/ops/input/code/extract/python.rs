use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, node_name};

/// Python declaration rules. `function_definition` becomes a `Method` (qualified
/// `Class::name`) when nested under a `class_definition`; `class_definition` maps
/// to `Struct` to mirror the original node-walk.
pub(super) static RULES: &[DeclRule] = &[
    DeclRule {
        pattern: "(function_definition name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(class_definition name: (identifier) @name) @decl",
        kind: SymbolKind::Struct,
        role: DeclRole::Container,
    },
];

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    if decl.kind() == "function_definition"
        && let Some(class_name) = python_class_ancestor_name(decl, content)
    {
        return (
            SymbolKind::Method,
            Some(format!("{class_name}::{raw_name}")),
        );
    }
    (kind, Some(raw_name.to_string()))
}

fn python_class_ancestor_name(mut node: Node<'_>, content: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "class_definition" {
            return node_name(parent, content);
        }
        node = parent;
    }
    None
}
