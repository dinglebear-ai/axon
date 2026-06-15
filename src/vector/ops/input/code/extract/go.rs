use tree_sitter::Node;

use super::super::chunk::SymbolKind;
use super::{DeclRole, DeclRule, clean_symbol_fragment};

/// Go declaration rules.
///
/// `const`/`var`/`type` declarations carry their names in nested `*_spec`
/// children, not a `name` field on the declaration node. We anchor `@decl` on the
/// **spec** (not the outer declaration), so a grouped block
/// `const ( A = 1; B = 2 )` yields one named chunk per spec — matching `go/ast`'s
/// per-spec model (bead axon_rust-8rpa: full per-language parity; closes the Go
/// symbol-coverage gap vs lumen). The wrapper bytes (`const (`, `)`, blank lines)
/// between specs fall to the residual sweep and drop below the floor. [`refine`]
/// qualifies receiver methods and otherwise keeps the captured spec name.
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
        pattern: "(const_declaration (const_spec name: (identifier) @name) @decl)",
        kind: SymbolKind::Const,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(var_declaration (var_spec name: (identifier) @name) @decl)",
        kind: SymbolKind::Static,
        role: DeclRole::Leaf,
    },
    DeclRule {
        pattern: "(type_declaration (type_spec name: (type_identifier) @name) @decl)",
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

#[cfg(test)]
#[path = "go_tests.rs"]
mod tests;
