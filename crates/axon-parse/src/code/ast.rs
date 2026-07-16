use axon_api::source::{GraphCandidate, SourceParseFacts, SourceRange};
use serde_json::json;
use tree_sitter::{Language, Node, Parser};

use crate::facts::{inline_text, source_fact_ranged};
use crate::graph_candidate::graph_candidate_ranged;
use crate::parser::ParseInput;

use super::AST_PARSER_METHOD;

#[derive(Clone, Copy)]
enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
}

impl CodeLanguage {
    fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript | Self::Tsx => "typescript",
        }
    }

    fn grammar(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }
}

pub(super) struct AstSymbol {
    name: String,
    kind: &'static str,
    language: &'static str,
    visibility: &'static str,
    parent_symbol: Option<String>,
    range: SourceRange,
    quote: String,
}

pub(super) fn parse_symbols(input: &ParseInput) -> Result<Vec<AstSymbol>, ()> {
    let language = detect_language(input).ok_or(())?;
    let source = inline_text(input);
    let mut parser = Parser::new();
    parser.set_language(&language.grammar()).map_err(|_| ())?;
    let tree = parser.parse(source, None).ok_or(())?;
    if tree.root_node().has_error() {
        return Err(());
    }

    let mut symbols = Vec::new();
    collect_symbols(tree.root_node(), source, language, None, &mut symbols);
    Ok(symbols)
}

pub(super) fn facts_with_graph(
    input: &ParseInput,
    symbols: Vec<AstSymbol>,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::with_capacity(symbols.len());
    let mut candidates = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        facts.push(source_fact_ranged(
            input,
            "code_symbols",
            AST_PARSER_METHOD,
            "code_symbol",
            symbol.name.clone(),
            json!({
                "language": symbol.language,
                "symbol_kind": symbol.kind,
                "symbol_visibility": symbol.visibility,
                "parent_symbol": symbol.parent_symbol,
                "symbol_extraction_status": "ast",
                "code_symbol_range_truncated": false,
            }),
            Some(symbol.range.clone()),
        ));
        candidates.push(graph_candidate_ranged(
            input,
            "code_symbols",
            "code_symbol",
            &symbol.name,
            Some(symbol.range),
            Some(symbol.quote),
        ));
    }
    (facts, candidates)
}

fn collect_symbols(
    node: Node<'_>,
    source: &str,
    language: CodeLanguage,
    parent_symbol: Option<&str>,
    output: &mut Vec<AstSymbol>,
) {
    let descriptor = symbol_descriptor(node, source, language);
    let next_parent = descriptor
        .as_ref()
        .map(|descriptor| descriptor.name.clone())
        .or_else(|| parent_symbol.map(str::to_string));

    if let Some(descriptor) = descriptor {
        let range_node = declaration_node(node);
        let range = node_range(range_node, source);
        let quote = source[range_node.start_byte()..range_node.end_byte()].to_string();
        output.push(AstSymbol {
            name: descriptor.name,
            kind: descriptor.kind,
            language: language.name(),
            visibility: visibility(node, source, language),
            parent_symbol: parent_symbol.map(str::to_string),
            range,
            quote,
        });
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_symbols(child, source, language, next_parent.as_deref(), output);
    }
}

struct SymbolDescriptor {
    name: String,
    kind: &'static str,
}

fn symbol_descriptor(
    node: Node<'_>,
    source: &str,
    language: CodeLanguage,
) -> Option<SymbolDescriptor> {
    let kind = match node.kind() {
        "function_item" | "function_definition" | "function_declaration" => "function",
        "method_definition" => "method",
        "struct_item" => "struct",
        "enum_item" | "enum_declaration" => "enum",
        "trait_item" => "trait",
        "impl_item" => "impl",
        "mod_item" => "module",
        "class_definition" | "class_declaration" | "abstract_class_declaration" => "class",
        "interface_declaration" => "interface",
        "type_item" | "type_alias_declaration" => "type",
        "const_item" | "static_item" => "constant",
        "variable_declarator" if js_ts_function_or_constant(node, language) => {
            variable_kind(node, source)
        }
        _ => return None,
    };
    let name_node = if node.kind() == "impl_item" {
        node.child_by_field_name("type")
    } else {
        node.child_by_field_name("name")
    }?;
    let name = name_node
        .utf8_text(source.as_bytes())
        .ok()?
        .trim()
        .to_string();
    (!name.is_empty()).then_some(SymbolDescriptor { name, kind })
}

fn js_ts_function_or_constant(node: Node<'_>, language: CodeLanguage) -> bool {
    matches!(
        language,
        CodeLanguage::JavaScript | CodeLanguage::TypeScript | CodeLanguage::Tsx
    ) && node.child_by_field_name("name").is_some()
}

fn variable_kind(node: Node<'_>, source: &str) -> &'static str {
    let Some(value) = node.child_by_field_name("value") else {
        return "constant";
    };
    match value.kind() {
        "arrow_function" | "function_expression" | "generator_function" => "function",
        _ if value
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.contains("React.FC")) =>
        {
            "function"
        }
        _ => "constant",
    }
}

fn visibility(node: Node<'_>, source: &str, language: CodeLanguage) -> &'static str {
    match language {
        CodeLanguage::Rust => {
            let mut cursor = node.walk();
            if node
                .children(&mut cursor)
                .any(|child| child.kind() == "visibility_modifier")
            {
                "public"
            } else {
                "private"
            }
        }
        CodeLanguage::Python => node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source.as_bytes()).ok())
            .map_or("public", |name| {
                if name.starts_with('_') {
                    "private"
                } else {
                    "public"
                }
            }),
        CodeLanguage::JavaScript | CodeLanguage::TypeScript | CodeLanguage::Tsx => {
            if ancestors(node).any(|ancestor| ancestor.kind() == "export_statement") {
                "public"
            } else {
                "private"
            }
        }
    }
}

fn ancestors(mut node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    std::iter::from_fn(move || {
        node = node.parent()?;
        Some(node)
    })
}

fn declaration_node(mut node: Node<'_>) -> Node<'_> {
    if node.kind() == "variable_declarator"
        && let Some(parent) = node.parent()
        && matches!(
            parent.kind(),
            "lexical_declaration" | "variable_declaration"
        )
    {
        node = parent;
    }
    if let Some(parent) = node.parent()
        && matches!(parent.kind(), "decorated_definition" | "export_statement")
    {
        return parent;
    }
    node
}

fn node_range(node: Node<'_>, source: &str) -> SourceRange {
    let start = node.start_byte();
    let end = node.end_byte();
    SourceRange {
        line_start: Some(node.start_position().row as u32 + 1),
        line_end: Some(node.end_position().row as u32 + 1),
        byte_start: Some(start as u64),
        byte_end: Some(end as u64),
        char_start: Some(source[..start].chars().count() as u64),
        char_end: Some(source[..end].chars().count() as u64),
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

fn detect_language(input: &ParseInput) -> Option<CodeLanguage> {
    let hint = input
        .document
        .language
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = input
        .document
        .path
        .as_deref()
        .unwrap_or(input.document.canonical_uri.as_str())
        .to_ascii_lowercase();
    if hint == "rust" || path.ends_with(".rs") {
        Some(CodeLanguage::Rust)
    } else if hint == "python" || path.ends_with(".py") || path.ends_with(".pyi") {
        Some(CodeLanguage::Python)
    } else if hint == "tsx" || path.ends_with(".tsx") {
        Some(CodeLanguage::Tsx)
    } else if hint == "typescript"
        || hint == "ts"
        || path.ends_with(".ts")
        || path.ends_with(".mts")
        || path.ends_with(".cts")
    {
        Some(CodeLanguage::TypeScript)
    } else if hint == "javascript"
        || hint == "js"
        || hint == "jsx"
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
    {
        Some(CodeLanguage::JavaScript)
    } else {
        None
    }
}
