use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, clean_symbol_fragment};

/// Go declaration rules.
///
/// `const_declaration`/`var_declaration`/`type_declaration` carry their names in
/// nested `*_spec` children, not a `name` field on the declaration node, so the
/// old node-walk produced `name: None` for them (it read only the declaration's
/// own `name` field). We mirror that exactly: these three rules use an anchored
/// wildcard `@name` capture purely to satisfy the query's capture requirement
/// and obtain the declaration range, then [`refine`] nulls the name back out.
pub(super) static RULES: &[DeclRule] = &[
    DeclRule {
        pattern: "(function_declaration name: (identifier) @name) @decl",
        kind: SymbolKind::Function,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(method_declaration name: (field_identifier) @name) @decl",
        kind: SymbolKind::Method,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(const_declaration (const_spec name: (identifier) @name)) @decl",
        kind: SymbolKind::Const,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(var_declaration (var_spec name: (identifier) @name)) @decl",
        kind: SymbolKind::Static,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(type_declaration (type_spec name: (type_identifier) @name)) @decl",
        kind: SymbolKind::Type,
        role: DeclRole::Leaf,
    },
];

pub(super) fn refine(
    decl: Node<'_>,
    raw_name: &str,
    content: &str,
    kind: SymbolKind,
) -> (SymbolKind, Option<String>) {
    match decl.kind() {
        "method_declaration" => {
            let name = match go_receiver_name(decl, content) {
                Some(receiver) => format!("{receiver}.{raw_name}"),
                None => raw_name.to_string(),
            };
            (kind, Some(name))
        }
        // Parity: declaration node has no `name` field → old walk gave `name: None`.
        "const_declaration" | "var_declaration" | "type_declaration" => (kind, None),
        _ => (kind, Some(raw_name.to_string())),
    }
}

fn go_receiver_name(node: Node<'_>, content: &str) -> Option<String> {
    let receiver = node
        .child_by_field_name("receiver")
        .and_then(|n| n.utf8_text(content.as_bytes()).ok())?;
    let trimmed = receiver
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let ty = trimmed.split_whitespace().last().unwrap_or(trimmed);
    Some(clean_symbol_fragment(ty))
}
