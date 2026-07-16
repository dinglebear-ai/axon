use std::path::PathBuf;

use axon_api::source::*;

use crate::SourceAdapter;
use crate::local_test_support::*;
use crate::mcp_tool::McpToolSourceAdapter;

fn mcp_tool_plan(source: &str, scope: SourceScope) -> SourcePlan {
    source_plan_for(
        "mcp_tool",
        SourceKind::McpTool,
        "mcp",
        PathBuf::from(source),
        scope,
    )
}

#[tokio::test]
async fn mcp_tool_adapter_declares_tool_api_scopes() {
    let adapter = McpToolSourceAdapter::new();
    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "mcp_tool");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::McpTool))
    );
    for scope in [SourceScope::Tool, SourceScope::Api] {
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
async fn mcp_tool_adapter_round_trips_a_tool_source_to_a_document() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("mcp://labby/search", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(
        manifest.items[0].source_item_key,
        SourceItemKey::from("labby/search")
    );

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
        Some(&serde_json::json!("mcp_tool"))
    );
    assert_eq!(
        doc.metadata.0.get("tool_name"),
        Some(&serde_json::json!("labby/search"))
    );
    assert_eq!(
        doc.metadata.0.get("tool_action"),
        Some(&serde_json::json!("metadata"))
    );
    match &doc.content {
        ContentRef::InlineText { text } => assert!(text.contains("mcp://labby/search")),
        other => panic!("expected inline text content, got {other:?}"),
    }
}

#[tokio::test]
async fn mcp_tool_adapter_api_scope_still_resolves_metadata_only() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("mcp://labby/search", SourceScope::Api);

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
async fn mcp_tool_adapter_call_scope_invokes_command_caller_once() {
    let adapter = McpToolSourceAdapter::new();
    let mut plan = mcp_tool_plan("mcp://labby/search", SourceScope::Api);
    plan.request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("call"));
    plan.request.options.values.insert(
        "mcp_allowlist".to_string(),
        serde_json::json!(["labby/search"]),
    );
    plan.request.options.values.insert(
        "mcp_caller_command".to_string(),
        serde_json::json!("/bin/echo"),
    );
    plan.request.metadata.insert(
        "tool_execute_authorized".to_string(),
        serde_json::json!(true),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items[0].metadata["tool_action"], "call");
    match &acquisition.fetched_items[0].content_ref {
        ContentRef::InlineText { text } => {
            assert!(text.contains("labby"));
            assert!(text.contains("search"));
        }
        other => panic!("expected inline text content, got {other:?}"),
    }

    let staged = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(
        staged.data[0].metadata.0.get("tool_action"),
        Some(&serde_json::json!("call"))
    );
}

#[tokio::test]
async fn mcp_tool_adapter_call_requires_caller() {
    let adapter = McpToolSourceAdapter::new();
    let mut plan = mcp_tool_plan("mcp://labby/search", SourceScope::Api);
    plan.request
        .options
        .values
        .insert("execution_mode".to_string(), serde_json::json!("call"));
    plan.request.options.values.insert(
        "mcp_allowlist".to_string(),
        serde_json::json!(["labby/search"]),
    );
    plan.request.metadata.insert(
        "tool_execute_authorized".to_string(),
        serde_json::json!(true),
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());
    let err = adapter.acquire(&plan, &diff).await.unwrap_err();
    assert_eq!(err.code.0, "mcp.caller_missing");
}

#[tokio::test]
async fn mcp_tool_adapter_accepts_router_shorthand() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("mcp:labby/search", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(
        manifest.items[0].source_item_key,
        SourceItemKey::from("labby/search")
    );
}

#[tokio::test]
async fn mcp_tool_adapter_accepts_router_canonical_tools_uri() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("mcp://labby/tools/search", SourceScope::Tool);

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(
        manifest.items[0].source_item_key,
        SourceItemKey::from("labby/search")
    );
}

#[tokio::test]
async fn mcp_tool_adapter_rejects_mismatched_route_adapter() {
    let adapter = McpToolSourceAdapter::new();
    let mut plan = mcp_tool_plan("mcp://labby/search", SourceScope::Tool);
    plan.route.adapter.name = "cli_tool".to_string();

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "adapter.mcp_tool.mismatch");
}

#[tokio::test]
async fn mcp_tool_adapter_rejects_unsupported_scope() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("mcp://labby/search", SourceScope::Repo);

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "adapter.scope.unsupported");
}

#[tokio::test]
async fn mcp_tool_adapter_rejects_invalid_uri() {
    let adapter = McpToolSourceAdapter::new();
    let plan = mcp_tool_plan("not-an-mcp-uri", SourceScope::Tool);

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "mcp.uri_invalid");
}
