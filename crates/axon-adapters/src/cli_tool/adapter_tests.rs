use std::path::PathBuf;

use axon_api::source::*;

use crate::SourceAdapter;
use crate::cli_tool::CliToolSourceAdapter;
use crate::local_test_support::*;

fn cli_tool_plan(source: &str, scope: SourceScope) -> SourcePlan {
    source_plan_for(
        "cli_tool",
        SourceKind::CliTool,
        "tool",
        PathBuf::from(source),
        scope,
    )
}

#[tokio::test]
async fn cli_tool_adapter_declares_tool_script_api_scopes() {
    let adapter = CliToolSourceAdapter::new();
    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "cli_tool");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::CliTool))
    );
    for scope in [SourceScope::Tool, SourceScope::Script, SourceScope::Api] {
        let tag = format!(
            "scope:{}",
            serde_json::to_value(scope).unwrap().as_str().unwrap()
        );
        assert!(
            capability.0.features.contains(&tag),
            "missing scope {scope:?}"
        );
    }
}

#[tokio::test]
async fn cli_tool_adapter_round_trips_a_tool_source_to_a_document() {
    let adapter = CliToolSourceAdapter::new();
    let plan = cli_tool_plan("tool:rg --help", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(manifest.items[0].source_item_key, SourceItemKey::from("rg"));

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 1);

    let staged = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(staged.data.len(), 1);
    let doc = &staged.data[0];
    assert_eq!(
        doc.metadata.0.get("source_family"),
        Some(&serde_json::json!("tool"))
    );
    assert!(!doc.metadata.0.contains_key("source_type"));
    assert_eq!(
        doc.metadata.0.get("source_kind"),
        Some(&serde_json::json!("cli_tool"))
    );
    assert_eq!(
        doc.metadata.0.get("tool_name"),
        Some(&serde_json::json!("rg"))
    );
    assert_eq!(
        doc.metadata.0.get("tool_action"),
        Some(&serde_json::json!("metadata"))
    );
    assert_eq!(
        doc.metadata.0.get("tool_side_effect_class"),
        Some(&serde_json::json!("none"))
    );
    match &doc.content {
        ContentRef::InlineText { text } => assert!(text.contains("rg")),
        other => panic!("expected inline text content, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_tool_adapter_api_scope_still_resolves_metadata_only() {
    let adapter = CliToolSourceAdapter::new();
    let plan = cli_tool_plan("tool:rg --help", SourceScope::Api);

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    let staged = adapter.normalize(&plan, acquisition).await.unwrap();

    assert_eq!(
        staged.data[0].metadata.0.get("tool_action"),
        Some(&serde_json::json!("metadata"))
    );
}

#[tokio::test]
async fn cli_tool_adapter_execute_scope_runs_once_and_redacts_output() {
    let adapter = CliToolSourceAdapter::new();
    let mut plan = cli_tool_plan("tool:/bin/echo sk-super-secret", SourceScope::Api);
    plan.request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    plan.request.options.values.insert(
        "command_allowlist".to_string(),
        serde_json::json!(["/bin/echo"]),
    );
    plan.request.metadata.insert(
        "tool_execute_authorized".to_string(),
        serde_json::json!(true),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(
        acquisition.fetched_items[0].metadata["tool_action"],
        "execute"
    );
    assert_eq!(
        acquisition.fetched_items[0].metadata["redaction_status"],
        "redacted"
    );
    match &acquisition.fetched_items[0].content_ref {
        ContentRef::InlineText { text } => {
            assert!(text.contains("[REDACTED]") || text.contains("[redacted-secret]"));
            assert!(!text.contains("sk-super-secret"));
        }
        other => panic!("expected inline text content, got {other:?}"),
    }

    let staged = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(
        staged.data[0].metadata.0.get("tool_action"),
        Some(&serde_json::json!("execute"))
    );
    assert_eq!(
        staged.data[0].metadata.0.get("redaction_status"),
        Some(&serde_json::json!("redacted"))
    );
}

#[tokio::test]
async fn cli_tool_adapter_execute_requires_internal_auth_marker() {
    let adapter = CliToolSourceAdapter::new();
    let mut plan = cli_tool_plan("tool:/bin/echo hello", SourceScope::Api);
    plan.request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("execute"));
    plan.request.options.values.insert(
        "command_allowlist".to_string(),
        serde_json::json!(["/bin/echo"]),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let err = adapter.acquire(&plan, &diff).await.unwrap_err();
    assert_eq!(err.code.0, "auth.scope_required");
}

#[tokio::test]
async fn cli_tool_adapter_accepts_router_cli_shorthand() {
    let adapter = CliToolSourceAdapter::new();
    let plan = cli_tool_plan("cli:rg --help", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(manifest.items[0].source_item_key, SourceItemKey::from("rg"));
    assert_eq!(manifest.items[0].metadata["tool_command"], "rg");
}

#[tokio::test]
async fn cli_tool_adapter_accepts_router_canonical_uri() {
    let adapter = CliToolSourceAdapter::new();
    let plan = cli_tool_plan("cli://rg", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(manifest.items[0].source_item_key, SourceItemKey::from("rg"));
    assert_eq!(manifest.items[0].metadata["tool_command"], "rg");
}

#[tokio::test]
async fn cli_tool_adapter_rejects_mismatched_route_adapter() {
    let adapter = CliToolSourceAdapter::new();
    let mut plan = cli_tool_plan("tool:rg --help", SourceScope::Tool);
    plan.route.adapter.name = "mcp_tool".to_string();

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "adapter.cli_tool.mismatch");
}

#[tokio::test]
async fn cli_tool_adapter_rejects_unsupported_scope() {
    let adapter = CliToolSourceAdapter::new();
    let plan = cli_tool_plan("tool:rg --help", SourceScope::Repo);

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "adapter.scope.unsupported");
}
