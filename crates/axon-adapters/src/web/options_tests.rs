use axon_api::source::*;
use serde_json::json;
use uuid::Uuid;

use super::*;

fn plan_with_options(values: MetadataMap) -> SourcePlan {
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(1)),
        request: SourceRequest::new("https://example.com/docs"),
        route: RoutePlan {
            source: ResolvedSource {
                source: "https://example.com/docs".to_string(),
                canonical_uri: "https://example.com/docs".to_string(),
                source_id: SourceId::from("src_web_options_test"),
                source_kind: SourceKind::Web,
                adapter: AdapterRef {
                    name: "web".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                default_scope: SourceScope::Docs,
                available_scopes: vec![SourceScope::Docs],
                authority: AuthorityLevel::Inferred,
                confidence: 1.0,
                reason: "test".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter: AdapterRef {
                name: "web".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            scope: SourceScope::Docs,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::PublicNetwork,
            option_schema_id: "adapter:web:options:v1".to_string(),
            validated_options: AdapterOptions { values },
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
        config_snapshot_id: ConfigSnapshotId::from("cfg_web_options_test"),
        provider_reservations: Vec::new(),
    }
}

#[test]
fn effective_render_mode_defaults_to_auto_switch() {
    assert_eq!(
        effective_render_mode(&MetadataMap::new()),
        RenderMode::AutoSwitch
    );
}

#[test]
fn effective_render_mode_reads_validated_option() {
    let mut values = MetadataMap::new();
    values.insert("render_mode".to_string(), json!("http"));
    assert_eq!(effective_render_mode(&values), RenderMode::Http);
}

#[test]
fn min_markdown_chars_defaults_to_200() {
    assert_eq!(min_markdown_chars(&MetadataMap::new()), 200);
}

#[test]
fn min_markdown_chars_reads_validated_option() {
    let mut values = MetadataMap::new();
    values.insert("min_markdown_chars".to_string(), json!(42));
    assert_eq!(min_markdown_chars(&values), 42);
}

#[test]
fn build_discovery_config_applies_defaults_when_no_options_set() {
    let plan = plan_with_options(MetadataMap::new());
    let cfg = build_discovery_config(&plan, std::env::temp_dir());

    assert!(!cfg.embed);
    assert_eq!(cfg.render_mode, axon_core::config::RenderMode::AutoSwitch);
}

#[test]
fn build_discovery_config_honors_crawl_options() {
    let mut values = MetadataMap::new();
    values.insert("render_mode".to_string(), json!("chrome"));
    values.insert("max_pages".to_string(), json!(25));
    values.insert("max_depth".to_string(), json!(3));
    values.insert("include_subdomains".to_string(), json!(true));
    values.insert("discover_sitemaps".to_string(), json!(false));
    values.insert("url_whitelist".to_string(), json!(["^https://example\\.com/docs"]));
    values.insert("url_blacklist".to_string(), json!(["/blocked"]));
    let plan = plan_with_options(values);

    let cfg = build_discovery_config(&plan, std::env::temp_dir());

    assert_eq!(cfg.render_mode, axon_core::config::RenderMode::Chrome);
    assert_eq!(cfg.max_pages, 25);
    assert_eq!(cfg.max_depth, 3);
    assert!(cfg.include_subdomains);
    assert!(!cfg.discover_sitemaps);
    assert_eq!(
        cfg.url_whitelist,
        vec!["^https://example\\.com/docs".to_string()]
    );
    assert!(
        cfg.exclude_path_prefix
            .iter()
            .any(|prefix| prefix == "/blocked")
    );
}
