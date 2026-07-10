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
fn extracts_simple_rust_and_python_symbol_facts() {
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub struct Parser;\nfn parse_one() {}\nclass PyThing:\n    def run(self):\n        pass\n",
    ));

    let names: Vec<_> = facts.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(names, vec!["Parser", "parse_one", "PyThing", "run"]);
    assert!(facts.iter().all(|fact| fact.fact_kind == "code_symbol"));
    assert_eq!(facts[0].value["symbol_kind"], "struct");
    assert_eq!(facts[1].value["language"], "rust");
    assert_eq!(facts[2].value["language"], "python");
    assert_eq!(facts[3].range.as_ref().unwrap().line_start, Some(4));
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
fn every_fact_is_stamped_as_a_disclosed_regex_fallback() {
    let facts = symbol_facts(&input("src/lib.rs", "pub fn run() {}\n"));

    assert_eq!(facts[0].parser_method, "regex_fallback");
    assert!(
        facts[0].confidence < 0.75,
        "fallback facts must carry confidence < 0.75 per parsing-contract.md"
    );
    assert_eq!(
        facts[0].value["symbol_extraction_status"],
        "heuristic_fallback"
    );
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
    let facts = symbol_facts(&input(
        "src/lib.rs",
        "pub fn run() {\n    let x = 1;\n    println!(\"{x}\");\n}\n",
    ));

    let range = facts[0].range.as_ref().expect("range");
    assert_eq!(range.line_start, Some(1));
    assert_eq!(range.line_end, Some(4));
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
