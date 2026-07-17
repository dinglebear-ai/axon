use axon_api::source::*;
use uuid::Uuid;

use crate::code::{symbol_facts, symbol_facts_with_graph};
use crate::parser::ParseInput;

fn input(path: &str, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(1)),
        stage_id: StageId::new(Uuid::from_u128(2)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_code"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: ContentKind::Code,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_simple_rust_symbol_facts() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub struct Parser;\nfn parse_one() {}\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["Parser", "parse_one"]);
    assert!(facts.iter().all(|fact| fact.fact_kind == "code_symbol"));
    assert_eq!(facts[0].value["symbol_kind"], "struct");
    assert_eq!(facts[1].value["language"], "rust");
}

#[test]
fn emits_graph_candidates_for_code_symbols_without_breaking_fact_api() {
    let input = input("src/lib.rs", "pub enum Mode {}\nasync fn run() {}\n");
    let fact_only = symbol_facts(&input);
    let (facts, candidates) = symbol_facts_with_graph(&input);

    assert_eq!(facts, fact_only);
    assert_eq!(candidates.len(), facts.len());
    assert!(candidates.iter().all(|candidate| {
        candidate.kind == "code_symbol"
            && !candidate.nodes.is_empty()
            && !candidate.evidence.is_empty()
    }));
    assert_eq!(
        candidates[0].producer.parser.as_deref(),
        Some("code_symbols")
    );
    assert_eq!(
        candidates[0].evidence[0].quote.as_deref(),
        Some("pub enum Mode {}")
    );
}

#[test]
fn ignores_commented_out_symbols() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "// fn not_real() {}\n# def not_python():\n/* struct NotReal; */\nfn real() {}\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["real"]);
}

#[test]
fn supported_rust_is_tree_sitter_backed() {
    let facts = symbol_facts(&input("src/lib.rs", "pub fn run() {}\n"));

    assert_eq!(facts[0].parser_method, "tree_sitter");
    assert!(facts[0].confidence >= 0.9);
    assert_eq!(facts[0].value["symbol_extraction_status"], "ast");
}

#[test]
fn rust_visibility_is_derived_from_the_pub_keyword() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub struct Public;\nstruct Private;\n",
    ));

    assert_eq!(facts[0].value["symbol_visibility"], "public");
    assert_eq!(facts[1].value["symbol_visibility"], "private");
}

#[test]
fn python_visibility_is_derived_from_a_leading_underscore() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "def public_fn():\n    pass\ndef _private_fn():\n    pass\n",
    ));

    assert_eq!(facts[0].value["symbol_visibility"], "public");
    assert_eq!(facts[1].value["symbol_visibility"], "private");
}

#[test]
fn nested_python_method_records_its_class_as_parent_symbol() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "class Widget:\n    def render(self):\n        pass\n",
    ));

    assert_eq!(facts[0].name, "Widget");
    assert!(facts[0].value["parent_symbol"].is_null());
    assert_eq!(facts[1].name, "render");
    assert_eq!(facts[1].value["parent_symbol"], "Widget");
}

#[test]
fn rust_function_span_covers_its_full_brace_body() {
    let source = "pub fn run() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
    let facts = symbol_facts(&input("src/lib.rs", source));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(4));
    assert_eq!(range.byte_start, Some(0));
    assert_eq!(range.byte_end, Some(source.trim_end().len() as u64));
}

#[test]
fn rust_ast_range_ignores_braces_inside_strings() {
    let source = "fn render() {\n    let close = \"}\";\n    let value = 1;\n}\nfn next() {}\n";
    let facts = symbol_facts(&input("src/lib.rs", source));

    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].range.as_ref().unwrap().line_end, Some(4));
    assert_eq!(
        facts[0].range.as_ref().unwrap().byte_end,
        Some(source.find("\nfn next").unwrap() as u64)
    );
}

#[test]
fn rust_unit_struct_span_is_single_line() {
    let facts = symbol_facts(&input("src/lib.rs", "pub struct Parser;\n"));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(1));
}

#[test]
fn python_function_span_covers_its_indented_body() {
    let facts = symbol_facts(&input(
        "pkg/mod.py",
        "def run():\n    a = 1\n    b = 2\n\nc = 3\n",
    ));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(3));
}

#[test]
fn every_symbol_range_is_ordered_and_therefore_survives_sanitization() {
    let (facts, candidates) = symbol_facts_with_graph(&input(
        "src/lib.rs",
        "pub struct Parser;\nasync fn run() {\n    let _ = 1;\n}\nclass PyThing:\n    def run(self):\n        pass\n",
    ));

    for fact in &facts {
        let range = fact.range.as_ref().expect("every code_symbol has a range");
        assert!(range.line_start.unwrap() <= range.line_end.unwrap());
    }
    assert_eq!(facts.len(), candidates.len());
}

#[test]
fn extracts_typescript_symbol_facts_at_heuristic_parity() {
    let facts = symbol_facts(&input(
        "src/component.tsx",
        "export interface Props {\n  name: string\n}\n\
export type Mode = 'light' | 'dark';\n\
export enum Size { Small, Large }\n\
export class Widget {\n  render() { return null; }\n}\n\
export const useWidget = (name: string) => ({ name });\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Props", "Mode", "Size", "Widget", "render", "useWidget"]
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact.value["language"] == "typescript")
    );
    assert_eq!(facts[0].value["symbol_kind"], "interface");
    assert_eq!(facts[1].value["symbol_kind"], "type");
    assert_eq!(facts[2].value["symbol_kind"], "enum");
    assert_eq!(facts[3].value["symbol_kind"], "class");
    assert_eq!(facts[4].value["symbol_kind"], "method");
    assert_eq!(facts[4].value["parent_symbol"], "Widget");
    assert_eq!(facts[5].value["symbol_kind"], "function");
    assert_eq!(facts[5].value["symbol_visibility"], "public");
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
}

#[test]
fn extracts_javascript_function_class_and_assignment_symbols() {
    let facts = symbol_facts(&input(
        "src/widget.js",
        "export default function createWidget() {\n  return {}\n}\n\
class LocalWidget {}\n\
const helper = function () { return 1; }\n\
let arrow = () => 2;\n\
var value = 3;\n",
    ));

    let pairs: Vec<_> = facts
        .iter()
        .map(|fact| {
            (
                fact.name.as_str(),
                fact.value["symbol_kind"].as_str().unwrap(),
                fact.value["symbol_visibility"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("createWidget", "function", "public"),
            ("LocalWidget", "class", "private"),
            ("helper", "function", "private"),
            ("arrow", "function", "private"),
            ("value", "constant", "private"),
        ]
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact.value["language"] == "javascript")
    );
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
}

#[test]
fn supported_python_is_tree_sitter_backed_with_nested_parent_ranges() {
    let source = "class Widget:\n    def render(self):\n        return 1\n";
    let facts = symbol_facts(&input("pkg/widget.py", source));

    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| fact.parser_method == "tree_sitter"));
    assert_eq!(facts[1].value["parent_symbol"], "Widget");
    let method = facts[1].range.as_ref().unwrap();
    assert_eq!(method.line_start, Some(2));
    assert_eq!(method.line_end, Some(3));
    assert_eq!(method.byte_start, Some(18));
    assert_eq!(method.byte_end, Some(source.trim_end().len() as u64));
}

#[test]
fn malformed_supported_source_discloses_regex_fallback() {
    let facts = symbol_facts(&input("src/lib.rs", "fn fallback() {\n"));

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].parser_method, "regex_fallback");
    assert_eq!(
        facts[0].value["symbol_extraction_status"],
        "heuristic_fallback"
    );
}

#[test]
fn unsupported_language_keeps_honest_regex_fallback() {
    let facts = symbol_facts(&input("src/main.rb", "class Widget\nend\n"));

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].parser_method, "regex_fallback");
    assert!(facts[0].confidence < 0.75);
}
