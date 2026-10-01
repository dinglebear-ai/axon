use crate::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use axon_api::source::*;

fn request(path: &str, text: &str) -> PrepareSourceDocumentRequest {
    PrepareSourceDocumentRequest {
        document: SourceDocument {
            document_id: DocumentId::from("observed-doc"),
            source_id: SourceId::from("observed-src"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: "file:///observed".into(),
            content_kind: ContentKind::Code,
            content: ContentRef::InlineText { text: text.into() },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.into()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: vec![],
            parser_hints: vec![],
        },
        generation: SourceGenerationId::from("observed-gen"),
        profile: None,
        parse_facts: vec![],
        graph_candidates: vec![],
        warnings: vec![],
        errors: vec![],
    }
}

#[test]
fn zero_symbol_ast_stays_successful_and_uses_tree_sitter_module_chunks() {
    let text = "// This module intentionally contains no declaration symbols and keeps complete useful source context.\n";
    let (result, observed) = crate::testing::preparer_for_small_fixtures()
        .prepare_observed(request("module.rs", text))
        .unwrap();
    assert_eq!(observed.outcome_label(), "ast_zero_symbol");
    assert!(!observed.heuristic_fallback());
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("module must remain searchable")
    };
    assert_eq!(prepared.chunking_method, "tree_sitter");
    assert_eq!(
        prepared.metadata.get("code_ast_status"),
        Some(&serde_json::json!("parsed"))
    );
    assert_eq!(
        prepared.metadata.get("code_symbol_count"),
        Some(&serde_json::json!(0))
    );
    assert!(prepared.chunks.iter().all(
        |chunk| chunk.metadata.get("parser_method") == Some(&serde_json::json!("tree_sitter"))
    ));
}

#[test]
fn short_ast_document_retains_observation_after_quality_skip() {
    let (result, observed) = crate::testing::preparer_for_small_fixtures()
        .prepare_observed(request("module.rs", "// short"))
        .unwrap();
    assert!(matches!(result, PrepareSourceDocumentResult::Skipped(_)));
    assert_eq!(observed.outcome_label(), "ast_zero_symbol");
}

#[test]
fn unsupported_grammar_is_distinct_from_failed_ast_and_reports_heuristic_use() {
    let mut input = request(
        "module.swift",
        "func searchable_function_name() { print(\"complete useful source context for searchable indexing\") }",
    );
    input
        .document
        .metadata
        .insert("code_grammar".into(), serde_json::json!("stale-rust"));
    let (result, observed) = crate::testing::preparer_for_small_fixtures()
        .prepare_observed(input)
        .unwrap();
    assert_eq!(observed.outcome_label(), "unsupported_grammar");
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("fallback must remain searchable")
    };
    assert_eq!(
        prepared.metadata.get("code_ast_status"),
        Some(&serde_json::json!("unsupported"))
    );
    assert_ne!(prepared.chunking_method, "tree_sitter");
    assert!(
        prepared.metadata.get("code_grammar").is_none(),
        "unsupported grammar must omit null and stale grammar metadata"
    );
}

#[test]
fn non_code_preparation_clears_stale_or_caller_supplied_ast_metadata() {
    let mut input = request(
        "README.md",
        "# Context\n\nThis Markdown article provides useful preparation context without any code parser observation.",
    );
    input.document.content_kind = ContentKind::Markdown;
    for (key, value) in [
        ("code_ast_status", serde_json::json!("parsed")),
        ("code_grammar", serde_json::json!("rust")),
        ("code_symbol_count", serde_json::json!(99)),
        ("symbol_extraction_status", serde_json::json!("ast")),
    ] {
        input.document.metadata.insert(key.into(), value);
    }
    let (result, observed) = crate::testing::preparer_for_small_fixtures()
        .prepare_observed(input)
        .unwrap();
    assert_eq!(observed.outcome_label(), "not_observed");
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("article must remain searchable")
    };
    for key in [
        "code_ast_status",
        "code_grammar",
        "code_symbol_count",
        "symbol_extraction_status",
    ] {
        assert!(
            prepared.metadata.get(key).is_none(),
            "stale document field {key}"
        );
        assert!(
            prepared
                .chunks
                .iter()
                .all(|chunk| chunk.metadata.get(key).is_none()),
            "stale chunk field {key}"
        );
    }
}

#[test]
fn malicious_supplied_outcome_is_not_observed_or_published_as_ast_metadata() {
    for (status, grammar) in [
        ("injected status contents", "rust"),
        ("parsed", "injected grammar contents"),
    ] {
        let mut input = request(
            "module.rs",
            "// This complete useful source context must not claim an unverified AST parser outcome.\n",
        );
        input.parse_facts.push(SourceParseFacts {
            document_id: input.document.document_id.clone(), source_item_key: input.document.source_item_key.clone(),
            fact_kind: "code_parse_outcome".into(), name: "document".into(),
            value: serde_json::json!({"code_ast_status":status, "code_grammar":grammar, "code_symbol_count":0, "symbol_extraction_status":"ast"}),
            parser_id: "code_symbols".into(), parser_version: "supplied".into(), parser_method: "tree_sitter".into(),
            range: None, confidence: 1.0, metadata: MetadataMap::new(),
        });
        let (result, observed) = crate::testing::preparer_for_small_fixtures()
            .prepare_observed(input)
            .unwrap();
        assert_eq!(observed.outcome_label(), "not_observed");
        assert_eq!(
            (
                observed.ast_status(),
                observed.grammar(),
                observed.symbol_count()
            ),
            (None, None, None)
        );
        let PrepareSourceDocumentResult::Prepared(prepared) = result else {
            panic!("source context must remain searchable")
        };
        assert_ne!(
            prepared.chunking_method, "tree_sitter",
            "invalid supplied claim must not select AST module chunking"
        );
        for key in [
            "code_ast_status",
            "code_grammar",
            "code_symbol_count",
            "symbol_extraction_status",
        ] {
            assert!(
                prepared.metadata.get(key).is_none(),
                "unverified field {key}"
            );
        }
        assert!(
            prepared
                .chunks
                .iter()
                .all(|chunk| !chunk.metadata.0.contains_key("code_ast_status"))
        );
    }
}
