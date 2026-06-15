use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule};

/// YAML declaration rules: each **top-level** mapping key becomes a chunk named
/// by the key (kind [`SymbolKind::Key`]). Anchored through `document → block_node
/// → block_mapping` so only top-level keys match — a nested key lives inside its
/// parent's chunk. Multi-document streams (`---`) match per document. A top-level
/// sequence or scalar matches nothing → whole-file prose fallback.
pub(super) static RULES: &[DeclRule] = &[DeclRule {
    pattern: "(document (block_node (block_mapping (block_mapping_pair key: (flow_node) @name) @decl)))",
    kind: SymbolKind::Key,
    role: DeclRole::Leaf,
}];

pub(super) fn refine(
    _decl: Node<'_>,
    raw_name: &str,
    _content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    // A quoted key (`"foo": 1` / `'foo': 1`) captures the surrounding quote
    // tokens in the flow_node; strip them so the symbol name is the bare key.
    let trimmed = raw_name.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed);
    (kind, Some(unquoted.to_string()))
}

#[cfg(test)]
#[path = "yaml_tests.rs"]
mod tests;
