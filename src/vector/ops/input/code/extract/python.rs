use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, node_name};

/// Python declaration rules. `function_definition` becomes a `Method` (qualified
/// `Class::name`) when nested under a `class_definition`; `class_definition` maps
/// to `Struct` to mirror the original node-walk.
///
/// Grammar parity notes (tree-sitter-python 0.25):
/// - `async def` is NOT a distinct node kind — `async` is a keyword child of
///   `function_definition`, so the bare `function_definition` rule already
///   captures async functions (no separate rule needed).
/// - `decorated_definition` wraps a `function_definition`/`class_definition` via
///   its `definition` field. The decorated rules capture the OUTER range (spanning
///   the `@decorator` lines) so a decorated symbol is one decl covering its
///   decorators. A decorated function still also matches the bare
///   `function_definition` rule at the inner range — `dedup_by_exact_range` only
///   collapses identical ranges, so both survive with the same name+kind, and
///   declaration-driven assembly emits one chunk per range. This duplication is
///   intentional and harmless (see bd axon_rust-8rpa.3).
/// - `assignment left: (identifier) right: (lambda)` captures `f = lambda ...` as a
///   `Function`. `left` is a `pattern` supertype (identifier is a subtype), and
///   `right`/`lambda` resolve through the `expression` supertype — both transparent
///   in queries.
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
    DeclRule {
        pattern: "(decorated_definition \
                   definition: (function_definition name: (identifier) @name)) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(decorated_definition \
                   definition: (class_definition name: (identifier) @name)) @decl",
        kind: SymbolKind::Struct,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(assignment left: (identifier) @name right: (lambda)) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
];

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    // Both a bare `function_definition` and a `decorated_definition` wrapping one
    // are reclassified to a qualified `Method` when nested under a class.
    let is_fn_decl = matches!(decl.kind(), "function_definition" | "decorated_definition");
    if is_fn_decl
        && kind == SymbolKind::Function
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

#[cfg(test)]
#[path = "python_tests.rs"]
mod tests;
