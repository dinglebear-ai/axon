use axon_api::{AuthorityLevel, SourceKind, SourceRequest, SourceScope};

use crate::{AdapterRegistry, AuthorityRecord, InMemoryAuthorityRegistry, SourceResolver};

fn resolver() -> SourceResolver {
    SourceResolver::new(
        InMemoryAuthorityRegistry::default(),
        AdapterRegistry::target_defaults(),
    )
}

#[test]
fn resolver_preserves_non_default_ports_in_canonical_sources() {
    let resolver = resolver();
    let web = resolver
        .resolve(&SourceRequest::new("https://example.com:8443/docs"))
        .expect("web resolves");
    let feed = resolver
        .resolve(&SourceRequest::new("rss:https://example.com:8443/feed.xml"))
        .expect("feed resolves");
    let git = resolver
        .resolve(&SourceRequest::new(
            "https://git.example.com:8443/org/project.git",
        ))
        .expect("git resolves");

    assert_eq!(web.canonical_uri, "https://example.com:8443/docs");
    assert_eq!(feed.canonical_uri, "feed://example.com:8443/feed.xml");
    assert_eq!(
        git.canonical_uri,
        "git+https://git.example.com:8443/org/project.git"
    );
}

#[test]
fn resolver_auto_detects_feed_shaped_urls() {
    let feed = resolver()
        .resolve(&SourceRequest::new("https://example.com/feed.xml"))
        .expect("feed-shaped URL resolves");

    assert_eq!(feed.source_kind, SourceKind::Feed);
    assert_eq!(feed.default_scope, SourceScope::Feed);
    assert_eq!(feed.adapter.name, "feed");
    assert_eq!(feed.canonical_uri, "feed://example.com/feed.xml");
}

#[test]
fn resolver_routes_memory_and_upload_identities() {
    let memory = resolver()
        .resolve(&SourceRequest::new("memory://mem_abc"))
        .expect("memory resolves");
    assert_eq!(memory.source_kind, SourceKind::Memory);
    assert_eq!(memory.default_scope, SourceScope::Api);
    assert_eq!(memory.adapter.name, "memory");

    let upload = resolver()
        .resolve(&SourceRequest::new("upload:upl_abc"))
        .expect("upload resolves");
    assert_eq!(upload.source_kind, SourceKind::Upload);
    assert_eq!(upload.canonical_uri, "upload://upl_abc");
    assert_eq!(upload.adapter.name, "upload");
}

#[test]
fn resolver_rejects_noncanonical_memory_and_upload_identities() {
    for source in [
        "memory://",
        "memory://abc",
        "memory://mem_a/child",
        "upload:relative-path",
        "upload://upl_a/child",
    ] {
        assert!(
            resolver().resolve(&SourceRequest::new(source)).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn resolver_preserves_explicit_http_scheme() {
    let resolver = resolver();
    let web = resolver
        .resolve(&SourceRequest::new("http://localhost:8080/docs"))
        .expect("http web resolves");
    let git = resolver
        .resolve(&SourceRequest::new(
            "http://git.example.com/org/project.git",
        ))
        .expect("http git resolves");

    assert_eq!(web.canonical_uri, "http://localhost:8080/docs");
    assert_eq!(
        git.canonical_uri,
        "git+http://git.example.com/org/project.git"
    );
}

#[test]
fn resolver_uses_lexical_local_path_identity_without_requiring_existing_paths() {
    let resolver = resolver();
    let first = resolver
        .resolve(&SourceRequest::local_path(
            "/tmp/axon-route-missing/./nested/../repo",
            true,
        ))
        .expect("first path resolves");
    let second = resolver
        .resolve(&SourceRequest::local_path(
            "/tmp/axon-route-missing/repo/",
            true,
        ))
        .expect("second path resolves");

    assert_eq!(first.canonical_uri, second.canonical_uri);
    assert_eq!(first.source_id, second.source_id);
}

#[test]
fn resolver_preserves_leading_parent_components_in_relative_local_paths() {
    let resolver = resolver();
    let parent = resolver
        .resolve(&SourceRequest::local_path("../repo", true))
        .expect("parent relative path resolves");
    let nested_parent = resolver
        .resolve(&SourceRequest::local_path("../../repo", true))
        .expect("nested parent relative path resolves");
    let local = resolver
        .resolve(&SourceRequest::local_path("./repo", true))
        .expect("local relative path resolves");

    assert_ne!(parent.canonical_uri, local.canonical_uri);
    assert_ne!(nested_parent.canonical_uri, parent.canonical_uri);
    assert_ne!(nested_parent.source_id, local.source_id);
}

#[test]
fn resolver_does_not_classify_spoofed_provider_hosts() {
    let resolver = resolver();
    let youtube_spoof = resolver
        .resolve(&SourceRequest::new("https://notyoutube.com/watch?v=abc"))
        .expect("spoof host resolves as web");
    let gitlab_spoof = resolver
        .resolve(&SourceRequest::new(
            "https://notgitlab.example.com/org/repo",
        ))
        .expect("spoof host resolves as web");

    assert_eq!(youtube_spoof.source_kind, SourceKind::Web);
    assert_eq!(
        youtube_spoof.canonical_uri,
        "https://notyoutube.com/watch?v=abc"
    );
    assert_eq!(gitlab_spoof.source_kind, SourceKind::Web);
    assert_eq!(
        gitlab_spoof.canonical_uri,
        "https://notgitlab.example.com/org/repo"
    );
}

#[test]
fn resolver_redacts_common_signed_url_query_params() {
    let resolved = resolver()
        .resolve(&SourceRequest::new(
            "https://example.com/file?X-Amz-Signature=abc&sig=def&jwt=ghi&q=rust&access_key=key&AWSAccessKeyId=id&X-Amz-Credential=cred",
        ))
        .expect("signed URL resolves");

    assert_eq!(
        resolved.canonical_uri,
        "https://example.com/file?AWSAccessKeyId=REDACTED&X-Amz-Credential=REDACTED&X-Amz-Signature=REDACTED&access_key=REDACTED&jwt=REDACTED&q=rust&sig=REDACTED"
    );
}

#[test]
fn resolver_suppresses_query_secrets_for_git_provider_urls() {
    let resolved = resolver()
        .resolve(&SourceRequest::new(
            "https://gitlab.com/group/repo?AWSAccessKeyId=id&X-Amz-Credential=cred",
        ))
        .expect("gitlab URL resolves");

    assert_eq!(resolved.canonical_uri, "gitlab://gitlab.com/group/repo");
    assert_eq!(resolved.source, resolved.canonical_uri);
    assert!(!resolved.source.contains("AWSAccessKeyId"));
    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.code == "source.query.sensitive_redacted")
    );
}

#[test]
fn source_id_uses_stable_source_kind_spelling() {
    let id = crate::source_id::source_id(SourceKind::CliTool, "cli://rg");
    let expected = format!(
        "src_{}",
        &crate::source_id::stable_hash("cli_tool:cli://rg:v1")[..16]
    );

    assert_eq!(id.0, expected);
}

#[test]
fn resolver_rejects_empty_source_identifiers() {
    let resolver = resolver();
    let invalid = [
        "mcp:/tool",
        "mcp:server/",
        "session:claude:",
        "r/",
        "https://youtu.be/",
        "https://youtube.com/watch?v=",
        "npm:",
        "crates:",
        "pypi:",
        "docker:",
        "https://github.com/jmagar/.git",
        "ftp://example.com/docs",
    ];

    for source in invalid {
        let err = match resolver.resolve(&SourceRequest::new(source)) {
            Ok(resolved) => panic!("{source} should be rejected, got {resolved:?}"),
            Err(err) => err,
        };
        assert_eq!(err.code.0, "source.resolve.unsupported", "{source}");
    }
}

#[test]
fn resolver_preserves_root_local_path_identity() {
    assert_eq!(crate::local_path::normalize_local_path("/"), "/");

    let root = resolver()
        .resolve(&SourceRequest::local_path("/", true))
        .expect("root path resolves");

    assert!(root.canonical_uri.starts_with("local://lp_"));
    assert_eq!(root.source, "local://redacted");
}

#[test]
fn resolver_classifies_prefixed_self_hosted_git_providers() {
    let resolver = resolver();
    let gitlab = resolver
        .resolve(&SourceRequest::new("https://gitlab.example.com/org/repo"))
        .expect("self-hosted gitlab resolves");
    let gitea = resolver
        .resolve(&SourceRequest::new("https://gitea.example.com/org/repo"))
        .expect("self-hosted gitea resolves");
    let forgejo = resolver
        .resolve(&SourceRequest::new("https://forgejo.example.com/org/repo"))
        .expect("self-hosted forgejo resolves");

    assert_eq!(gitlab.canonical_uri, "gitlab://gitlab.example.com/org/repo");
    assert_eq!(gitlab.adapter.name, "gitlab");
    assert_eq!(gitea.canonical_uri, "gitea://gitea.example.com/org/repo");
    assert_eq!(gitea.adapter.name, "gitea");
    assert_eq!(
        forgejo.canonical_uri,
        "gitea://forgejo.example.com/org/repo"
    );
    assert_eq!(forgejo.adapter.name, "gitea");
}

#[test]
fn authority_aliases_preserve_provider_specific_adapter_hints() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_axon_repo",
                "github://jmagar/axon",
                SourceKind::Git,
                AuthorityLevel::Official,
            )
            .with_alias("axon-repo"),
            AuthorityRecord::new(
                "auth_fastapi_pkg",
                "pkg://pypi/fastapi",
                SourceKind::Registry,
                AuthorityLevel::Official,
            )
            .with_alias("fastapi"),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let github = resolver
        .resolve(&SourceRequest::new("axon-repo"))
        .expect("repo alias resolves");
    let pypi = resolver
        .resolve(&SourceRequest::new("fastapi"))
        .expect("package alias resolves");

    assert_eq!(github.adapter.name, "github");
    assert_eq!(pypi.adapter.name, "pypi");
}

#[test]
fn authority_alias_matching_is_scheme_case_insensitive() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_shadcn_docs",
                "https://ui.shadcn.com/docs",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("https://ui.shadcn.com/docs"),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let resolved = resolver
        .resolve(&SourceRequest::new("HTTPS://UI.SHADCN.COM/DOCS/"))
        .expect("uppercase scheme alias resolves");

    assert_eq!(resolved.authority, AuthorityLevel::Official);
    assert_eq!(resolved.canonical_uri, "https://ui.shadcn.com/docs");
}

#[test]
fn authority_records_derive_default_scope_from_entrypoints() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_axon_repo",
                "github://jmagar/axon",
                SourceKind::Git,
                AuthorityLevel::Official,
            )
            .with_alias("axon-source")
            .with_entrypoint(SourceScope::Repo, "github://jmagar/axon"),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let resolved = resolver
        .resolve(&SourceRequest::new("axon-source"))
        .expect("authority resolves");

    assert_eq!(resolved.default_scope, SourceScope::Repo);
    assert_eq!(resolved.canonical_uri, "github://jmagar/axon");
}

#[test]
fn resolver_unions_available_scopes_from_all_candidate_adapters() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("alpha-web", "1", SourceKind::Web, SourceScope::Site),
        crate::AdapterDefinition::new("beta-web", "1", SourceKind::Web, SourceScope::Page)
            .with_scope(SourceScope::Map),
    ]);
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry);

    let mut request = SourceRequest::new("example.com");
    request.adapter = Some("alpha-web".to_string());

    let resolved = resolver.resolve(&request).expect("web resolves");

    assert!(resolved.available_scopes.contains(&SourceScope::Site));
    assert!(resolved.available_scopes.contains(&SourceScope::Page));
    assert!(resolved.available_scopes.contains(&SourceScope::Map));
}

#[test]
fn ambiguous_web_adapter_candidates_return_typed_error() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("web", "1", SourceKind::Web, SourceScope::Site),
        crate::AdapterDefinition::new("web", "2", SourceKind::Web, SourceScope::Page),
    ]);
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry);

    let err = resolver
        .resolve(&SourceRequest::new("example.com"))
        .expect_err("web adapters without an explicit selection are ambiguous");

    assert_eq!(err.code.0, "source.resolve.ambiguous");
}

#[test]
fn ambiguous_registry_adapter_candidates_return_typed_error() {
    let registry = AdapterRegistry::from_adapters(vec![
        crate::AdapterDefinition::new("crates", "1", SourceKind::Registry, SourceScope::Package),
        crate::AdapterDefinition::new("crates", "2", SourceKind::Registry, SourceScope::Package),
    ]);
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_package",
                "pkg://crates/example",
                SourceKind::Registry,
                AuthorityLevel::Official,
            )
            .with_alias("custom-package"),
        ]),
        registry,
    );

    let err = resolver
        .resolve(&SourceRequest::new("custom-package"))
        .expect_err("registry adapters without an explicit selection are ambiguous");

    assert_eq!(err.code.0, "source.resolve.ambiguous");
}

#[test]
fn empty_adapter_candidates_return_typed_error() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::default(),
        AdapterRegistry::from_adapters(Vec::new()),
    );

    let err = resolver
        .resolve(&SourceRequest::new("example.com"))
        .expect_err("empty adapter registry cannot serialize a resolved source");

    assert_eq!(err.code.0, "source.resolve.no_adapter");
}

#[test]
fn resolver_rejects_ambiguous_authority_aliases() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_fastapi_pkg",
                "pkg://pypi/fastapi",
                SourceKind::Registry,
                AuthorityLevel::Official,
            )
            .with_alias("fastapi"),
            AuthorityRecord::new(
                "auth_fastapi_docs",
                "https://fastapi.tiangolo.com/",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("fastapi"),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let err = resolver
        .resolve(&SourceRequest::new("fastapi"))
        .expect_err("duplicate alias is ambiguous");

    assert_eq!(err.code.0, "source.resolve.ambiguous");
}

#[test]
fn resolver_redacts_requested_uri_when_input_contains_url_secrets() {
    let resolved = resolver()
        .resolve(&SourceRequest::new(
            "https://example.com/file?token=abc&q=rust",
        ))
        .expect("secret URL resolves");

    assert_eq!(
        resolved.source,
        "https://example.com/file?q=rust&token=REDACTED"
    );
    assert!(!resolved.source.contains("abc"));
}

#[test]
fn resolver_preserves_github_subpath_identity_for_supported_scopes() {
    let resolver = resolver();
    let issue = resolver
        .resolve(&SourceRequest::new(
            "https://github.com/jmagar/axon/issues/308",
        ))
        .expect("issue resolves");
    let pull = resolver
        .resolve(&SourceRequest::new(
            "https://github.com/jmagar/axon/pull/308",
        ))
        .expect("pull resolves");
    let branch = resolver
        .resolve(&SourceRequest::new(
            "https://github.com/jmagar/axon/tree/main",
        ))
        .expect("branch resolves");

    assert_eq!(issue.canonical_uri, "github://jmagar/axon/issues/308");
    assert_eq!(pull.canonical_uri, "github://jmagar/axon/pulls/308");
    assert_eq!(branch.canonical_uri, "github://jmagar/axon/tree/main");
    assert_ne!(issue.source_id, pull.source_id);
}

#[test]
fn resolver_uses_authority_record_confidence() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_low_confidence_docs",
                "https://example.com/docs",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("low-docs")
            .with_confidence(0.42),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let resolved = resolver
        .resolve(&SourceRequest::new("low-docs"))
        .expect("authority resolves");

    assert_eq!(resolved.confidence, 0.42);
}

#[test]
fn resolver_keeps_authority_evidence_out_of_public_resolved_source() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_shadcn_docs",
                "https://ui.shadcn.com/docs",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("shadcn")
            .with_confidence(0.94)
            .with_evidence("official_docs", "https://ui.shadcn.com/docs", 0.94),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let resolved = resolver
        .resolve(&SourceRequest::new("shadcn"))
        .expect("authority resolves");

    assert_eq!(resolved.authority, AuthorityLevel::Official);
    assert_eq!(resolved.confidence, 0.94);
    assert!(resolved.metadata.is_empty());
}

#[test]
fn resolver_rejects_inconsistent_authority_records() {
    let resolver = SourceResolver::new(
        InMemoryAuthorityRegistry::from_records(vec![
            AuthorityRecord::new(
                "auth_bad_docs",
                "local://lp_bad",
                SourceKind::Web,
                AuthorityLevel::Official,
            )
            .with_alias("bad-docs"),
        ]),
        AdapterRegistry::target_defaults(),
    );

    let err = resolver
        .resolve(&SourceRequest::new("bad-docs"))
        .expect_err("inconsistent authority record fails");

    assert_eq!(err.code.0, "source.authority.invalid");
}
