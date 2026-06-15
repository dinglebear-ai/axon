use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule};

/// JSON declaration rules: each **top-level** object key becomes a chunk named by
/// the key (kind [`SymbolKind::Key`]). The pattern is anchored to the document
/// root so only top-level keys match — a nested key lives inside its parent key's
/// chunk (bounded by the oversized split). A top-level array or scalar matches
/// nothing, so the file degrades to whole-file prose (never zero chunks).
pub(super) static RULES: &[DeclRule] = &[DeclRule {
    pattern: "(document (object (pair key: (string (string_content) @name)) @decl))",
    kind: SymbolKind::Key,
    role: DeclRole::Leaf,
}];

pub(super) fn refine(
    _decl: Node<'_>,
    raw_name: &str,
    _content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    (kind, Some(raw_name.to_string()))
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
