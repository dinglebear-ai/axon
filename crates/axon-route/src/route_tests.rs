use axon_api::{AuthorityLevel, SourceIntent, SourceKind, SourceRequest, SourceScope};

use crate::{
    AdapterRegistry, AuthorityRecord, InMemoryAuthorityRegistry, SourceResolver, SourceRouter,
};

fn resolver_with_authority() -> SourceResolver {
    SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_shadcn_docs",
                "https://ui.shadcn.com/docs",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("shadcn.com")
            .with_entrypoint(SourceScope::Docs, "https://ui.shadcn.com/docs"),
        ]),
        AdapterRegistry::target_defaults(),
    )
}

#[test]
fn resolver_maps_known_alias_to_official_docs_entrypoint() {
    let resolver = resolver_with_authority();
    let mut request = SourceRequest::new("shadcn.com");
    request.scope = Some(SourceScope::Docs);

    let resolved = resolver.resolve(&request).expect("alias resolves");

    assert_eq!(resolved.requested_uri, "shadcn.com");
    assert_eq!(resolved.canonical_uri, "https://ui.shadcn.com/docs");
    assert_eq!(resolved.source_kind, SourceKind::Web);
    assert_eq!(resolved.default_scope, SourceScope::Docs);
    assert_eq!(resolved.authority, AuthorityLevel::Official);
    assert!(resolved.confidence >= 0.9);
    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.code == "authority.entrypoint_mapped")
    );
}

#[test]
fn resolver_normalizes_source_families_without_fetching_content() {
    let resolver = resolver_with_authority();

    let cases = [
        (
            SourceRequest::new("example.com"),
            SourceKind::Web,
            "https://example.com/",
            SourceScope::Site,
            "web",
        ),
        (
            SourceRequest::new("https://github.com/jmagar/axon"),
            SourceKind::Git,
            "github://jmagar/axon",
            SourceScope::Repo,
            "github",
        ),
        (
            SourceRequest::new("jmagar/axon"),
            SourceKind::Git,
            "github://jmagar/axon",
            SourceScope::Repo,
            "github",
        ),
        (
            SourceRequest::new("crates:serde"),
            SourceKind::Registry,
            "pkg://crates/serde",
            SourceScope::Package,
            "crates",
        ),
        (
            SourceRequest::new("npm:@modelcontextprotocol/sdk"),
            SourceKind::Registry,
            "pkg://npm/@modelcontextprotocol/sdk",
            SourceScope::Package,
            "npm",
        ),
        (
            SourceRequest::new("pypi:FastAPI"),
            SourceKind::Registry,
            "pkg://pypi/fastapi",
            SourceScope::Package,
            "pypi",
        ),
        (
            SourceRequest::new("docker:library/postgres:16"),
            SourceKind::Registry,
            "docker://docker.io/library/postgres:16",
            SourceScope::Package,
            "docker",
        ),
        (
            SourceRequest::new("https://gitlab.com/group/subgroup/project"),
            SourceKind::Git,
            "gitlab://gitlab.com/group/subgroup/project",
            SourceScope::Repo,
            "gitlab",
        ),
        (
            SourceRequest::new("https://git.example.com/org/project.git"),
            SourceKind::Git,
            "git+https://git.example.com/org/project.git",
            SourceScope::Repo,
            "git",
        ),
        (
            SourceRequest::new("r/rust"),
            SourceKind::Reddit,
            "reddit://r/rust",
            SourceScope::Subreddit,
            "reddit",
        ),
        (
            SourceRequest::new("https://youtube.com/watch?v=dQw4w9WgXcQ"),
            SourceKind::Youtube,
            "youtube://video/dQw4w9WgXcQ",
            SourceScope::Video,
            "youtube",
        ),
        (
            SourceRequest::new("rss:https://example.com/feed.xml"),
            SourceKind::Feed,
            "feed://example.com/feed.xml",
            SourceScope::Feed,
            "feed",
        ),
        (
            SourceRequest::new("session:claude:abc123"),
            SourceKind::Session,
            "session://claude/abc123",
            SourceScope::Thread,
            "session",
        ),
        (
            SourceRequest::new("cli:rg --help"),
            SourceKind::CliTool,
            "cli://rg",
            SourceScope::Tool,
            "cli",
        ),
        (
            SourceRequest::new("mcp:context7/resolve-library-id"),
            SourceKind::McpTool,
            "mcp://context7/tools/resolve-library-id",
            SourceScope::Tool,
            "mcp",
        ),
    ];

    for (request, source_kind, canonical_uri, default_scope, adapter) in cases {
        let resolved = resolver
            .resolve(&request)
            .unwrap_or_else(|err| panic!("{} should resolve: {err}", request.source));
        assert_eq!(resolved.source_kind, source_kind, "{}", request.source);
        assert_eq!(resolved.canonical_uri, canonical_uri, "{}", request.source);
        assert_eq!(resolved.default_scope, default_scope, "{}", request.source);
        assert_eq!(resolved.candidate_adapters[0].adapter.name, adapter);
    }
}

#[test]
fn resolver_uses_equivalent_canonical_uri_for_noisy_web_urls() {
    let resolver = resolver_with_authority();
    let clean = resolver
        .resolve(&SourceRequest::new("https://example.com/docs"))
        .expect("clean resolves");
    let noisy = resolver
        .resolve(&SourceRequest::new(
            "HTTPS://Example.COM:443//docs/?utm_source=newsletter&fbclid=abc#install",
        ))
        .expect("noisy resolves");

    assert_eq!(noisy.canonical_uri, "https://example.com/docs");
    assert_eq!(clean.canonical_uri, noisy.canonical_uri);
    assert_eq!(clean.source_id, noisy.source_id);
}

#[test]
fn resolver_preserves_meaningful_query_params_and_redacts_sensitive_params() {
    let resolver = resolver_with_authority();
    let search = resolver
        .resolve(&SourceRequest::new(
            "https://example.com/search?utm_source=newsletter&q=rust&page=2",
        ))
        .expect("search URL resolves");
    let sensitive = resolver
        .resolve(&SourceRequest::new(
            "https://example.com/search?token=abc&q=rust&password=secret",
        ))
        .expect("sensitive URL resolves");

    assert_eq!(
        search.canonical_uri,
        "https://example.com/search?page=2&q=rust"
    );
    assert_eq!(
        sensitive.canonical_uri,
        "https://example.com/search?password=REDACTED&q=rust&token=REDACTED"
    );
    assert!(
        sensitive
            .warnings
            .iter()
            .any(|warning| warning.code == "source.query.sensitive_redacted")
    );
}

#[test]
fn resolver_keeps_local_absolute_paths_out_of_public_identity() {
    let resolver = resolver_with_authority();
    let request = SourceRequest::local_path("/home/jmagar/workspace/axon/crates/axon-route", true);

    let resolved = resolver.resolve(&request).expect("local path resolves");

    assert_eq!(resolved.source_kind, SourceKind::Local);
    assert_eq!(resolved.default_scope, SourceScope::Directory);
    assert!(resolved.canonical_uri.starts_with("local://lp_"));
    assert_eq!(resolved.requested_uri, "local://redacted");
    assert!(!resolved.canonical_uri.contains("/home/jmagar"));
    assert!(!resolved.requested_uri.contains("/home/jmagar"));
    assert!(resolved.display_name.contains("axon-route"));
}

#[test]
fn resolver_keys_equivalent_local_path_spellings_together() {
    let resolver = resolver_with_authority();
    let plain = resolver
        .resolve(&SourceRequest::local_path(
            "/home/jmagar/workspace/axon/crates/axon-route",
            true,
        ))
        .expect("plain path resolves");
    let trailing = resolver
        .resolve(&SourceRequest::local_path(
            "/home/jmagar/workspace/axon/crates/axon-route/",
            true,
        ))
        .expect("trailing path resolves");

    assert_eq!(plain.canonical_uri, trailing.canonical_uri);
    assert_eq!(plain.source_id, trailing.source_id);
}

#[test]
fn resolver_normalizes_upload_sources() {
    let resolver = resolver_with_authority();
    let resolved = resolver
        .resolve(&SourceRequest::new("upload:artifact_123"))
        .expect("upload source resolves");

    assert_eq!(resolved.source_kind, SourceKind::Upload);
    assert_eq!(resolved.canonical_uri, "upload://artifact_123");
    assert_eq!(resolved.default_scope, SourceScope::File);
    assert_eq!(resolved.candidate_adapters[0].adapter.name, "upload");
}

#[test]
fn resolver_warns_when_source_shape_is_inferred() {
    let resolver = resolver_with_authority();
    let domain = resolver
        .resolve(&SourceRequest::new("example.com/docs"))
        .expect("scheme-less docs domain resolves");
    let shorthand = resolver
        .resolve(&SourceRequest::new("jmagar/axon"))
        .expect("github shorthand resolves");

    assert!(domain.confidence < 0.9);
    assert!(shorthand.confidence < 0.9);
    assert!(!domain.reason.is_empty());
    assert!(!shorthand.reason.is_empty());
    assert!(
        domain
            .warnings
            .iter()
            .any(|warning| warning.code == "source.inferred.scheme")
    );
    assert!(
        shorthand
            .warnings
            .iter()
            .any(|warning| warning.code == "source.inferred.github_shorthand")
    );
}

#[test]
fn target_registry_declares_expected_route_time_adapters() {
    let registry = AdapterRegistry::target_defaults();
    let expected = [
        ("cli", SourceKind::CliTool, SourceScope::Tool),
        ("crates", SourceKind::Registry, SourceScope::Package),
        ("docker", SourceKind::Registry, SourceScope::Package),
        ("feed", SourceKind::Feed, SourceScope::Feed),
        ("git", SourceKind::Git, SourceScope::Repo),
        ("github", SourceKind::Git, SourceScope::Repo),
        ("gitlab", SourceKind::Git, SourceScope::Repo),
        ("gitea", SourceKind::Git, SourceScope::Repo),
        ("local", SourceKind::Local, SourceScope::Directory),
        ("mcp", SourceKind::McpTool, SourceScope::Tool),
        ("npm", SourceKind::Registry, SourceScope::Package),
        ("pypi", SourceKind::Registry, SourceScope::Package),
        ("reddit", SourceKind::Reddit, SourceScope::Subreddit),
        ("session", SourceKind::Session, SourceScope::Thread),
        ("upload", SourceKind::Upload, SourceScope::File),
        ("web", SourceKind::Web, SourceScope::Site),
        ("youtube", SourceKind::Youtube, SourceScope::Video),
    ];

    for (name, source_kind, default_scope) in expected {
        let adapter = registry
            .find(name)
            .unwrap_or_else(|| panic!("{name} exists"));
        assert_eq!(adapter.source_kind, source_kind, "{name}");
        assert_eq!(adapter.default_scope, default_scope, "{name}");
        assert!(
            adapter.supported_scopes.contains(&default_scope),
            "{name} supports its default scope"
        );
    }
}

#[test]
fn router_rejects_unsupported_scope_before_acquisition() {
    let resolver = resolver_with_authority();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("crates:serde");
    request.scope = Some(SourceScope::Subreddit);
    let resolved = resolver.resolve(&request).expect("source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("unsupported scope fails before acquisition");

    assert_eq!(err.code.0, "source.scope.unsupported");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_rejects_unknown_explicit_adapter() {
    let resolver = resolver_with_authority();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request.adapter = Some("missing-adapter".to_string());
    let resolved = resolver.resolve(&request).expect("source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("unknown adapter fails before acquisition");

    assert_eq!(err.code.0, "route.adapter.unknown");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_rejects_adapter_that_does_not_support_source_kind() {
    let resolver = resolver_with_authority();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request.adapter = Some("github".to_string());
    let resolved = resolver.resolve(&request).expect("source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("source-kind mismatch fails before acquisition");

    assert_eq!(err.code.0, "route.adapter.unsupported_source");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_rejects_explicit_adapter_outside_resolved_provider_family() {
    let resolver = resolver_with_authority();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("https://github.com/jmagar/axon");
    request.adapter = Some("git".to_string());
    let resolved = resolver.resolve(&request).expect("github source resolves");

    let err = router
        .route(&request, resolved)
        .expect_err("generic git adapter is not the resolved github provider");

    assert_eq!(err.code.0, "route.adapter.unsupported_source");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn router_routes_map_as_first_class_scope() {
    let resolver = resolver_with_authority();
    let router = SourceRouter::new(AdapterRegistry::target_defaults());
    let mut request = SourceRequest::new("example.com");
    request.intent = SourceIntent::Map;
    request.scope = Some(SourceScope::Map);
    request.embed = false;
    let resolved = resolver.resolve(&request).expect("source resolves");

    let route = router.route(&request, resolved).expect("map routes");

    assert_eq!(route.adapter.name, "web");
    assert_eq!(route.scope, SourceScope::Map);
    assert!(route.graph_fact_kinds.contains(&"source".to_string()));
}
