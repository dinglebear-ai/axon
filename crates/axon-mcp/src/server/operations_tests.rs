use super::*;
use crate::server::{AxonMcpServer, projection};
use axon_core::config::{Config, McpToolProjection};
use rmcp::ServerHandler;

#[test]
fn complete_leaf_inventory_matches_typed_branches_in_both_directions() {
    let root = tool_schema::canonical_request_schema();
    let mut expected = BTreeSet::new();
    for branch in root["oneOf"].as_array().unwrap() {
        let action = branch["properties"]["action"]["const"].as_str().unwrap();
        let selectors = operation_schema::subactions(&root, branch);
        if selectors.is_empty() {
            expected.insert(action.to_string());
        }
        for selector in selectors {
            expected.insert(format!("{action}_{selector}"));
        }
    }
    let actual: BTreeSet<_> = operation_registry()
        .iter()
        .map(|op| op.name.clone())
        .collect();
    assert_eq!(actual.len(), operation_registry().len());
    assert_eq!(actual, expected);
    for required in [
        "query",
        "source",
        "ask",
        "jobs_get",
        "jobs_cancel",
        "uploads_create",
        "watch_history",
        "prune_get",
        "reset_get",
    ] {
        assert!(actual.contains(required), "missing {required}");
    }
    assert!(!actual.iter().any(|name| name.starts_with("axon_")));
}

#[test]
fn all_modes_preserve_auxiliary_tool_and_exact_inventory() {
    let auxiliary = AxonMcpServer::tool_router()
        .get("axon_status_dashboard")
        .unwrap()
        .clone();
    for mode in [
        McpToolProjection::Legacy,
        McpToolProjection::Atomic,
        McpToolProjection::Both,
    ] {
        let server = AxonMcpServer::new(Config {
            mcp_tool_projection: mode,
            ..Config::default()
        });
        assert_eq!(
            server.get_tool("axon_status_dashboard"),
            Some(auxiliary.clone())
        );
        assert_eq!(
            server.get_tool("axon").is_some(),
            mode != McpToolProjection::Atomic
        );
        let mut expected = BTreeSet::from(["axon_status_dashboard".to_string()]);
        if mode != McpToolProjection::Atomic {
            expected.insert("axon".to_string());
        }
        if mode != McpToolProjection::Legacy {
            expected.extend(operation_registry().iter().map(|op| op.name.clone()));
        }
        let actual: BTreeSet<_> = server
            .projected_router
            .list_all()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
#[should_panic(expected = "collides")]
fn collision_with_dedicated_tool_fails_construction() {
    let mut router = AxonMcpServer::tool_router();
    let mut dedicated = router.map.get("axon_status_dashboard").unwrap().clone();
    dedicated.attr.name = "query".into();
    router.add_route(dedicated);
    projection::compose_router(McpToolProjection::Both, router);
}

#[test]
fn leaf_safety_and_tasks_are_distinct_from_scope() {
    let get = operation_named("jobs_get").unwrap();
    let cancel = operation_named("jobs_cancel").unwrap();
    assert!(get.read_only);
    assert!(!cancel.read_only);
    assert!(cancel.destructive);
    assert_eq!(get.required_scope, Some("axon:read"));
    assert_eq!(cancel.required_scope, Some("axon:write"));
    let plan = operation_named("reset_plan").unwrap();
    assert!(!plan.read_only && !plan.destructive);
    assert!(operation_named("reset_exec").unwrap().destructive);
    assert!(operation_named("reset_get").unwrap().read_only);
    assert_eq!(
        operation_named("reset_get").unwrap().required_scope,
        Some("axon:admin")
    );
    for name in ["search", "research"] {
        let op = operation_named(name).unwrap();
        assert!(!op.read_only);
        assert_eq!(op.required_scope, Some("axon:write"));
    }
    for op in operation_registry() {
        assert_eq!(op.supports_tasks, op.name == "extract_start");
    }
}

#[test]
fn help_and_resources_publish_complete_canonical_inventory_in_every_mode() {
    for mode in [
        McpToolProjection::Legacy,
        McpToolProjection::Atomic,
        McpToolProjection::Both,
    ] {
        let server = AxonMcpServer::new(Config {
            mcp_tool_projection: mode,
            ..Config::default()
        });
        let payload = crate::server::handler_meta::operation_payload(&server);
        assert_eq!(payload["projection"], mode.to_string());
        let published: BTreeSet<_> = payload["active_tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_string())
            .collect();
        let actual: BTreeSet<_> = server
            .projected_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        assert_eq!(published, actual);
        let descriptors = payload["operations"].as_array().unwrap();
        assert_eq!(descriptors.len(), operation_registry().len());
        for descriptor in descriptors {
            jsonschema::validator_for(&descriptor["inputSchema"]).unwrap();
        }
        let instructions = server.get_info().instructions.unwrap();
        match mode {
            McpToolProjection::Legacy => {
                assert!(instructions.contains("Use axon with action/subaction"))
            }
            McpToolProjection::Atomic => {
                assert!(instructions.contains("aggregate axon tool is unavailable"))
            }
            McpToolProjection::Both => {
                assert!(instructions.contains("Both axon action/subaction calls"))
            }
        }
    }
}
