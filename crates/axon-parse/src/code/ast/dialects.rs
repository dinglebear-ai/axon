use tree_sitter::Node;

use super::{CodeLanguage, SymbolDescriptor};

pub(super) fn descriptor(
    node: Node<'_>,
    source: &str,
    language: CodeLanguage,
) -> Option<SymbolDescriptor> {
    match language {
        CodeLanguage::Css => css(node, source),
        CodeLanguage::Elixir => elixir(node, source),
        _ => None,
    }
}

fn css(node: Node<'_>, source: &str) -> Option<SymbolDescriptor> {
    let kind = match node.kind() {
        "rule_set" | "keyframe_block" => "css_rule",
        "at_rule"
        | "media_statement"
        | "supports_statement"
        | "keyframes_statement"
        | "import_statement"
        | "charset_statement"
        | "namespace_statement" => "css_at_rule",
        _ => return None,
    };
    let mut cursor = node.walk();
    let end = node
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "block" | "keyframe_block_list"))
        .map_or(node.end_byte(), |child| child.start_byte());
    let name = source[node.start_byte()..end]
        .trim()
        .trim_end_matches(';')
        .to_string();
    (!name.is_empty()).then_some(SymbolDescriptor { name, kind })
}

fn target<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    let target = node.child_by_field_name("target")?;
    (target.kind() == "identifier")
        .then(|| target.utf8_text(source.as_bytes()).ok())
        .flatten()
}

fn elixir(node: Node<'_>, source: &str) -> Option<SymbolDescriptor> {
    if node.kind() != "call" {
        return None;
    }
    // A quoted declaration is syntax data rather than a declared source symbol.
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        if ancestor.kind() == "call" && target(ancestor, source) == Some("quote") {
            return None;
        }
        parent = ancestor.parent();
    }
    let kind = match target(node, source)? {
        "defmodule" | "defprotocol" => "module",
        "defimpl" => "impl",
        "def" | "defp" => "function",
        "defmacro" | "defmacrop" => "macro",
        _ => return None,
    };
    let mut cursor = node.walk();
    let args = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments")?;
    let mut cursor = node.walk();
    let has_block = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "do_block");
    let mut cursor = args.walk();
    let has_inline_body = args.named_children(&mut cursor).any(|child| {
        child.kind() == "keywords"
            && child
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.trim_start().starts_with("do:"))
    });
    if !has_block && !has_inline_body {
        // Valid multi-clause function heads may omit a body. Recovery can also
        // produce a clean `def name` node before an unmatched opening paren.
        let trailing = &source[node.end_byte()..];
        if matches!(kind, "module" | "impl")
            || (!trailing.is_empty() && !trailing.starts_with(['\n', '\r']))
        {
            return None;
        }
    }
    let mut head = args.named_child(0)?;
    if head.kind() == "binary_operator" {
        if head
            .child_by_field_name("operator")?
            .utf8_text(source.as_bytes())
            .ok()?
            != "when"
        {
            return None;
        }
        head = head.child_by_field_name("left")?;
    }
    let name = if kind == "impl" {
        implementation_name(args, head, source)?
    } else if kind == "module" {
        head.utf8_text(source.as_bytes()).ok()?.to_string()
    } else {
        let name_node = if head.kind() == "call" {
            head.child_by_field_name("target")?
        } else {
            head
        };
        if name_node.kind() != "identifier" {
            return None;
        }
        let name = name_node.utf8_text(source.as_bytes()).ok()?;
        if matches!(name, "unquote" | "unquote_splicing") {
            return None;
        }
        name.to_string()
    };
    Some(SymbolDescriptor { name, kind })
}

fn implementation_name(args: Node<'_>, head: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = args.walk();
    let keywords = args
        .named_children(&mut cursor)
        .find(|child| child.kind() == "keywords");
    let implementation_type = keywords.and_then(|keywords| {
        let mut cursor = keywords.walk();
        keywords.named_children(&mut cursor).find_map(|pair| {
            let key = pair
                .child_by_field_name("key")?
                .utf8_text(source.as_bytes())
                .ok()?;
            (key.trim() == "for:")
                .then(|| pair.child_by_field_name("value"))
                .flatten()
        })
    });
    let implementation_type = implementation_type.map_or(Some("__MODULE__"), |value| {
        value.utf8_text(source.as_bytes()).ok()
    })?;
    Some(format!(
        "{} for {implementation_type}",
        head.utf8_text(source.as_bytes()).ok()?
    ))
}

pub(super) fn visibility(node: Node<'_>, source: &str) -> &'static str {
    if matches!(target(node, source), Some("defp" | "defmacrop")) {
        "private"
    } else {
        "public"
    }
}

pub(super) fn arity(node: Node<'_>) -> Option<usize> {
    let mut cursor = node.walk();
    let args = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments")?;
    let mut head = args.named_child(0)?;
    if head.kind() == "binary_operator" {
        head = head.child_by_field_name("left")?;
    }
    if head.kind() == "identifier" {
        return Some(0);
    }
    let mut cursor = head.walk();
    Some(
        head.named_children(&mut cursor)
            .find(|child| child.kind() == "arguments")
            .map_or(0, |args| args.named_child_count()),
    )
}

pub(super) fn attribute_start<'a>(node: Node<'a>, source: &str) -> Node<'a> {
    let mut start = node;
    while let Some(previous) = start.prev_named_sibling() {
        let text = previous.utf8_text(source.as_bytes()).unwrap_or_default();
        if ![
            "@doc ",
            "@doc\n",
            "@doc false",
            "@spec ",
            "@impl ",
            "@deprecated ",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            break;
        }
        if !source[previous.end_byte()..start.start_byte()]
            .trim()
            .is_empty()
        {
            break;
        }
        start = previous;
    }
    start
}

/// Elixir symbols are scoped by module and function/macro arity.
pub(super) fn graph_name(name: &str, parent: Option<&str>, arity: Option<usize>) -> String {
    let qualified = if let Some(absolute) = name.strip_prefix("Elixir.") {
        absolute.to_string()
    } else if let Some(parent) = parent.filter(|parent| !parent.is_empty()) {
        format!("{parent}.{name}")
    } else {
        name.to_string()
    };
    match arity {
        Some(arity) => format!("{qualified}/{arity}"),
        None => qualified,
    }
}

/// A damaged body does not erase a structurally clean module declaration head.
/// This is traversal context only; the damaged container is not emitted.
pub(super) fn module_scope(node: Node<'_>, source: &str) -> Option<SymbolDescriptor> {
    if node.kind() != "call" {
        return None;
    }
    let mut cursor = node.walk();
    let arguments = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments")?;
    if arguments.has_error() {
        return None;
    }
    let descriptor = elixir(node, source)?;
    matches!(descriptor.kind, "module" | "impl").then_some(descriptor)
}
