use super::client_contract::{
    ClientExtractMode, ClientExtractRequest, ClientRoutePreference, RestExtractRequest,
    RestMemoryRequest, rest_route_contracts,
};
use axon_api::action::{MemoryRequest, MemorySubaction};
use axon_core::config::RenderMode;
use std::collections::BTreeSet;

#[test]
fn extract_request_defaults_to_auto_mode() {
    let req = ClientExtractRequest {
        urls: vec!["https://example.com/docs".to_string()],
        prompt: Some("extract title".to_string()),
        mode: None,
        max_pages: Some(1),
        render_mode: Some(RenderMode::Http),
        embed: Some(false),
        headers: vec![],
        route_preference: ClientRoutePreference::Default,
    };

    assert_eq!(req.effective_mode(), ClientExtractMode::Auto);
}

#[test]
fn rest_extract_request_rejects_unimplemented_modes() {
    let err = serde_json::from_value::<RestExtractRequest>(serde_json::json!({
        "urls": ["https://example.com"],
        "mode": "deterministic"
    }))
    .expect_err("unsupported REST extract mode should not deserialize");

    assert!(err.to_string().contains("unknown variant"));
}

#[test]
fn rest_memory_request_carries_list_status_filter_to_mcp_request() {
    let req: RestMemoryRequest = serde_json::from_value(serde_json::json!({
        "subaction": "list",
        "project": "axon",
        "repo": "jmagar/axon",
        "status": "superseded",
        "limit": 25
    }))
    .expect("deserialize REST memory list request");

    let mcp_req = MemoryRequest::from(req);
    assert!(matches!(mcp_req.subaction, Some(MemorySubaction::List)));
    assert_eq!(mcp_req.project.as_deref(), Some("axon"));
    assert_eq!(mcp_req.repo.as_deref(), Some("jmagar/axon"));
    assert_eq!(mcp_req.status.as_deref(), Some("superseded"));
    assert_eq!(mcp_req.limit, Some(25));
}

#[test]
fn rest_route_contract_fields_match_openapi_schema_properties() {
    let openapi: serde_json::Value =
        serde_json::from_str(include_str!("../../../apps/web/openapi/axon.json"))
            .expect("parse generated OpenAPI");

    for contract in rest_route_contracts() {
        let properties = openapi
            .pointer(&format!(
                "/components/schemas/{}/properties",
                contract.schema_name
            ))
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("OpenAPI schema {} has properties", contract.schema_name));
        let openapi_fields = properties
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let contract_fields = contract.fields.iter().copied().collect::<BTreeSet<_>>();

        assert_eq!(
            contract_fields, openapi_fields,
            "{} {} must list the same fields as OpenAPI schema {}",
            contract.method, contract.path, contract.schema_name
        );
    }
}

#[test]
fn rag_requests_accept_typed_source_scope_and_reject_unknown_kinds() {
    use super::client_contract::{RestAskRequest, RestQueryRequest};
    for kinds in [
        serde_json::json!(["web", "git"]),
        serde_json::json!(["session"]),
        serde_json::json!([]),
    ] {
        let query = serde_json::json!({"query":"Unraid parity", "source_kinds": kinds});
        assert!(
            serde_json::from_value::<RestQueryRequest>(query.clone()).is_ok(),
            "query must support explicit source scope"
        );
        assert!(
            serde_json::from_value::<RestAskRequest>(query).is_ok(),
            "ask must support the same source scope"
        );
    }
    let invalid = serde_json::json!({"query":"Unraid parity", "source_kinds":["invented"]});
    assert!(serde_json::from_value::<RestQueryRequest>(invalid.clone()).is_err());
    assert!(serde_json::from_value::<RestAskRequest>(invalid).is_err());
}

#[test]
fn canonical_source_urls_reach_request_config_without_changing_shared_defaults() {
    use crate::client_contract::RestAskRequest;
    use crate::transport::{AskTransportOverrides, apply_ask_overrides};
    let request: RestAskRequest = serde_json::from_value(serde_json::json!({
        "query": "document question", "source_urls": ["https://example.com/canonical"]
    }))
    .unwrap();
    let base = axon_core::config::Config::default();
    let cfg = apply_ask_overrides(
        &base,
        AskTransportOverrides {
            source_urls: request.source_urls,
            ..Default::default()
        },
    );
    assert_eq!(cfg.retrieval_source_urls, ["https://example.com/canonical"]);
    assert!(base.retrieval_source_urls.is_empty());
    let inherited = apply_ask_overrides(&cfg, AskTransportOverrides::default());
    assert_eq!(inherited.retrieval_source_urls, cfg.retrieval_source_urls);
    let cleared = apply_ask_overrides(
        &cfg,
        AskTransportOverrides {
            source_urls: Some(Vec::new()),
            ..Default::default()
        },
    );
    assert!(cleared.retrieval_source_urls.is_empty());
}
