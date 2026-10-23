use axon_api::source::*;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::schema::api_schema_facts;

fn input(path: &str, kind: ContentKind, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(41)),
        stage_id: StageId::new(Uuid::from_u128(42)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_schema"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: kind,
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
fn extracts_openapi_graphql_and_proto_schema_facts() {
    let (openapi_facts, openapi_candidates) = api_schema_facts(&input(
        "openapi.yaml",
        ContentKind::Yaml,
        "openapi: 3.1.0\npaths:\n  /v1/ask:\n    post:\n      operationId: ask\n",
    ));
    assert_eq!(openapi_facts[0].fact_kind, "api_endpoint");
    assert_eq!(openapi_facts[0].name, "POST /v1/ask");
    assert_eq!(openapi_candidates[0].kind, "api_endpoint");

    let (graphql_facts, graphql_candidates) = api_schema_facts(&input(
        "schema.graphql",
        ContentKind::PlainText,
        "type Query {\n  ask(question: String!): Answer\n}\n",
    ));
    assert_eq!(graphql_facts[0].fact_kind, "graphql_type");
    assert_eq!(graphql_facts[1].name, "Query.ask");
    assert_eq!(graphql_candidates.len(), graphql_facts.len());
    assert_eq!(graphql_candidates[0].kind, "graphql_type");
    assert_eq!(graphql_candidates[1].kind, "graphql_field");

    let (proto_facts, proto_candidates) = api_schema_facts(&input(
        "service.proto",
        ContentKind::PlainText,
        "service Axon { rpc Ask (AskRequest) returns (AskReply); }\nmessage AskRequest {}\n",
    ));
    assert_eq!(proto_facts[0].fact_kind, "proto_service");
    assert_eq!(proto_facts[1].value["request"], "AskRequest");
    assert_eq!(proto_facts[2].fact_kind, "proto_message");
    assert_eq!(proto_candidates.len(), proto_facts.len());
    assert_eq!(proto_candidates[0].kind, "proto_service");
    assert_eq!(proto_candidates[1].kind, "proto_rpc");
    assert_eq!(proto_candidates[2].kind, "proto_message");
}

#[test]
fn extracts_openapi_operations_schemas_and_auth_requirements() {
    let text = concat!(
        "openapi: 3.1.0\n",
        "paths:\n",
        "  /v1/ask:\n",
        "    post:\n",
        "      operationId: ask\n",
        "      security:\n",
        "        - bearerAuth: []\n",
        "components:\n",
        "  schemas:\n",
        "    AskRequest:\n",
        "      type: object\n",
        "  securitySchemes:\n",
        "    bearerAuth:\n",
        "      type: http\n",
    );
    let (facts, candidates) = api_schema_facts(&input("openapi.yaml", ContentKind::Yaml, text));

    let by_kind =
        |kind: &str| -> Vec<_> { facts.iter().filter(|fact| fact.fact_kind == kind).collect() };

    let endpoints = by_kind("api_endpoint");
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].name, "POST /v1/ask");

    let operations = by_kind("api_operation");
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].name, "POST /v1/ask #ask");
    assert_eq!(operations[0].value["operation_id"], "ask");

    let schemas = by_kind("api_schema");
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].name, "AskRequest");

    let auth = by_kind("api_auth_requirement");
    let auth_names: Vec<_> = auth.iter().map(|fact| fact.name.as_str()).collect();
    assert_eq!(auth_names, vec!["bearerAuth", "bearerAuth"]);

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "api_operation")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "api_schema")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.kind == "api_auth_requirement")
    );
}

#[test]
fn json_catalog_words_do_not_manufacture_proto_or_graphql_facts() {
    let body = serde_json::json!({"icons": (0..600).map(|i| format!("customer service Catalog{i} message Label{i} type Query {{ field: String }}")).collect::<Vec<_>>()}).to_string();
    let (facts, candidates) = api_schema_facts(&input("catalog.json", ContentKind::Json, &body));
    assert!(facts.is_empty());
    assert!(candidates.is_empty());
    let (facts, candidates) = api_schema_facts(&input(
        "catalog.yaml",
        ContentKind::Yaml,
        "description: customer service Catalog message Label type Query",
    ));
    assert!(facts.is_empty());
    assert!(candidates.is_empty());
}

#[test]
fn legitimate_minified_proto_keeps_every_fact_without_repeating_the_full_line() {
    let mut body = String::from("service Actual { rpc Ask (Request) returns (Reply); } ");
    for i in 0..600 {
        body.push_str(&format!("message Item{i} {{ string value = 1; }} "));
    }
    let (facts, candidates) =
        api_schema_facts(&input("service.proto", ContentKind::PlainText, &body));
    assert_eq!(facts.len(), 602);
    assert_eq!(candidates.len(), facts.len());
    let total_quote: usize = candidates
        .iter()
        .flat_map(|c| &c.evidence)
        .map(|e| {
            let quote = e.quote.as_deref().unwrap();
            assert!(quote.len() <= 4096);
            assert!(body.contains(quote));
            quote.len()
        })
        .sum();
    assert!(total_quote < body.len());
    assert!(facts.iter().any(|f| f.name == "Item599"));
}

#[test]
#[ignore = "requires AXON_REAL_CATALOG_FIXTURE pinned private input"]
fn real_core_catalog_does_not_amplify_graph_evidence() {
    let path = std::env::var("AXON_REAL_CATALOG_FIXTURE").expect("explicit fixture path required");
    let body = std::fs::read_to_string(path).unwrap();
    assert!(body.len() > 4_000_000);
    let (facts, candidates) = api_schema_facts(&input("catalog.json", ContentKind::Json, &body));
    assert!(facts.is_empty());
    assert!(candidates.is_empty());
}

#[test]
fn explicit_schema_mime_preserves_extensionless_graphql_and_proto_support() {
    for (mime, body, fact) in [
        (
            "application/graphql",
            "type Person {\n name: String\n}\n",
            "graphql_type",
        ),
        (
            "application/protobuf",
            "syntax = \"proto3\";\nservice Actual {}",
            "proto_service",
        ),
    ] {
        let mut source = input("schema", ContentKind::PlainText, body);
        source.document.mime_type = Some(mime.into());
        let (facts, _) = api_schema_facts(&source);
        assert!(facts.iter().any(|f| f.fact_kind == fact));
    }
}

#[test]
fn graph_quote_excerpt_preserves_named_fact_and_literal_provenance() {
    let name = "ActualNamedEntity";
    let body = format!("{}{}{}", "α".repeat(20_000), name, "β".repeat(20_000));
    let source = input("schema.proto", ContentKind::PlainText, &body);
    let candidate = crate::graph_candidate::graph_candidate(
        &source,
        "proto_schema",
        "proto_service",
        name,
        Some(1),
        Some(body.clone()),
    );
    assert_eq!(candidate.nodes[1].label, name);
    let evidence = &candidate.evidence[0];
    let quote = evidence.quote.as_deref().unwrap();
    assert!(quote.len() <= 4096 && quote.contains(name) && body.contains(quote));
    assert_eq!(
        evidence.metadata["quote_excerpted"],
        serde_json::json!(true)
    );
    assert_eq!(
        evidence.metadata["quote_original_bytes"],
        serde_json::json!(body.len())
    );
    assert_eq!(evidence.range.as_ref().unwrap().line_start, Some(1));
}
