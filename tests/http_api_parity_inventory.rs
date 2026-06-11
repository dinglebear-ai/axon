use axon::services::client_contract::rest_route_contracts;
use axon::services::types::supported_routes;
use std::collections::BTreeSet;

const DOC: &str = include_str!("../docs/reference/api-parity.md");

fn row_for_cli(command: &str) -> Option<&'static str> {
    let needle = format!("| `{command}` |");
    DOC.lines().find(|line| line.starts_with(&needle))
}

#[test]
fn parity_doc_covers_every_cli_command_kind() {
    let commands = [
        "scrape",
        "crawl",
        "watch",
        "map",
        "extract",
        "search",
        "embed",
        "debug",
        "doctor",
        "query",
        "retrieve",
        "ask",
        "evaluate",
        "train",
        "suggest",
        "sources",
        "domains",
        "stats",
        "status",
        "dedupe",
        "ingest",
        "memory",
        "sessions",
        "research",
        "screenshot",
        "completions",
        "mcp",
        "serve",
        "setup",
        "migrate",
    ];

    for command in commands {
        assert!(
            row_for_cli(command).is_some(),
            "docs/reference/api-parity.md is missing CLI command row `{command}`"
        );
    }
}

#[test]
fn parity_doc_lists_all_advertised_http_routes() {
    for route in supported_routes() {
        let needle = format!("`{route}`");
        assert!(
            DOC.contains(&needle) || DOC.contains(&route),
            "docs/reference/api-parity.md does not mention advertised HTTP route `{route}`"
        );
    }
}

#[test]
fn parity_doc_marks_representative_current_http_statuses() {
    let ask = row_for_cli("ask").expect("ask row");
    assert!(ask.contains("`POST /v1/ask`"), "{ask}");
    assert!(ask.contains("Implemented"), "{ask}");

    let status = row_for_cli("status").expect("status row");
    assert!(status.contains("`GET /v1/status`"), "{status}");
    assert!(status.contains("Implemented"), "{status}");

    let query = row_for_cli("query").expect("query row");
    assert!(query.contains("`POST /v1/query`"), "{query}");
    assert!(query.contains("Implemented"), "{query}");

    let retrieve = row_for_cli("retrieve").expect("retrieve row");
    assert!(retrieve.contains("`POST /v1/retrieve`"), "{retrieve}");
    assert!(retrieve.contains("Implemented"), "{retrieve}");

    let completions = row_for_cli("completions").expect("completions row");
    assert!(completions.contains("Deferred"), "{completions}");

    assert!(
        DOC.contains("`POST /v1/actions` action-envelope endpoint is removed"),
        "docs/reference/api-parity.md should state that /v1/actions is removed"
    );
}

#[test]
fn rest_route_contracts_match_openapi_request_schemas() {
    let openapi = axon::web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");
    let components = openapi.components.expect("OpenAPI components");
    let schemas = components.schemas;

    for contract in rest_route_contracts() {
        let path_item = paths
            .get(contract.path)
            .unwrap_or_else(|| panic!("OpenAPI is missing path `{}`", contract.path));
        assert!(
            path_item
                .get(contract.method.to_ascii_lowercase())
                .is_some(),
            "OpenAPI path {} is missing method {}",
            contract.path,
            contract.method
        );
        let operation = path_item
            .get(contract.method.to_ascii_lowercase())
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("OpenAPI operation {} {}", contract.method, contract.path));
        let request_ref = operation
            .get("requestBody")
            .and_then(|value| value.get("content"))
            .and_then(|value| value.get("application/json"))
            .and_then(|value| value.get("schema"))
            .and_then(|value| value.get("$ref"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "OpenAPI operation {} {} does not reference a JSON request schema",
                    contract.method, contract.path
                )
            });
        assert_eq!(
            request_ref,
            format!("#/components/schemas/{}", contract.schema_name),
            "OpenAPI operation {} {} is wired to the wrong request schema",
            contract.method,
            contract.path
        );
        let schema = schemas
            .get(contract.schema_name)
            .unwrap_or_else(|| panic!("OpenAPI is missing schema `{}`", contract.schema_name));
        let schema_json = serde_json::to_value(schema)
            .unwrap_or_else(|err| panic!("serialize schema {}: {err}", contract.schema_name));
        let properties = schema_json
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("schema {} has no properties", contract.schema_name));
        let actual = properties
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let expected = contract.fields.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "OpenAPI schema {} drifted from canonical REST route contract for {} {}",
            contract.schema_name, contract.method, contract.path
        );
    }
}

#[test]
fn openapi_job_list_pagination_uses_query_parameters() {
    let openapi = axon::web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");

    for path in ["/v1/crawl", "/v1/embed", "/v1/extract", "/v1/ingest"] {
        let parameters = paths
            .get(path)
            .and_then(|path_item| path_item.get("get"))
            .and_then(|operation| operation.get("parameters"))
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("OpenAPI operation GET {path} has no parameters"));
        for name in ["limit", "offset"] {
            let parameter = parameters
                .iter()
                .find(|parameter| {
                    parameter.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
                .unwrap_or_else(|| panic!("OpenAPI operation GET {path} is missing `{name}`"));
            assert_eq!(
                parameter.get("in").and_then(serde_json::Value::as_str),
                Some("query"),
                "OpenAPI operation GET {path} must expose `{name}` as a query parameter"
            );
            assert_eq!(
                parameter
                    .get("required")
                    .and_then(serde_json::Value::as_bool),
                Some(false),
                "OpenAPI operation GET {path} must not require optional pagination parameter `{name}`"
            );
        }
    }
}
