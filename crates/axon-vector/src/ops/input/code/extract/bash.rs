use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule};

/// Bash declaration rules. The bash grammar's `function_definition` does not
/// expose a `name` field; its name is a `word` child node. The query captures
/// that child `word` as `@name` (purely to satisfy the capture contract and seed
/// a range) while `@decl` spans the whole definition. [`refine`] then reproduces
/// the original logic exactly — scanning children for the first `word`/
/// `command_name` that is not the literal `function` keyword.
///
/// The single `function_definition` rule covers BOTH bash function forms —
/// `f() {}` and `function f {}` — because tree-sitter-bash 0.25 parses both into a
/// `function_definition` whose name is a `word` child (the `function` keyword, when
/// present, is a separate sibling token that [`refine`] skips). Bash has no other
/// meaningful top-level declaration kind, so this registry is intentionally minimal.
pub(super) static RULES: &[DeclRule] = &[DeclRule {
    pattern: "(function_definition (word) @name) @decl",
    kind: SymbolKind::Function,
    role: DeclRole::Leaf,
}];

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    // raw_name is the captured `word`; refine recomputes via the original scan to
    // preserve exact parity (skipping a leading `function` keyword token).
    let name = bash_function_name(decl, content).unwrap_or_else(|| raw_name.to_string());
    (kind, Some(name))
}

fn bash_function_name(node: Node<'_>, content: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "word" | "command_name")
            && let Ok(name) = child.utf8_text(content.as_bytes())
        {
            let name = name.trim();
            if !name.is_empty() && name != "function" {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "bash_tests.rs"]
mod tests;
