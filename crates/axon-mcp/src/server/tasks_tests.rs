use super::*;
use crate::schema::{HelpRequest, parse_axon_request};

#[test]
fn unsupported_task_request_names_immediate_actions() {
    let err = unsupported_task_request(&AxonRequest::Help(HelpRequest {
        response_mode: None,
    }));
    assert!(
        err.message.contains("help"),
        "unexpected error: {}",
        err.message
    );
    assert!(
        err.message.contains("extract.start"),
        "error should name the supported task start: {}",
        err.message
    );
}

#[test]
fn task_mode_removed_crawl_fails_before_task_dispatch() {
    let raw = serde_json::json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://example.com/one"]
    })
    .as_object()
    .expect("object")
    .clone();

    let err = parse_axon_request(raw).expect_err("removed crawl must not parse");
    assert!(
        err.contains("action `crawl` was removed from MCP") && err.contains("action=source"),
        "removed crawl should fail closed with replacement guidance: {err}"
    );
}

// `tasks/list` and its cursor helper were removed with rmcp 3.x: SEP-2663
// defines no task-enumeration method, so there is no paginated offset left to
// validate. See `tasks.rs` for the surviving get/cancel surface.

// Regression coverage for the cross-transport visibility divergence
// (security finding S-3 / pipeline-unification redaction-contract C1-V01):
// `caller_auth_snapshot_from_auth_context` used to hardcode
// `Visibility::Internal` for every remote MCP caller, even ones holding only
// `axon:read`, where the identical REST caller (`axon-web`'s
// `caller_context_from_auth`) got `Visibility::Public` via
// `axon_authz::VisibilityPolicy`. These tests pin the fixed behavior: the
// ceiling must come from the shared policy, keyed off scopes, not be a
// blanket grant for every authenticated remote caller.
fn auth_context_with_scopes(scopes: &[&str]) -> lab_auth::AuthContext {
    lab_auth::AuthContext {
        sub: "mcp-caller".to_string(),
        actor_key: None,
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        issuer: "test".to_string(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

#[test]
fn caller_auth_snapshot_caps_non_admin_remote_caller_at_public() {
    let auth_ctx = auth_context_with_scopes(&["axon:read"]);
    let snapshot = caller_auth_snapshot_from_auth_context(&auth_ctx);
    assert_eq!(
        snapshot.visibility_ceiling,
        axon_api::source::Visibility::Public,
        "a remote MCP caller holding only axon:read must be capped at Public, \
         matching the identical REST caller — not granted Internal by default"
    );
}

#[test]
fn caller_auth_snapshot_grants_internal_to_admin_scoped_remote_caller() {
    let auth_ctx = auth_context_with_scopes(&["axon:read", "axon:admin"]);
    let snapshot = caller_auth_snapshot_from_auth_context(&auth_ctx);
    assert_eq!(
        snapshot.visibility_ceiling,
        axon_api::source::Visibility::Internal,
        "a remote MCP caller holding axon:admin is still allowed the Internal ceiling"
    );
}

#[test]
fn visibility_policy_grants_internal_to_trusted_local_caller_directly() {
    // `axon-mcp`'s remote-caller construction always sets `trusted_local:
    // false` (there is no per-caller local trust concept over the MCP
    // transport), so exercise the trusted-local branch of the shared policy
    // directly to confirm it still yields Internal — the same policy that
    // backs the CLI/system's trusted-local snapshots.
    let caller = axon_api::source::CallerContext {
        caller_id: Some("local".to_string()),
        transport: axon_api::source::TransportKind::Mcp,
        trusted_local: true,
        scopes: vec![],
        visibility_ceiling: axon_api::source::Visibility::Public,
        auth_mode: axon_api::source::AuthMode::TrustedLocal,
        token_id: None,
        display_name: None,
    };
    assert_eq!(
        axon_authz::VisibilityPolicy::new().ceiling_for(&caller),
        axon_api::source::Visibility::Internal
    );
}
