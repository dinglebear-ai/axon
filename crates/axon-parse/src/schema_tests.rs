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
