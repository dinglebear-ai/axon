use std::cell::RefCell;
use std::sync::LazyLock;

use tree_sitter::{Language, Node, Parser};
use tree_sitter_language::LanguageFn;

use super::chunk::SymbolKind;

#[derive(Debug, Clone)]
pub(super) struct SymbolInfo {
    pub(super) byte_start: usize,
    pub(super) byte_end: usize,
    pub(super) start_line: u32,
    pub(super) end_line: u32,
    pub(super) name: Option<String>,
    pub(super) kind: SymbolKind,
}

#[derive(Clone, Copy)]
pub(super) struct LanguageSpec {
    pub(super) grammar: LanguageFn,
    pub(super) extractor: Extractor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Extractor {
    None,
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Bash,
}

pub(super) fn language_for_extension(ext: &str) -> Option<LanguageSpec> {
    match ext {
        "rs" => Some(LanguageSpec {
            grammar: tree_sitter_rust::LANGUAGE,
            extractor: Extractor::Rust,
        }),
        "py" => Some(LanguageSpec {
            grammar: tree_sitter_python::LANGUAGE,
            extractor: Extractor::Python,
        }),
        "js" | "jsx" => Some(LanguageSpec {
            grammar: tree_sitter_javascript::LANGUAGE,
            extractor: Extractor::JavaScript,
        }),
        "ts" => Some(LanguageSpec {
            grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            extractor: Extractor::TypeScript,
        }),
        "tsx" => Some(LanguageSpec {
            grammar: tree_sitter_typescript::LANGUAGE_TSX,
            extractor: Extractor::TypeScript,
        }),
        "go" => Some(LanguageSpec {
            grammar: tree_sitter_go::LANGUAGE,
            extractor: Extractor::Go,
        }),
        "sh" | "bash" => Some(LanguageSpec {
            grammar: tree_sitter_bash::LANGUAGE,
            extractor: Extractor::Bash,
        }),
        _ => None,
    }
}

static RUST_LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_rust::LANGUAGE));
static GO_LANGUAGE: LazyLock<Language> = LazyLock::new(|| Language::from(tree_sitter_go::LANGUAGE));
static PYTHON_LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_python::LANGUAGE));
static JAVASCRIPT_LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_javascript::LANGUAGE));
static TYPESCRIPT_LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT));
static BASH_LANGUAGE: LazyLock<Language> =
    LazyLock::new(|| Language::from(tree_sitter_bash::LANGUAGE));

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

pub(super) fn extract_symbols(content: &str, extractor: Extractor) -> Vec<SymbolInfo> {
    let language = match extractor {
        Extractor::Rust => &*RUST_LANGUAGE,
        Extractor::Go => &*GO_LANGUAGE,
        Extractor::Python => &*PYTHON_LANGUAGE,
        Extractor::JavaScript => &*JAVASCRIPT_LANGUAGE,
        Extractor::TypeScript => &*TYPESCRIPT_LANGUAGE,
        Extractor::Bash => &*BASH_LANGUAGE,
        Extractor::None => return Vec::new(),
    };
    PARSER.with(|slot| {
        let mut parser = slot.borrow_mut();
        parser.reset();
        if parser.set_language(language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(content, None) else {
            return Vec::new();
        };
        let mut symbols = Vec::new();
        collect_symbols(tree.root_node(), content, extractor, &mut symbols);
        symbols.sort_by_key(|sym| (sym.byte_start, sym.byte_end));
        symbols
    })
}

pub(super) fn find_symbol_for_chunk(
    symbols: &[SymbolInfo],
    chunk_start: usize,
    chunk_end: usize,
) -> Option<&SymbolInfo> {
    symbols
        .iter()
        .filter(|sym| sym.byte_start <= chunk_start && sym.byte_end >= chunk_end)
        .min_by_key(|sym| sym.byte_end.saturating_sub(sym.byte_start))
        .or_else(|| {
            symbols
                .iter()
                .find(|sym| sym.byte_start < chunk_end && sym.byte_end > chunk_start)
        })
}

fn collect_symbols(
    node: Node<'_>,
    content: &str,
    extractor: Extractor,
    symbols: &mut Vec<SymbolInfo>,
) {
    if let Some(symbol) = symbol_from_node(node, content, extractor) {
        symbols.push(symbol);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(child, content, extractor, symbols);
    }
}

fn symbol_from_node(node: Node<'_>, content: &str, extractor: Extractor) -> Option<SymbolInfo> {
    match extractor {
        Extractor::Rust => rust_symbol_from_node(node, content),
        Extractor::Go => go_symbol_from_node(node, content),
        Extractor::Python => python_symbol_from_node(node, content),
        Extractor::JavaScript => js_ts_symbol_from_node(node, content),
        Extractor::TypeScript => js_ts_symbol_from_node(node, content),
        Extractor::Bash => bash_symbol_from_node(node, content),
        Extractor::None => None,
    }
}

fn rust_symbol_from_node(node: Node<'_>, content: &str) -> Option<SymbolInfo> {
    let kind = match node.kind() {
        "function_item" => {
            if has_impl_parent(node) {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            }
        }
        "struct_item" => SymbolKind::Struct,
        "enum_item" => SymbolKind::Enum,
        "trait_item" => SymbolKind::Trait,
        "impl_item" => SymbolKind::Impl,
        "const_item" => SymbolKind::Const,
        "static_item" => SymbolKind::Static,
        "type_item" => SymbolKind::Type,
        "mod_item" => SymbolKind::Mod,
        _ => return None,
    };
    let mut name = node_name(node, content);
    if kind == SymbolKind::Method
        && let Some(method) = name.as_deref()
        && let Some(parent) = rust_impl_type_name(node, content)
    {
        name = Some(format!("{parent}::{method}"));
    }
    Some(symbol_info(node, name, kind))
}

fn go_symbol_from_node(node: Node<'_>, content: &str) -> Option<SymbolInfo> {
    let kind = match node.kind() {
        "function_declaration" => SymbolKind::Function,
        "method_declaration" => SymbolKind::Method,
        "const_declaration" => SymbolKind::Const,
        "var_declaration" => SymbolKind::Static,
        "type_declaration" => SymbolKind::Type,
        _ => return None,
    };
    let mut name = node_name(node, content);
    if kind == SymbolKind::Method
        && let Some(method) = name.as_deref()
        && let Some(receiver) = go_receiver_name(node, content)
    {
        name = Some(format!("{receiver}.{method}"));
    }
    Some(symbol_info(node, name, kind))
}

fn python_symbol_from_node(node: Node<'_>, content: &str) -> Option<SymbolInfo> {
    let kind = match node.kind() {
        "function_definition" => {
            if let Some(class_name) = python_class_ancestor_name(node, content) {
                return Some(symbol_info(
                    node,
                    node_name(node, content).map(|name| format!("{class_name}::{name}")),
                    SymbolKind::Method,
                ));
            }
            SymbolKind::Function
        }
        "class_definition" => SymbolKind::Struct,
        _ => return None,
    };
    Some(symbol_info(node, node_name(node, content), kind))
}

fn js_ts_symbol_from_node(node: Node<'_>, content: &str) -> Option<SymbolInfo> {
    let kind = match node.kind() {
        "function_declaration" | "generator_function_declaration" => SymbolKind::Function,
        "class_declaration" => SymbolKind::Struct,
        "interface_declaration" | "type_alias_declaration" => SymbolKind::Type,
        "method_definition" | "method_signature" => {
            return Some(symbol_info(
                node,
                node_name(node, content).map(|name| {
                    match js_ts_class_ancestor_name(node, content) {
                        Some(parent) => format!("{parent}::{name}"),
                        None => name,
                    }
                }),
                SymbolKind::Method,
            ));
        }
        _ => return None,
    };
    Some(symbol_info(node, node_name(node, content), kind))
}

fn bash_symbol_from_node(node: Node<'_>, content: &str) -> Option<SymbolInfo> {
    if node.kind() != "function_definition" {
        return None;
    }
    Some(symbol_info(
        node,
        bash_function_name(node, content),
        SymbolKind::Function,
    ))
}

fn symbol_info(node: Node<'_>, name: Option<String>, kind: SymbolKind) -> SymbolInfo {
    SymbolInfo {
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
        name,
        kind,
    }
}

fn node_name(node: Node<'_>, content: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(content.as_bytes()).ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
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

fn python_class_ancestor_name(mut node: Node<'_>, content: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "class_definition" {
            return node_name(parent, content);
        }
        node = parent;
    }
    None
}

fn js_ts_class_ancestor_name(mut node: Node<'_>, content: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "class_declaration" | "class") {
            return node_name(parent, content);
        }
        node = parent;
    }
    None
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
    node_name(node, content)
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

fn clean_symbol_fragment(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('&')
        .trim_start_matches('*')
        .trim()
        .to_string()
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
