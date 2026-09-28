use super::*;
use axon_api::source::{AuthMode, AuthScope, TransportKind, Visibility};
use axon_core::config::{Config, McpToolProjection};

fn caller(scopes: &[&str]) -> AuthContext {
    AuthContext {
        sub: "owned-test-caller".to_owned(),
        actor_key: None,
        scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
        issuer: "test".to_owned(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

#[test]
fn every_leaf_and_aggregate_call_has_identical_authorization() {
    let server = AxonMcpServer::new(Config {
        mcp_tool_projection: McpToolProjection::Both,
        ..Config::default()
    });
    for operation in operation_registry() {
        for scopes in [
            vec![],
            vec!["axon:read"],
            vec!["axon:write"],
            vec!["axon:admin"],
            vec!["axon:read", "axon:write", "axon:admin"],
        ] {
            let auth = caller(&scopes);
            let mut leaf = CallToolRequestParams::new(operation.name.clone());
            assert!(projection::normalize_projected_tool_call(&server, &mut leaf).unwrap());
            let mut aggregate =
                CallToolRequestParams::new("axon").with_arguments(leaf.arguments.clone().unwrap());
            assert!(projection::normalize_projected_tool_call(&server, &mut aggregate).unwrap());
            let (action, subaction) = identity(&leaf, true);
            assert_eq!(
                identity(&aggregate, true),
                (action.clone(), subaction.clone())
            );
            let legacy_result =
                server_authz::enforce_call_tool_scope(Some(&auth), "axon", &action, &subaction);
            let atomic_result = server_authz::enforce_call_tool_scope(
                Some(&auth),
                &operation.name,
                &action,
                &subaction,
            );
            assert_eq!(
                legacy_result.is_ok(),
                atomic_result.is_ok(),
                "{} {scopes:?}",
                operation.name
            );
        }
    }
}

#[test]
fn remote_context_retains_identity_visibility_and_only_real_admin_authority() {
    for scopes in [vec!["axon:read"], vec!["axon:write"], vec!["axon:admin"]] {
        let auth = caller(&scopes);
        let admin = scopes.contains(&"axon:admin");
        for action in ["prune", "reset", "memory", "query"] {
            let context = ExecutionContext::new(action, Some(&auth));
            assert_eq!(context.prune.is_admin, admin && action == "prune");
            assert_eq!(context.reset.is_admin, admin && action == "reset");
            assert_eq!(context.memory.is_admin, admin && action == "memory");
            assert_eq!(context.codex.actor, auth.sub);
            assert_eq!(context.codex.scopes, scopes.join(" "));
            let snapshot = context.snapshot.unwrap();
            assert_eq!(snapshot.caller_id.as_deref(), Some("owned-test-caller"));
            assert_eq!(snapshot.transport, TransportKind::Mcp);
            assert_eq!(snapshot.auth_mode, AuthMode::Oauth);
            assert_eq!(
                snapshot.visibility_ceiling,
                if admin {
                    Visibility::Internal
                } else {
                    Visibility::Public
                }
            );
            assert!(!snapshot.granted_scopes.contains(&AuthScope::Local));
            assert_eq!(snapshot.granted_scopes.contains(&AuthScope::Admin), admin);
        }
    }
    let mut static_caller = caller(&["axon:read"]);
    static_caller.sub = "static-bearer".to_owned();
    assert_eq!(
        caller_snapshot(Some(&static_caller)).unwrap().auth_mode,
        AuthMode::StaticToken
    );
    assert!(caller_snapshot(None).is_none());
}

#[test]
fn auxiliary_arguments_cannot_forge_canonical_execution_context() {
    let request = CallToolRequestParams::new("axon_status_dashboard").with_arguments(
        serde_json::from_value(serde_json::json!({"action":"reset","subaction":"exec"})).unwrap(),
    );
    assert_eq!(identity(&request, false), (String::new(), String::new()));
    let context = ExecutionContext::new("", None);
    assert!(!context.prune.is_admin && !context.reset.is_admin && !context.memory.is_admin);
}
