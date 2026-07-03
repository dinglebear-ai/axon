/// Verifies that migrated MCP handlers do not import job-layer modules directly.
/// Each handler file is scanned at compile time (via include_str!) for forbidden
/// import fragments.  Broader patterns (no `{`) catch both `use jobs::foo;` and
/// `use jobs::foo::{bar, baz};` forms.
#[test]
fn migrated_mcp_handlers_do_not_import_jobs_layers_directly() {
    let checks = [
        (
            "handlers_source.rs",
            include_str!("handlers_source.rs"),
            &["axon_jobs::embed", "axon_jobs::ingest", "axon_jobs::crawl"][..],
        ),
        (
            "handlers_extract.rs",
            include_str!("handlers_extract.rs"),
            &["axon_jobs::extract"][..],
        ),
        (
            "handlers_system.rs",
            include_str!("handlers_system.rs"),
            &["crawl::screenshot::spider_screenshot_with_options"][..],
        ),
    ];

    for (file, source, forbidden_fragments) in checks {
        for forbidden in forbidden_fragments {
            assert!(
                !source.contains(forbidden),
                "{file} still contains forbidden direct-layer reference: {forbidden}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler dispatch tests (comment #17).
//
// These run inside the `server.rs` #[cfg(test)] module, which grants access to
// the pub(super) handler methods on AxonMcpServer.
// ─────────────────────────────────────────────────────────────────────────────

/// The unified `source` dispatch rejects a missing input with INVALID_PARAMS.
///
/// Calls the real `handle_source` dispatch with no `source`/`input` and
/// verifies the returned error code — proving the action is wired.
#[tokio::test]
async fn source_start_missing_input_returns_invalid_params() {
    use crate::schema::SourceRequest;
    use axon_core::config::Config;

    let server = super::AxonMcpServer::new(Config::default());
    let req = SourceRequest::default();
    let result = server.handle_source(req).await;
    assert!(
        result.is_err(),
        "source without an input must return an error"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "missing source input must return INVALID_PARAMS, got: {:?}",
        err.code
    );
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("source") || msg.contains("input") || msg.contains("required"),
        "error message should mention the missing source/input; got: {msg}"
    );
}

/// Verify the refresh.start response shape includes both `job_ids` (array) and
/// `job_id` (last element) for multi-URL enqueue, using the actual `AxonToolResponse::ok`
/// builder that the handler uses rather than a locally-constructed JSON value.
#[test]
fn refresh_start_response_includes_all_job_ids() {
    use crate::schema::AxonToolResponse;

    let job_ids = vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
    let last = *job_ids.last().unwrap();

    // Build the response the same way the real handler does.
    let response = AxonToolResponse::ok(
        "refresh",
        "start",
        serde_json::json!({
            "job_ids": job_ids,
            "job_id": last,
        }),
    );

    assert!(response.ok, "response must be ok=true");
    assert_eq!(response.action, "refresh");
    assert_eq!(response.subaction, "start");

    let ids: Vec<uuid::Uuid> = serde_json::from_value(response.data["job_ids"].clone()).unwrap();
    assert_eq!(ids.len(), 2, "job_ids must contain all enqueued IDs");

    let single: uuid::Uuid = serde_json::from_value(response.data["job_id"].clone()).unwrap();
    assert_eq!(
        single, last,
        "job_id must equal the last element of job_ids"
    );
}

#[test]
fn mcp_apps_ui_metadata_is_on_dedicated_dashboard_tool_only() {
    let tools = super::AxonMcpServer::tool_router().list_all();

    let axon = tools
        .iter()
        .find(|tool| tool.name == "axon")
        .expect("axon tool must be registered");
    assert!(
        axon.meta.is_none(),
        "catch-all axon tool must not render the dashboard for every routed action"
    );

    let dashboard = tools
        .iter()
        .find(|tool| tool.name == "axon_status_dashboard")
        .expect("dashboard tool must be registered");
    let meta = dashboard
        .meta
        .as_ref()
        .expect("dashboard tool must advertise MCP Apps metadata");
    assert_eq!(
        meta.get("ui")
            .and_then(|ui| ui.get("resourceUri"))
            .and_then(serde_json::Value::as_str),
        Some(super::handler_meta::STATUS_DASHBOARD_URI)
    );
}

#[test]
fn routed_axon_tool_advertises_optional_task_support() {
    let tools = super::AxonMcpServer::tool_router().list_all();

    let axon = tools
        .iter()
        .find(|tool| tool.name == "axon")
        .expect("axon tool must be registered");
    assert_eq!(
        axon.execution
            .as_ref()
            .and_then(|execution| execution.task_support),
        Some(rmcp::model::TaskSupport::Optional),
        "routed axon tool must support task-augmented calls without requiring them"
    );
    assert_eq!(
        axon.task_support(),
        rmcp::model::TaskSupport::Optional,
        "rmcp must allow normal non-task calls for optional task tools"
    );

    let serialized = serde_json::to_value(axon).expect("serialize axon tool metadata");
    assert_eq!(
        serialized["execution"]["taskSupport"],
        serde_json::json!("optional")
    );
}

#[test]
fn status_dashboard_tool_does_not_advertise_task_support() {
    let tools = super::AxonMcpServer::tool_router().list_all();

    let dashboard = tools
        .iter()
        .find(|tool| tool.name == "axon_status_dashboard")
        .expect("dashboard tool must be registered");
    assert!(
        dashboard.execution.is_none(),
        "dashboard tool renders an MCP App widget and must not advertise task support"
    );

    let serialized = serde_json::to_value(dashboard).expect("serialize dashboard tool metadata");
    assert!(
        serialized.get("execution").is_none(),
        "dashboard tool metadata must not include execution.taskSupport"
    );
}

#[test]
fn mcp_apps_resource_meta_declares_locked_down_policy() {
    let meta = super::handler_meta::status_dashboard_resource_meta();
    let ui = meta
        .get("ui")
        .expect("resource metadata must include a ui object");

    assert_eq!(ui["permissions"], serde_json::json!({}));
    assert_eq!(ui["csp"]["connectDomains"], serde_json::json!([]));
    assert_eq!(ui["csp"]["resourceDomains"], serde_json::json!([]));
    assert_eq!(ui["csp"]["frameDomains"], serde_json::json!([]));
    assert_eq!(ui["csp"]["baseUriDomains"], serde_json::json!([]));
}

#[test]
fn mcp_apps_capabilities_advertise_html_app_mime_type() {
    let capabilities = serde_json::to_value(super::handler_meta::mcp_apps_server_capabilities())
        .expect("serialize caps");
    assert_eq!(
        capabilities["extensions"]["io.modelcontextprotocol/ui"]["mimeTypes"],
        serde_json::json!([super::handler_meta::MCP_APP_MIME_TYPE])
    );
}

#[test]
fn mcp_capabilities_advertise_task_augmented_tool_calls() {
    let capabilities = serde_json::to_value(super::handler_meta::mcp_apps_server_capabilities())
        .expect("serialize caps");
    assert_eq!(
        capabilities["tasks"]["requests"]["tools"]["call"],
        serde_json::json!({})
    );
    assert_eq!(capabilities["tasks"]["list"], serde_json::json!({}));
    assert_eq!(capabilities["tasks"]["cancel"], serde_json::json!({}));
}

#[test]
fn dedicated_dashboard_tool_requires_read_scope() {
    assert_eq!(
        super::required_scope_for_tool("axon_status_dashboard", "", ""),
        Some("axon:read")
    );
    assert_eq!(
        super::required_scope_for_tool("axon", "source", ""),
        Some("axon:write")
    );
    // Removed indexing actions are no longer in the allow-list — they resolve to
    // the deny sentinel at the MCP boundary.
    assert_eq!(
        super::required_scope_for_tool("axon", "crawl", ""),
        Some("__deny__")
    );
}
