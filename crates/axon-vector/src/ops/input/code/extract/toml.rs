use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule};

/// TOML declaration rules. The natural chunk unit is a **table** (`[section]` /
/// `[server.db]`) or an **array-of-tables element** (`[[items]]`): the whole
/// block (header + its key-values) becomes one chunk named by the section path.
/// Top-level key-value pairs that appear before any table are captured too. All
/// are [`SymbolKind::Key`]. Headers are matched as direct children of the table /
/// document node, so nested pair keys never match (they live inside their table's
/// chunk, bounded by the oversized split).
pub(super) static RULES: &[DeclRule] = &[
    // Top-level key-value pairs (before the first table).
    DeclRule {
        pattern: "(document (pair (bare_key) @name) @decl)",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(document (pair (dotted_key) @name) @decl)",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
    // `[table]` / `[a.b.c]`.
    DeclRule {
        pattern: "(table (bare_key) @name) @decl",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(table (dotted_key) @name) @decl",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
    // `[[array_of_tables]]`.
    DeclRule {
        pattern: "(table_array_element (bare_key) @name) @decl",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(table_array_element (dotted_key) @name) @decl",
        kind: SymbolKind::Key,
        role: DeclRole::Leaf,
    },
];

pub(super) fn refine(
    _decl: Node<'_>,
    raw_name: &str,
    _content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    (kind, Some(raw_name.to_string()))
}

#[cfg(test)]
#[path = "toml_tests.rs"]
mod tests;
