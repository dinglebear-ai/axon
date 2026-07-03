use axon_services::client_contract::rest_route_contracts;
use axon_services::types::{RestRouteAuth, rest_route_inventory};
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

// NOTE: the old `parity_doc_lists_all_advertised_http_routes`,
// `parity_doc_matches_capabilities_auth_contract`, and
// `parity_doc_marks_representative_current_http_statuses` tests were removed when
// `docs/reference/api-parity.md` became a *generated* `| Operation | CLI | MCP |
// REST |` matrix (see `cargo xtask gen-api-parity`). The retired hand-written doc
// carried per-route auth strings ("axon:read or axon:write"), status words
// ("Implemented"/"Deferred"), and full `METHOD /path` rows — none of which the
// factual matrix encodes. The matrix's correctness is now enforced by
// `cargo xtask check-api-parity` (drift gate against the CLI/MCP/REST surfaces),
// and the OpenAPI↔route-inventory consistency it depends on is covered by the
// tests below. `parity_doc_covers_every_cli_command_kind` still guards CLI rows.

#[test]
fn rest_route_contracts_match_openapi_request_schemas() {
    let openapi = axon_web::openapi_document();
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
fn route_inventory_openapi_operations_are_registered() {
    let openapi = axon_web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");

    for route in rest_route_inventory().iter().filter(|route| route.openapi) {
        let path_item = paths
            .get(route.path)
            .unwrap_or_else(|| panic!("OpenAPI is missing route {}", route.display()));
        let method = route.method.to_ascii_lowercase();
        assert!(
            path_item.get(&method).is_some(),
            "OpenAPI path {} is missing method {}",
            route.path,
            route.method
        );
    }
}

#[test]
fn openapi_security_schemes_and_operation_security_match_inventory() {
    let openapi = axon_web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let schemes = openapi_json
        .pointer("/components/securitySchemes")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI security schemes");
    assert_eq!(schemes["bearerAuth"]["type"], "http");
    assert_eq!(schemes["bearerAuth"]["scheme"], "bearer");
    assert_eq!(schemes["oauth2"]["type"], "oauth2");

    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");
    for route in rest_route_inventory().iter().filter(|route| route.openapi) {
        let operation = paths
            .get(route.path)
            .and_then(|path_item| path_item.get(route.method.to_ascii_lowercase()))
            .unwrap_or_else(|| panic!("OpenAPI operation missing for {}", route.display()));
        match route.auth {
            RestRouteAuth::Public => assert!(
                operation.get("security").is_none(),
                "public operation {} must not require auth",
                route.display()
            ),
            RestRouteAuth::Read | RestRouteAuth::Write => {
                let security = operation
                    .get("security")
                    .and_then(serde_json::Value::as_array)
                    .unwrap_or_else(|| {
                        panic!(
                            "protected operation {} must declare security",
                            route.display()
                        )
                    });
                assert!(
                    security
                        .iter()
                        .any(|entry| entry.get("bearerAuth").is_some()),
                    "{} must allow bearer auth",
                    route.display()
                );
                for expected_scope in ["axon:read", "axon:write"] {
                    assert!(
                        security.iter().any(|entry| {
                            entry
                                .get("oauth2")
                                .and_then(serde_json::Value::as_array)
                                .is_some_and(|scopes| {
                                    scopes
                                        .iter()
                                        .any(|scope| scope.as_str() == Some(expected_scope))
                                })
                        }),
                        "{} must allow OAuth scope {expected_scope}",
                        route.display()
                    );
                }
                for status in ["401", "403"] {
                    let response = operation
                        .pointer(&format!(
                            "/responses/{status}/content/application~1json/schema/$ref"
                        ))
                        .and_then(serde_json::Value::as_str);
                    assert_eq!(
                        response,
                        Some("#/components/schemas/ErrorBody"),
                        "{} must document JSON ErrorBody auth response {status}",
                        route.display()
                    );
                }
            }
        }
    }
}

#[test]
fn openapi_artifact_route_accepts_slash_containing_path_as_query_param() {
    let openapi = axon_web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");

    assert!(
        paths.contains_key("/v1/artifacts"),
        "OpenAPI should advertise the slash-preserving query route"
    );
    assert!(
        !paths.contains_key("/v1/artifacts/{path}"),
        "OpenAPI must not imply artifact paths are a single segment"
    );
    let parameters = paths["/v1/artifacts"]["get"]["parameters"]
        .as_array()
        .expect("artifact parameters");
    let path = parameters
        .iter()
        .find(|parameter| parameter["name"] == "path")
        .expect("path query parameter");
    assert_eq!(path["in"], "query");
    assert_eq!(path["required"], true);
}

#[test]
fn openapi_error_body_kind_uses_error_kind_enum_schema() {
    let openapi = axon_web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    assert_eq!(
        openapi_json.pointer("/components/schemas/ErrorBody/properties/kind/$ref"),
        Some(&serde_json::json!("#/components/schemas/ErrorKind"))
    );
    let error_kind = openapi_json
        .pointer("/components/schemas/ErrorKind/enum")
        .and_then(serde_json::Value::as_array)
        .expect("ErrorKind enum");
    for kind in [
        "bad_request",
        "unauthorized",
        "forbidden",
        "not_found",
        "upstream_unavailable",
        "timeout",
        "vertical_rate_limited",
    ] {
        assert!(
            error_kind.iter().any(|value| value.as_str() == Some(kind)),
            "ErrorKind enum should include {kind}"
        );
    }
}

#[test]
fn openapi_job_list_pagination_uses_query_parameters() {
    let openapi = axon_web::openapi_document();
    let openapi_json = serde_json::to_value(&openapi).expect("serialize OpenAPI document");
    let paths = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("OpenAPI paths");

    for path in ["/v1/extract"] {
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
