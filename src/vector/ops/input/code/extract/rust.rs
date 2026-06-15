use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, clean_symbol_fragment};

/// Rust declaration rules. `function_item` is reclassified to `Method` by
/// [`refine`] when it sits inside an `impl_item`; the others are leaf/container
/// as listed. The `name` capture nesting matches each grammar node's `name`
/// field so `@decl` spans the whole declaration.
pub(super) static RULES: &[DeclRule] = &[
    DeclRule {
        pattern: "(function_item name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(struct_item name: (type_identifier) @name) @decl",
        kind: SymbolKind::Struct,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(enum_item name: (type_identifier) @name) @decl",
        kind: SymbolKind::Enum,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(trait_item name: (type_identifier) @name) @decl",
        kind: SymbolKind::Trait,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(impl_item type: (type_identifier) @name) @decl",
        kind: SymbolKind::Impl,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(impl_item type: (generic_type type: (type_identifier) @name)) @decl",
        kind: SymbolKind::Impl,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(impl_item type: (scoped_type_identifier name: (type_identifier) @name)) @decl",
        kind: SymbolKind::Impl,
        role: DeclRole::Container,
    },
    DeclRule {
        pattern: "(const_item name: (identifier) @name) @decl",
        kind: SymbolKind::Const,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(static_item name: (identifier) @name) @decl",
        kind: SymbolKind::Static,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(type_item name: (type_identifier) @name) @decl",
        kind: SymbolKind::Type,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(mod_item name: (identifier) @name) @decl",
        kind: SymbolKind::Mod,
        role: DeclRole::Container,
    },
    // `macro_rules! foo { ... }` parses as `macro_definition` with a `name`
    // (identifier) field in tree-sitter-rust 0.24. No dedicated `Macro` variant
    // exists, so map to `Mod` for searchability (per bd axon_rust-8rpa.3).
    DeclRule {
        pattern: "(macro_definition name: (identifier) @name) @decl",
        kind: SymbolKind::Mod,
        role: DeclRole::Leaf,
    },
];

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    if decl.kind() == "function_item"
        && has_impl_parent(decl)
        && let Some(parent) = rust_impl_type_name(decl, content)
    {
        return (SymbolKind::Method, Some(format!("{parent}::{raw_name}")));
    }
    // Behavior parity: the old node-walk read `child_by_field_name("name")` for
    // every node, and `impl_item` has no `name` field (its identifier is in the
    // `type` field), so impl symbols always carried `name: None`. The query
    // captures the type as `@name` only to obtain a range; null it back out here.
    if decl.kind() == "impl_item" {
        return (kind, None);
    }
    (kind, Some(raw_name.to_string()))
}

fn has_impl_parent(mut node: Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "impl_item" {
            return true;
        }
        node = parent;
    }
    false
}

fn rust_impl_type_name(mut node: Node<'_>, content: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "impl_item" {
            return parent
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                .map(clean_symbol_fragment);
        }
        node = parent;
    }
    None
}

#[cfg(test)]
#[path = "rust_tests.rs"]
mod tests;
