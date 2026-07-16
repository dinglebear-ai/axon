use axon_api::{SafetyClass, SourceKind, SourceRequest, SourceScope};
use serde_json::json;

use crate::{
    AdapterRegistry, InMemoryAuthorityRegistry, RouteSecurityPolicy, SourceResolver, SourceRouter,
};

fn resolver() -> SourceResolver {
    SourceResolver::new(
        InMemoryAuthorityRegistry::default(),
        AdapterRegistry::target_defaults(),
    )
}

#[test]
fn router_rejects_unknown_route_options() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request
        .options
        .values
        .insert("definitely_not_valid".to_string(), true.into());
    let resolved = resolver.resolve(&request).expect("source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("unknown route option fails");

    assert_eq!(err.code.0, "route.options.unsupported");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_requires_explicit_tool_execution_allowance() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let request = SourceRequest::new("mcp:context7/resolve-library-id");
    let resolved = resolver.resolve(&request).expect("mcp source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("tool execution needs explicit opt-in");

    assert_eq!(err.code.0, "route.tool_execution.denied");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_reports_credentials_required_by_adapter() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let request = SourceRequest::new("r/rust");
    let resolved = resolver.resolve(&request).expect("reddit source resolves");

    let route = router.route(&request, resolved).expect("reddit routes");

    assert_eq!(route.adapter.name, "reddit");
    assert!(
        route
            .credential_requirements
            .iter()
            .any(|requirement| requirement.required)
    );
    assert_eq!(route.safety_class, SafetyClass::AuthenticatedNetwork);
}

#[test]
fn router_marks_memory_and_upload_as_authenticated_sources() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());

    for source in ["memory://mem_abc", "artifact://art_abc"] {
        let request = SourceRequest::new(source);
        let resolved = resolver.resolve(&request).expect("source resolves");
        let route = router.route(&request, resolved).expect("source routes");
        assert_eq!(route.safety_class, SafetyClass::AuthenticatedNetwork);
    }
}

#[test]
fn router_rejects_caller_controlled_tool_execution_option() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("cli:repomix --help");
    request
        .options
        .values
        .insert("allow_tool_execution".to_string(), true.into());
    let resolved = resolver.resolve(&request).expect("cli source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("public option does not authorize tool execution");

    assert_eq!(err.code.0, "route.options.unsupported");
}

#[test]
fn router_allows_tool_execution_with_trusted_policy() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let request = SourceRequest::new("cli:repomix --help");
    let resolved = resolver.resolve(&request).expect("cli source resolves");

    let route = router
        .route_with_policy(
            &request,
            resolved,
            RouteSecurityPolicy::trusted_tool_execution(),
        )
        .expect("trusted policy allows cli route");

    assert_eq!(route.adapter.name, "cli");
    assert_eq!(route.safety_class, SafetyClass::ToolExecution);
    assert_eq!(route.parser_hints[0].parser_id, "cli_tool");
}

#[test]
fn router_carries_tool_execution_options_with_trusted_policy() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("cli:repomix --help");
    request.scope = Some(SourceScope::Api);
    request
        .options
        .values
        .insert("execution_mode".to_string(), json!("execute"));
    request
        .options
        .values
        .insert("command_allowlist".to_string(), json!(["repomix"]));
    let resolved = resolver.resolve(&request).expect("cli source resolves");

    let route = router
        .route_with_policy(
            &request,
            resolved,
            RouteSecurityPolicy::trusted_tool_execution(),
        )
        .expect("trusted policy allows cli execution route options");

    assert_eq!(route.scope, SourceScope::Api);
    assert_eq!(
        route.validated_options.values["execution_mode"],
        json!("execute")
    );
    assert_eq!(
        route.validated_options.values["command_allowlist"],
        json!(["repomix"])
    );
}

#[test]
fn router_uses_api_style_parser_ids_for_mcp_tools() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let request = SourceRequest::new("mcp:context7/resolve-library-id");
    let resolved = resolver.resolve(&request).expect("mcp source resolves");

    let route = router
        .route_with_policy(
            &request,
            resolved,
            RouteSecurityPolicy::trusted_tool_execution(),
        )
        .expect("trusted policy allows mcp route");

    assert_eq!(route.parser_hints[0].parser_id, "mcp_tool");
}

#[test]
fn router_rejects_forged_resolved_source_identity() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let request = SourceRequest::new("example.com");
    let mut resolved = resolver.resolve(&request).expect("source resolves");
    resolved.canonical_uri = "https://evil.example/".to_string();

    let err = router
        .route(&request, resolved)
        .expect_err("forged source identity fails");

    assert_eq!(err.code.0, "route.source.invalid");
}

#[test]
fn router_enforces_minimum_tool_safety_class_from_registry() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("cli", "1", SourceKind::CliTool, SourceScope::Tool)
            .with_safety_class(SafetyClass::PublicNetwork),
    ]);
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let router = SourceRouter::new(registry);
    let request = SourceRequest::new("cli:repomix --help");
    let resolved = resolver.resolve(&request).expect("cli resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("downgraded tool adapter is still denied");

    assert_eq!(err.code.0, "route.tool_execution.denied");
}

#[test]
fn router_preserves_stricter_safety_class_than_source_minimum() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("local", "1", SourceKind::Local, SourceScope::Directory)
            .with_safety_class(SafetyClass::ToolExecution),
    ]);
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let router = SourceRouter::new(registry);
    let request = SourceRequest::local_path("/tmp/axon-route-local-tool", true);
    let resolved = resolver.resolve(&request).expect("local resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("stricter local tool execution is still denied");

    assert_eq!(err.code.0, "route.tool_execution.denied");
}

#[test]
fn resolver_requires_canonical_adapter_hint_to_match_registry() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("zeta", "1", SourceKind::Web, SourceScope::Site)
            .with_scope(SourceScope::Page),
        crate::AdapterDefinition::new("alpha", "1", SourceKind::Web, SourceScope::Site)
            .with_scope(SourceScope::Page),
    ]);
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry);
    let request = SourceRequest::new("example.com");

    let err = resolver
        .resolve(&request)
        .expect_err("web source requires a matching web adapter hint");

    assert_eq!(err.code.0, "source.resolve.no_adapter");
    assert_eq!(err.stage, axon_error::ErrorStage::Resolving);
}

/// End-to-end: a web `SourceRequest` carrying the full documented web option
/// set (adapter-scopes.md "Web Adapter" table) routes successfully and the
/// values land unchanged in `RoutePlan.validated_options`
/// (`SourcePlan.route.validated_options` per the target DTO contract).
#[test]
fn router_carries_full_web_option_set_into_validated_options() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request
        .options
        .values
        .insert("max_pages".to_string(), json!(2000));
    request
        .options
        .values
        .insert("max_depth".to_string(), json!(10));
    request
        .options
        .values
        .insert("include_subdomains".to_string(), json!(false));
    request
        .options
        .values
        .insert("render_mode".to_string(), json!("auto_switch"));
    request
        .options
        .values
        .insert("discover_sitemaps".to_string(), json!(true));
    request
        .options
        .values
        .insert("max_sitemaps".to_string(), json!(512));
    request
        .options
        .values
        .insert("sitemap_since_days".to_string(), json!(0));
    request
        .options
        .values
        .insert("url_whitelist".to_string(), json!(["/docs"]));
    request
        .options
        .values
        .insert("url_blacklist".to_string(), json!(["/private"]));
    request
        .options
        .values
        .insert("etag_conditional".to_string(), json!(true));
    request
        .options
        .values
        .insert("min_markdown_chars".to_string(), json!(200));
    request
        .options
        .values
        .insert("drop_thin_markdown".to_string(), json!(true));
    request
        .options
        .values
        .insert("warc_path".to_string(), json!("artifact://warc/site.warc"));
    request.options.values.insert(
        "automation_script".to_string(),
        json!("artifact://automation/steps.json"),
    );
    request
        .options
        .values
        .insert("verticals_enabled".to_string(), json!(true));
    let resolved = resolver.resolve(&request).expect("web source resolves");

    let route = router
        .route(&request, resolved)
        .expect("full web option set routes");

    assert_eq!(route.adapter.name, "web");
    assert_eq!(route.validated_options.values, request.options.values);
}

#[test]
fn router_rejects_invalid_web_render_mode_value() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request
        .options
        .values
        .insert("render_mode".to_string(), json!("carrier_pigeon"));
    let resolved = resolver.resolve(&request).expect("web source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("invalid render_mode must fail before acquisition");

    assert_eq!(err.code.0, "route.options.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_rejects_negative_web_max_pages_value() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request
        .options
        .values
        .insert("max_pages".to_string(), json!(-5));
    let resolved = resolver.resolve(&request).expect("web source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("negative max_pages must fail before acquisition");

    assert_eq!(err.code.0, "route.options.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_rejects_non_array_web_url_whitelist_value() {
    let resolver = resolver();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request
        .options
        .values
        .insert("url_whitelist".to_string(), json!("/docs"));
    let resolved = resolver.resolve(&request).expect("web source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("non-array url_whitelist must fail before acquisition");

    assert_eq!(err.code.0, "route.options.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}
