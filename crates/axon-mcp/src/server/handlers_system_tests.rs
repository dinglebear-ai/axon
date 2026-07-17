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

    assert_eq!(
        payload.pointer("/actions/jobs").unwrap(),
        &serde_json::json!([
            "list", "get", "status", "events", "stream", "cancel", "retry", "recover", "cleanup",
            "clear"
        ])
    );
    assert_eq!(
        payload.pointer("/actions/prune").unwrap(),
        &serde_json::json!(["plan", "exec"])
    );
    assert_eq!(
        payload.pointer("/actions/reset").unwrap(),
        &serde_json::json!(["plan", "exec"])
    );
    assert_eq!(
        payload.pointer("/actions/collections").unwrap(),
        &serde_json::json!(["list", "get"])
    );
    assert_eq!(
        payload.pointer("/actions/extract").unwrap(),
        &serde_json::json!(["start"])
    );
    assert_eq!(
        payload.pointer("/actions/watch").unwrap(),
        &serde_json::json!([
            "create", "list", "get", "status", "exec", "history", "update", "pause", "resume",
            "delete"
        ])
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
    assert!(error.message.contains("expected plan|exec"));
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
