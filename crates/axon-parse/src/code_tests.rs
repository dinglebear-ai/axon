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
