use super::*;
use uuid::Uuid;

fn plan(scope: SourceScope, canonical_uri: &str) -> SourcePlan {
    let adapter = AdapterRef {
        name: "github".to_string(),
        version: "1".to_string(),
    };
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(298_298)),
        request: SourceRequest::new("https://github.com/jmagar/axon".to_string()),
        route: RoutePlan {
            source: ResolvedSource {
                source: canonical_uri.to_string(),
                canonical_uri: canonical_uri.to_string(),
                source_id: SourceId::from("src_git_vertical_test"),
                source_kind: SourceKind::Git,
                adapter: adapter.clone(),
                default_scope: scope,
                available_scopes: vec![scope],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter,
            scope,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::PublicNetwork,
            option_schema_id: "adapter:git:options:v1".to_string(),
            validated_options: AdapterOptions {
                values: MetadataMap::new(),
            },
            chunking_hints: Vec::new(),
            parser_hints: Vec::new(),
            graph_fact_kinds: Vec::new(),
            watch_supported: true,
            refresh_supported: true,
        },
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::from("cfg_git_vertical_test"),
        provider_reservations: Vec::new(),
    }
}

#[test]
fn extractor_for_scope_maps_github_subpage_scopes() {
    assert_eq!(
        extractor_for_scope(SourceScope::Issue),
        Some("github_issue")
    );
    assert_eq!(
        extractor_for_scope(SourceScope::PullRequest),
        Some("github_pr")
    );
    assert_eq!(
        extractor_for_scope(SourceScope::Release),
        Some("github_release")
    );
    assert_eq!(extractor_for_scope(SourceScope::Repo), None);
    assert_eq!(extractor_for_scope(SourceScope::Directory), None);
}

#[test]
fn resolve_rebuilds_each_extractor_matcher_url() {
    // `pulls` in the canonical URI must become `pull` — the singular form
    // github_pr::matches() requires.
    let pr = plan(SourceScope::PullRequest, "github://jmagar/axon/pulls/42");
    assert_eq!(
        resolve(&pr),
        Some((
            "github_pr",
            "https://github.com/jmagar/axon/pull/42".to_string()
        ))
    );

    let issue = plan(SourceScope::Issue, "github://jmagar/axon/issues/7");
    assert_eq!(
        resolve(&issue),
        Some((
            "github_issue",
            "https://github.com/jmagar/axon/issues/7".to_string()
        ))
    );

    let release = plan(
        SourceScope::Release,
        "github://jmagar/axon/releases/tag/v1.2.3",
    );
    assert_eq!(
        resolve(&release),
        Some((
            "github_release",
            "https://github.com/jmagar/axon/releases/tag/v1.2.3".to_string()
        ))
    );
}

#[test]
fn resolve_ignores_clone_scopes_and_non_github_hosts() {
    let repo = plan(SourceScope::Repo, "github://jmagar/axon");
    assert!(resolve(&repo).is_none());
    assert!(!is_vertical(&repo));

    // A gitlab canonical carrying a vertical scope must NOT be handed to a
    // `github_*` extractor — only github.com sub-pages take this path.
    let gitlab = plan(SourceScope::Issue, "gitlab://group/proj/issues/9");
    assert!(resolve(&gitlab).is_none());
    assert!(!is_vertical(&gitlab));
}

#[test]
fn discover_emits_one_item_at_the_subpage_uri() {
    let pr = plan(SourceScope::PullRequest, "github://jmagar/axon/pulls/42");
    let manifest = discover(&pr).expect("discover");

    assert_eq!(manifest.items.len(), 1);
    assert_eq!(manifest.scope, SourceScope::PullRequest);
    let item = &manifest.items[0];
    assert_eq!(item.canonical_uri, "github://jmagar/axon/pulls/42");
    assert_eq!(item.content_kind, Some(ContentKind::Markdown));
    assert_eq!(item.item_kind, ItemKind::WebPage);
}
