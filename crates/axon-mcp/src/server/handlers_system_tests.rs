use super::super::server_authz::mcp_action_names;
use super::{AxonMcpServer, help_payload};
use crate::schema::PruneMcpRequest;
use crate::server::system_requests::{ResetMcpRequest, ResetSubaction};
use axon_core::config::Config;
use axon_services::transport;
use std::collections::BTreeSet;

#[test]
fn help_payload_lists_every_supported_action() {
    let payload = help_payload();
    let help_actions = payload
        .pointer("/actions")
        .and_then(serde_json::Value::as_object)
        .expect("help payload should expose actions");
    let help_actions = help_actions
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let supported_actions = mcp_action_names().into_iter().collect::<BTreeSet<_>>();

    assert_eq!(help_actions, supported_actions);
}

#[test]
fn help_payload_subactions_match_live_contract() {
    let payload = help_payload();

    let assert_subactions = |action: &str, expected: &[&str]| {
        let actual = payload
            .pointer(&format!("/actions/{action}"))
            .and_then(serde_json::Value::as_array)
            .expect("action subactions should be an array")
            .iter()
            .map(|value| value.as_str().expect("subaction should be a string"))
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected.iter().copied().collect());
    };

    assert_subactions(
        "jobs",
        &[
            "list", "get", "status", "events", "stream", "cancel", "retry", "recover", "cleanup",
            "clear",
        ],
    );
    assert_subactions("prune", &["plan", "get", "exec"]);
    assert_subactions("reset", &["plan", "get", "exec"]);
    assert_subactions("collections", &["list", "get"]);
    assert_subactions("extract", &["start"]);
    assert_subactions(
        "watch",
        &[
            "create", "list", "get", "status", "exec", "history", "update", "pause", "resume",
            "delete",
        ],
    );
}

#[test]
fn sources_domain_path_uses_export_pagination_cap() {
    let pagination = transport::domain_sources_pagination(Some(10_000), Some(0));

    assert_eq!(pagination.limit, transport::DOMAIN_SOURCES_PAGE_MAX);
    assert_eq!(pagination.offset, 0);
}

#[tokio::test]
async fn removed_prune_subactions_fail_before_service_initialization() {
    let server = AxonMcpServer::new(Config::default());
    let error = server
        .handle_prune(PruneMcpRequest {
            subaction: Some("dedupe".to_string()),
            target: Some("collection:axon".to_string()),
            ..PruneMcpRequest::default()
        })
        .await
        .expect_err("removed scoped dedupe must fail closed");
    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(error.message.contains("unknown prune subaction 'dedupe'"));
    assert!(server.service_context.get().is_none());
}

#[tokio::test]
async fn reset_exec_requires_confirmation_and_plan_id_before_io() {
    let server = AxonMcpServer::new(Config::default());
    let error = server
        .handle_reset(ResetMcpRequest {
            subaction: Some(ResetSubaction::Exec),
            confirm: Some(false),
            ..ResetMcpRequest::default()
        })
        .await
        .expect_err("unconfirmed reset must fail closed");
    assert!(error.message.contains("confirm=true"));

    let error = server
        .handle_reset(ResetMcpRequest {
            subaction: Some(ResetSubaction::Exec),
            confirm: Some(true),
            ..ResetMcpRequest::default()
        })
        .await
        .expect_err("reset without a reviewed plan must fail closed");
    assert!(error.message.contains("plan_id"));
}
