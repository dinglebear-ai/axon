use axon_api::source::{GraphCandidate, SourceParseFacts, SourceRange};
use serde_json::json;
use tree_sitter::{Language, Node, Parser};

use crate::facts::{inline_text, source_fact_ranged};
use crate::graph_candidate::graph_candidate_ranged;
use crate::parser::ParseInput;

use super::AST_PARSER_METHOD;

mod dialects;

#[derive(Clone, Copy)]
enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Bash,
    Css,
    Elixir,
}

impl CodeLanguage {
    fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Css => "css",
            Self::Elixir => "elixir",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript | Self::Tsx => "typescript",
        }
    }

    fn grammar_name(self) -> &'static str {
        if matches!(self, Self::Tsx) {
            "tsx"
        } else {
            self.name()
        }
    }

    fn grammar(self) -> Language {
        match self {
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
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
    recovered: bool,
    arity: Option<usize>,
}

pub(super) struct AstParse {
    pub symbols: Vec<AstSymbol>,
    pub grammar: &'static str,
    pub recovered: bool,
}

pub(super) fn parse_symbols(input: &ParseInput) -> Result<AstParse, &'static str> {
    let language = detect_language(input).ok_or("unsupported")?;
    let source = inline_text(input);
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar())
        .map_err(|_| "failed")?;
    let tree = parser.parse(source, None).ok_or("failed")?;

    let mut symbols = Vec::new();
    let offsets = CharOffsets::new(source);
    collect_symbols(
        tree.root_node(),
        source,
        language,
        None,
        &mut symbols,
        &offsets,
    );
    for symbol in &mut symbols {
        symbol.recovered = tree.root_node().has_error();
    }
    Ok(AstParse {
        symbols,
        grammar: language.grammar_name(),
        recovered: tree.root_node().has_error(),
    })
}

pub(super) fn facts_with_graph(
    input: &ParseInput,
    symbols: Vec<AstSymbol>,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::with_capacity(symbols.len());
    let mut candidates = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let graph_name = if symbol.language == "elixir" {
            dialects::graph_name(&symbol.name, symbol.parent_symbol.as_deref(), symbol.arity)
        } else {
            symbol.name.clone()
        };
        facts.push(source_fact_ranged(
            input,
            "code_symbols",
            AST_PARSER_METHOD,
            "code_symbol",
            symbol.name.clone(),
            json!({
                "language": symbol.language,
                "symbol_arity": symbol.arity,
                "symbol_kind": symbol.kind,
                "symbol_visibility": symbol.visibility,
                "parent_symbol": symbol.parent_symbol,
                "symbol_extraction_status": "ast",
                "code_symbol_range_truncated": false,
                "code_parse_status": if symbol.recovered { "partial" } else { "parsed" },
                "code_syntax_recovered": symbol.recovered,
            }),
            Some(symbol.range.clone()),
        ));
        let mut candidate = graph_candidate_ranged(
            input,
            "code_symbols",
            "code_symbol",
            &graph_name,
            Some(symbol.range),
            Some(symbol.quote),
        );
        if symbol.language == "elixir" {
            let properties = &mut candidate.nodes[1].properties;
            properties.insert("symbol_name".into(), json!(symbol.name));
            properties.insert("symbol_arity".into(), json!(symbol.arity));
            properties.insert("symbol_kind".into(), json!(symbol.kind));
            properties.insert("parent_symbol".into(), json!(symbol.parent_symbol));
        }
        candidates.push(candidate);
    }
    (facts, candidates)
}

fn collect_symbols(
    node: Node<'_>,
    source: &str,
    language: CodeLanguage,
    parent_symbol: Option<&str>,
    output: &mut Vec<AstSymbol>,
    offsets: &CharOffsets<'_>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }
    let descriptor = (!node.has_error() && !declaration_node(node).has_error())
        .then(|| symbol_descriptor(node, source, language))
        .flatten();
    let recovered_scope = (matches!(language, CodeLanguage::Elixir) && descriptor.is_none())
        .then(|| dialects::module_scope(node, source))
        .flatten();
    let next_parent = descriptor
        .as_ref()
        .or(recovered_scope.as_ref())
        .map(|descriptor| {
            if matches!(language, CodeLanguage::Elixir) {
                if matches!(descriptor.kind, "module" | "impl") {
                    dialects::graph_name(&descriptor.name, parent_symbol, None)
                } else {
                    parent_symbol.unwrap_or_default().to_string()
                }
            } else {
                descriptor.name.clone()
            }
        })
        .filter(|parent| !parent.is_empty())
        .or_else(|| parent_symbol.map(str::to_string));

    if let Some(descriptor) = descriptor {
        let range_node = declaration_node(node);
        let mut range = node_range(range_node, offsets);
        let start_node = if matches!(language, CodeLanguage::Elixir) {
            dialects::attribute_start(range_node, source)
        } else {
            range_node
        };
        range.byte_start = Some(start_node.start_byte() as u64);
        range.char_start = Some(offsets.at(start_node.start_byte()));
        range.line_start = Some(start_node.start_position().row as u32 + 1);
        let quote = source[start_node.start_byte()..range_node.end_byte()].to_string();
        output.push(AstSymbol {
            name: descriptor.name,
            kind: descriptor.kind,
            language: language.name(),
            visibility: visibility(node, source, language),
            parent_symbol: parent_symbol.map(str::to_string),
            range,
            quote,
            recovered: false,
            arity: if matches!(language, CodeLanguage::Elixir)
                && matches!(descriptor.kind, "function" | "macro")
            {
                dialects::arity(node)
            } else {
                None
            },
        });
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_symbols(
            child,
            source,
            language,
            next_parent.as_deref(),
            output,
            offsets,
        );
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
    if matches!(language, CodeLanguage::Css | CodeLanguage::Elixir) {
        return dialects::descriptor(node, source, language);
    }
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
        CodeLanguage::Bash | CodeLanguage::Css => "public",
        CodeLanguage::Elixir => dialects::visibility(node, source),
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

/// Sparse byte checkpoints bound each symbol's character-coordinate scan.
struct CharOffsets<'a> {
    source: &'a str,
    checkpoints: Vec<(usize, u64)>,
}

impl<'a> CharOffsets<'a> {
    fn new(source: &'a str) -> Self {
        let mut checkpoints = vec![(0, 0)];
        for (chars, (byte, _)) in source.char_indices().enumerate() {
            if byte - checkpoints.last().unwrap().0 >= 256 {
                checkpoints.push((byte, chars as u64));
            }
        }
        Self {
            source,
            checkpoints,
        }
    }

    fn at(&self, byte: usize) -> u64 {
        let index = self
            .checkpoints
            .partition_point(|&(offset, _)| offset <= byte)
            - 1;
        let (start, chars) = self.checkpoints[index];
        chars + self.source[start..byte].chars().count() as u64
    }
}

fn node_range(node: Node<'_>, offsets: &CharOffsets<'_>) -> SourceRange {
    let start = node.start_byte();
    let end = node.end_byte();
    SourceRange {
        line_start: Some(node.start_position().row as u32 + 1),
        line_end: Some(node.end_position().row as u32 + 1),
        byte_start: Some(start as u64),
        byte_end: Some(end as u64),
        char_start: Some(offsets.at(start)),
        char_end: Some(offsets.at(end)),
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
    let shebang = inline_text(input).lines().next().unwrap_or_default();
    if matches!(hint.as_str(), "zsh" | "scss" | "sass" | "heex")
        || path.ends_with(".zsh")
        || (shebang.starts_with("#!") && shebang.contains("zsh"))
        || path.ends_with(".scss")
        || path.ends_with(".sass")
        || path.ends_with(".heex")
    {
        return None;
    }
    if hint == "bash"
        || hint == "shell"
        || hint == "sh"
        || path.ends_with(".sh")
        || path.ends_with(".bash")
    {
        Some(CodeLanguage::Bash)
    } else if hint == "css" || path.ends_with(".css") {
        Some(CodeLanguage::Css)
    } else if hint == "elixir" || path.ends_with(".ex") || path.ends_with(".exs") {
        Some(CodeLanguage::Elixir)
    } else if hint == "rust" || path.ends_with(".rs") {
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

#[cfg(test)]
#[path = "ast_tests.rs"]
mod tests;
