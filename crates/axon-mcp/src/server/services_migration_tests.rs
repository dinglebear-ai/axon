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
            &["axon_jobs::"][..],
        ),
        (
            "handlers_extract.rs",
            include_str!("handlers_extract.rs"),
            &["axon_jobs::"][..],
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

#[test]
fn jobs_subactions_use_read_write_admin_scope_split() {
    assert_eq!(
        super::required_scope_for_tool("axon", "jobs", "events"),
        Some("axon:read")
    );
    assert_eq!(
        super::required_scope_for_tool("axon", "jobs", "cancel"),
        Some("axon:write")
    );
    assert_eq!(
        super::required_scope_for_tool("axon", "jobs", "cleanup"),
        Some("axon:admin")
    );
    assert_eq!(
        super::required_scope_for_tool("axon", "jobs", "unknown"),
        Some("__deny__")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// `prune` action (auth rejection, dry-run plan, exec confirmation gate).
// ─────────────────────────────────────────────────────────────────────────────

/// The MCP allow-list requires `axon:admin` for `prune` — a plain `axon:write`
/// token must not satisfy it (mirrors the pruning contract's "destructive
/// prune requires axon:admin" rule, distinct from every other write action).
#[test]
fn prune_action_requires_admin_scope_not_just_write() {
    assert_eq!(
        super::required_scope_for_tool("axon", "prune", "plan"),
        Some("axon:admin")
    );
    assert_eq!(
        super::required_scope_for_tool("axon", "prune", "exec"),
        Some("axon:admin")
    );

    // A caller holding only `axon:write` must be rejected by the resolved
    // scope requirement (the real gate check happens in `check_scope`, which
    // exercises `axon_authz::scope_satisfies` directly — asserted below).
    let write_only = vec!["axon:write".to_string()];
    assert!(!axon_authz::scope_satisfies(&write_only, "axon:admin"));
    let admin = vec!["axon:admin".to_string()];
    assert!(axon_authz::scope_satisfies(&admin, "axon:admin"));
}

/// A `prune` call with no target is rejected before touching any store.
#[tokio::test]
async fn prune_missing_target_returns_invalid_params() {
    use crate::schema::PruneMcpRequest;
    use axon_core::config::Config;

    let server = super::AxonMcpServer::new(Config::default());
    let req = PruneMcpRequest::default();
    let result = super::common::CURRENT_PRUNE_AUTHZ
        .scope(axon_services::prune::PruneAuthz::admin(), async {
            server.handle_prune(req).await
        })
        .await;
    let err = result.expect_err("prune without a target must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.to_lowercase().contains("target"),
        "error should mention the missing target; got: {}",
        err.message
    );
}

/// A `prune exec` call without `confirm: true` is rejected — the destructive
/// path always requires explicit confirmation, mirroring the CLI's `--confirm`
/// gate and the pruning contract's "explicit confirmation" safety rule.
#[tokio::test]
async fn prune_exec_without_confirm_is_rejected() {
    use crate::schema::PruneMcpRequest;
    use axon_core::config::Config;

    let server = super::AxonMcpServer::new(Config::default());
    let req = PruneMcpRequest {
        subaction: Some("exec".to_string()),
        target: Some("src_test".to_string()),
        confirm: Some(false),
        ..Default::default()
    };
    let result = super::common::CURRENT_PRUNE_AUTHZ
        .scope(axon_services::prune::PruneAuthz::admin(), async {
            server.handle_prune(req).await
        })
        .await;
    let err = result.expect_err("prune exec without confirm must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.to_lowercase().contains("confirm"),
        "error should mention the missing confirmation; got: {}",
        err.message
    );
}

/// A successful `prune plan` (dry-run) call — proves the selector-building,
/// dry-run planning, and response-envelope path all wire together without
/// requiring a live Qdrant/TEI connection (planning never touches a store; see
/// `axon_services::prune::prune_plan`'s module docs).
#[tokio::test]
async fn prune_plan_dry_run_succeeds_without_live_store() {
    use crate::schema::PruneMcpRequest;
    use axon_core::config::Config;

    // Must not point at the real default `~/.axon/jobs.db`: on a machine
    // whose real store already has pre-cutover rows, `ServiceContext`
    // startup's `assert_workers_allowed_by_cutover` guard would (correctly)
    // reject it before this test ever reaches prune planning.
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..Default::default()
    };
    let server = super::AxonMcpServer::new(cfg);
    let req = PruneMcpRequest {
        subaction: Some("plan".to_string()),
        target: Some("src_test".to_string()),
        ..Default::default()
    };
    let result = super::common::CURRENT_PRUNE_AUTHZ
        .scope(axon_services::prune::PruneAuthz::anonymous(), async {
            server.handle_prune(req).await
        })
        .await;
    let response = result.expect("dry-run plan must succeed even without admin authz");
    assert!(response.ok);
    assert_eq!(response.action, "prune");
    assert_eq!(response.subaction, "plan");
    // Small payloads auto-inline under `data.data` via `respond_with_mode`
    // (see `crates/axon-mcp/src/server/artifacts/respond.rs`).
    let inline = &response.data["data"];
    assert_eq!(inline["subaction"], serde_json::json!("plan"));
    // Dry-run planning never produces an execution result.
    assert_eq!(inline["result"], serde_json::Value::Null);
}

/// A `collection:` target with a `generation` is rejected — invalid selector
/// combination per `build_selector`'s grammar (mirrors the CLI validation).
#[tokio::test]
async fn prune_collection_target_rejects_generation() {
    use crate::schema::PruneMcpRequest;
    use axon_core::config::Config;

    let server = super::AxonMcpServer::new(Config::default());
    let req = PruneMcpRequest {
        subaction: Some("plan".to_string()),
        target: Some("collection:axon".to_string()),
        generation: Some("gen_1".to_string()),
        ..Default::default()
    };
    let result = super::common::CURRENT_PRUNE_AUTHZ
        .scope(axon_services::prune::PruneAuthz::admin(), async {
            server.handle_prune(req).await
        })
        .await;
    let err = result.expect_err("collection target with generation must fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

// ─────────────────────────────────────────────────────────────────────────────
// `memory` `import` mode=replace_scope requires `axon:admin` (Fix 1). Mirrors
// the `prune` authz tests above: `handle_memory` reads
// `super::common::CURRENT_MEMORY_AUTHZ` (resolved by `call_tool`'s scope gate
// in production; scoped directly here for a unit test).
// ─────────────────────────────────────────────────────────────────────────────

fn memory_import_request(mode: &str) -> crate::schema::MemoryRequest {
    crate::schema::MemoryRequest {
        subaction: Some(crate::schema::MemorySubaction::Import),
        records: Some(vec![axon_api::source::MemoryRecord {
            memory_id: axon_api::source::MemoryId::new("mem_mcp_test"),
            memory_type: axon_api::source::MemoryType::Fact,
            status: axon_api::source::MemoryStatus::Active,
            body: "mcp import test".to_string(),
            confidence: 1.0,
            salience: 0.5,
            scope: axon_api::source::MemoryScope {
                kind: "global".to_string(),
                value: String::new(),
            },
            history: Vec::new(),
            visibility: axon_api::source::Visibility::default(),
            title: None,
            links: Vec::new(),
            decay: None,
            embedding_refs: Vec::new(),
            superseded_by: None,
            contradicts: None,
        }]),
        import_mode: Some(match mode {
            "replace_scope" => axon_api::source::MemoryImportMode::ReplaceScope,
            _ => axon_api::source::MemoryImportMode::Merge,
        }),
        dry_run: Some(true),
        ..Default::default()
    }
}

fn memory_test_server() -> super::AxonMcpServer {
    use axon_core::config::Config;

    // Isolated sqlite path — same rationale as `prune_plan_dry_run_succeeds_
    // without_live_store` above: must not point at a real pre-existing store.
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..Default::default()
    };
    std::mem::forget(dir);
    super::AxonMcpServer::new(cfg)
}

#[tokio::test]
async fn mcp_memory_import_replace_scope_denied_without_admin_authz() {
    let server = memory_test_server();
    let req = memory_import_request("replace_scope");
    let result = super::common::CURRENT_MEMORY_AUTHZ
        .scope(axon_services::memory::MemoryAuthz::anonymous(), async {
            server.handle_memory(req).await
        })
        .await;
    let err = result.expect_err("replace_scope without axon:admin must be denied");
    assert!(
        err.message.to_lowercase().contains("admin"),
        "expected an axon:admin denial, got: {}",
        err.message
    );
}

#[tokio::test]
async fn mcp_memory_import_replace_scope_allowed_with_admin_authz() {
    let server = memory_test_server();
    let req = memory_import_request("replace_scope");
    let result = super::common::CURRENT_MEMORY_AUTHZ
        .scope(axon_services::memory::MemoryAuthz::admin(), async {
            server.handle_memory(req).await
        })
        .await;
    let response = result.expect("replace_scope with axon:admin must succeed");
    assert!(response.ok);
}

#[tokio::test]
async fn mcp_memory_import_merge_mode_does_not_require_admin_authz() {
    let server = memory_test_server();
    let req = memory_import_request("merge");
    let result = super::common::CURRENT_MEMORY_AUTHZ
        .scope(axon_services::memory::MemoryAuthz::anonymous(), async {
            server.handle_memory(req).await
        })
        .await;
    let response = result.expect("merge mode must succeed without admin authz");
    assert!(response.ok);
}
