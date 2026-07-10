use std::sync::Arc;

use axon_api::source::*;
use uuid::Uuid;

use crate::feed::FeedSourceAdapter;
use crate::git::GitSourceAdapter;
use crate::local::LocalSourceAdapter;
use crate::reddit::RedditSourceAdapter;
use crate::registry_sources::RegistrySourceAdapter;
use crate::sessions::SessionSourceAdapter;
use crate::web::WebSourceAdapter;
use crate::youtube::YoutubeSourceAdapter;
use crate::{
    AdapterCapability, FakeSourceAdapter, FakeSourceAdapterMode, SourceAdapter,
    SourceAdapterRegistry,
};

#[tokio::test]
async fn fake_source_adapter_acquires_manifest_items_and_documents_without_preparing() {
    let route = route_plan("local", SourceKind::Local, SourceScope::Directory);
    let adapter = FakeSourceAdapter::new(route.adapter.clone()).with_item(
        "README.md",
        ContentKind::Markdown,
        "# Axon",
    );
    let plan = source_plan(route);

    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "local");

    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.generation, SourceGenerationId::from("gen_fake"));
    assert_eq!(manifest.items.len(), 1);

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(acquisition.source_id, SourceId::from("src_local"));
    assert_eq!(acquisition.generation, SourceGenerationId::from("gen_fake"));
    assert_eq!(acquisition.adapter.name, "local");
    assert_eq!(acquisition.scope, SourceScope::Directory);
    assert_eq!(acquisition.manifest.items.len(), 1);
    assert_eq!(
        acquisition.manifest.items[0].source_item_key,
        SourceItemKey::from("README.md")
    );
    assert_eq!(
        acquisition.manifest.items[0].canonical_uri,
        "local://lp_test/README.md"
    );
    assert_eq!(acquisition.fetched_items.len(), 1);
    assert_eq!(
        acquisition.fetched_items[0].manifest_item,
        acquisition.manifest.items[0]
    );
    assert!(matches!(
        acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText { .. }
    ));

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.header.phase, PipelinePhase::Normalizing);
    let documents = normalized.data;
    assert_eq!(documents.len(), 1);
    assert_eq!(
        documents[0].document_id,
        DocumentId::from("doc_src_local_README_md")
    );
    assert_eq!(documents[0].source_id, SourceId::from("src_local"));
    assert_eq!(
        documents[0].source_item_key,
        SourceItemKey::from("README.md")
    );
    assert_eq!(documents[0].content_kind, ContentKind::Markdown);
    assert_eq!(documents[0].chunk_hints.len(), 1);
    assert_eq!(documents[0].parser_hints.len(), 1);
}

#[tokio::test]
async fn source_adapter_registry_routes_by_selected_adapter_and_reports_capabilities() {
    let local = FakeSourceAdapter::new(AdapterRef {
        name: "local".to_string(),
        version: "test".to_string(),
    })
    .with_scope(SourceScope::Directory)
    .with_scope(SourceScope::File);
    let web = FakeSourceAdapter::new(AdapterRef {
        name: "web".to_string(),
        version: "test".to_string(),
    })
    .with_scope(SourceScope::Site)
    .with_scope(SourceScope::Page);
    let registry = SourceAdapterRegistry::from_adapters(vec![local, web]);
    let route = route_plan("web", SourceKind::Web, SourceScope::Site);

    let adapter = registry
        .adapter_for(&route)
        .expect("selected adapter is registered");
    let capability = adapter.capabilities().await.unwrap();

    assert_eq!(capability.0.name, "web");
    assert_eq!(capability.0.owner_crate, "axon-adapters");
    assert_eq!(capability.0.health, HealthStatus::Healthy);
    assert!(capability.0.features.contains(&"scope:site".to_string()));
    assert!(capability.0.features.contains(&"scope:page".to_string()));
    assert!(capability.0.features.contains(&"watch".to_string()));
    assert!(capability.0.features.contains(&"refresh".to_string()));
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Web))
    );
    assert_eq!(
        capability.0.limits.0.get("default_scope"),
        Some(&serde_json::json!(SourceScope::Site))
    );
}

#[tokio::test]
async fn source_adapter_registry_accepts_mixed_trait_objects() {
    let local = FakeSourceAdapter::new(AdapterRef {
        name: "local".to_string(),
        version: "test".to_string(),
    });
    let web = FakeSourceAdapter::new(AdapterRef {
        name: "web".to_string(),
        version: "test".to_string(),
    });
    let registry = SourceAdapterRegistry::from_arc_adapters(vec![
        std::sync::Arc::new(local) as std::sync::Arc<dyn SourceAdapter>,
        std::sync::Arc::new(web) as std::sync::Arc<dyn SourceAdapter>,
    ]);

    assert!(
        registry
            .adapter_for(&route_plan(
                "local",
                SourceKind::Local,
                SourceScope::Directory
            ))
            .is_some()
    );
    assert!(
        registry
            .adapter_for(&route_plan("web", SourceKind::Web, SourceScope::Site))
            .is_some()
    );
}

#[tokio::test]
async fn fake_source_adapter_preserves_content_for_normalized_absolute_item_keys() {
    let route = route_plan("local", SourceKind::Local, SourceScope::Directory);
    let adapter = FakeSourceAdapter::new(route.adapter.clone()).with_item(
        "/home/jmagar/workspace/axon/src/main.rs",
        ContentKind::Code,
        "fn main() {}",
    );
    let plan = source_plan(route);

    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(
        manifest.items[0].source_item_key,
        SourceItemKey::from("src/main.rs")
    );

    let diff = manifest_diff(&plan, manifest.items);
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();

    assert_eq!(
        acquisition.fetched_items[0].content_ref,
        ContentRef::InlineText {
            text: "fn main() {}".to_string()
        }
    );
}

#[test]
fn adapter_capability_rejects_unsupported_scope_before_acquisition() {
    let capability = AdapterCapability::new(
        AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        SourceKind::Web,
        SourceScope::Site,
    );

    let err = capability
        .validate_scope(SourceScope::Repo)
        .expect_err("unsupported scope fails before acquisition");

    assert_eq!(err.code.0, "adapter.scope.unsupported");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

/// Every production `SourceAdapter` implementor coerces to `Arc<dyn
/// SourceAdapter>` without changing call sites — the load-bearing
/// compatibility check for the trait-signature edit in `adapter.rs`
/// (`&str` -> `&'static str`, `AdapterCapability` -> `SourceAdapterCapability`).
#[test]
fn every_production_adapter_satisfies_source_adapter_as_trait_object() {
    let adapters: Vec<Arc<dyn SourceAdapter>> = vec![
        Arc::new(WebSourceAdapter::new()),
        Arc::new(GitSourceAdapter::new()),
        Arc::new(FeedSourceAdapter::new()),
        Arc::new(SessionSourceAdapter::new()),
        Arc::new(YoutubeSourceAdapter::new()),
        Arc::new(RedditSourceAdapter::new()),
        Arc::new(LocalSourceAdapter::new()),
        Arc::new(RegistrySourceAdapter::new()),
        Arc::new(FakeSourceAdapter::new(AdapterRef {
            name: "fake".to_string(),
            version: "test".to_string(),
        })),
    ];

    let names: Vec<&'static str> = adapters.iter().map(|adapter| adapter.name()).collect();
    assert_eq!(
        names,
        vec![
            "web", "git", "feed", "session", "youtube", "reddit", "local", "registry", "fake",
        ]
    );
    for adapter in &adapters {
        assert!(!adapter.version().is_empty());
    }
}

#[tokio::test]
async fn every_production_adapter_reports_capabilities_via_source_adapter_capability() {
    let adapters: Vec<Arc<dyn SourceAdapter>> = vec![
        Arc::new(WebSourceAdapter::new()),
        Arc::new(GitSourceAdapter::new()),
        Arc::new(FeedSourceAdapter::new()),
        Arc::new(SessionSourceAdapter::new()),
        Arc::new(YoutubeSourceAdapter::new()),
        Arc::new(RedditSourceAdapter::new()),
        Arc::new(LocalSourceAdapter::new()),
        Arc::new(RegistrySourceAdapter::new()),
    ];

    for adapter in &adapters {
        let capability = adapter.capabilities().await.unwrap();
        assert_eq!(capability.0.name, adapter.name());
        assert_eq!(capability.0.owner_crate, "axon-adapters");
        assert_eq!(capability.0.health, HealthStatus::Healthy);
        assert!(!capability.0.features.is_empty());
        assert!(capability.0.limits.0.contains_key("source_kind"));
    }
}

#[tokio::test]
async fn fake_source_adapter_failure_mode_fails_discover_acquire_normalize() {
    let route = route_plan("local", SourceKind::Local, SourceScope::Directory);
    let adapter = FakeSourceAdapter::new(route.adapter.clone())
        .with_item("README.md", ContentKind::Markdown, "# Axon")
        .with_mode(FakeSourceAdapterMode::Failure);
    let plan = source_plan(route);

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("failure mode fails discover");
    assert_eq!(err.code.0, "adapter.fake.failure");

    assert_eq!(adapter.calls(), vec!["discover"]);
}

#[tokio::test]
async fn fake_source_adapter_degraded_mode_emits_warnings_and_succeeds() {
    let route = route_plan("local", SourceKind::Local, SourceScope::Directory);
    let adapter = FakeSourceAdapter::new(route.adapter.clone())
        .with_item("README.md", ContentKind::Markdown, "# Axon")
        .with_mode(FakeSourceAdapterMode::Degraded);
    let plan = source_plan(route);

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items);
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert!(!acquisition.header.warnings.is_empty());
    assert_eq!(acquisition.header.warnings[0].severity, Severity::Degraded);

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert!(!normalized.header.warnings.is_empty());

    assert_eq!(adapter.calls(), vec!["discover", "acquire", "normalize"]);
}

#[tokio::test]
async fn fake_source_adapter_capability_override_replaces_reported_capability() {
    let route = route_plan("local", SourceKind::Local, SourceScope::Directory);
    let override_capability = AdapterCapability::new(
        AdapterRef {
            name: "local".to_string(),
            version: "override".to_string(),
        },
        SourceKind::Local,
        SourceScope::Directory,
    );
    let adapter =
        FakeSourceAdapter::new(route.adapter.clone()).with_capability_override(override_capability);

    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.version, "override");
}

fn source_plan(route: RoutePlan) -> SourcePlan {
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(1)),
        request: SourceRequest::new(route.source.canonical_uri.clone()),
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::from("cfg_test"),
        provider_reservations: Vec::new(),
    }
}

fn manifest_diff(plan: &SourcePlan, added: Vec<ManifestItem>) -> SourceManifestDiff {
    SourceManifestDiff {
        header: stage_header(plan.job_id, PipelinePhase::Diffing),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("gen_fake"),
        added,
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 1,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

fn stage_header(job_id: JobId, phase: PipelinePhase) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(Uuid::from_u128(2)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
        completed_at: Some(Timestamp("2026-07-01T00:00:01Z".to_string())),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn route_plan(adapter_name: &str, source_kind: SourceKind, scope: SourceScope) -> RoutePlan {
    RoutePlan {
        source: ResolvedSource {
            source: "local://redacted".to_string(),
            canonical_uri: match source_kind {
                SourceKind::Local => "local://lp_test".to_string(),
                SourceKind::Web => "https://example.com/".to_string(),
                _ => "source://test".to_string(),
            },
            source_id: SourceId::from(match source_kind {
                SourceKind::Local => "src_local",
                SourceKind::Web => "src_web",
                _ => "src_test",
            }),
            source_kind,
            adapter: AdapterRef {
                name: adapter_name.to_string(),
                version: "test".to_string(),
            },
            default_scope: scope,
            available_scopes: vec![scope],
            authority: AuthorityLevel::Inferred,
            confidence: 1.0,
            reason: "test source".to_string(),
            graph: Vec::new(),
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        },
        adapter: AdapterRef {
            name: adapter_name.to_string(),
            version: "test".to_string(),
        },
        scope,
        provider_requirements: Vec::new(),
        credential_requirements: Vec::new(),
        execution_affinity: ExecutionAffinity::Worker,
        safety_class: SafetyClass::LocalFilesystem,
        option_schema_id: format!("adapter:{adapter_name}:options:v1"),
        validated_options: AdapterOptions::default(),
        chunking_hints: vec![ChunkHint {
            profile: ChunkProfile::MarkdownSections,
            reason: "test chunk hint".to_string(),
            options: MetadataMap::new(),
        }],
        parser_hints: vec![ParserHint {
            parser_id: "markdown".to_string(),
            reason: "test parser hint".to_string(),
            options: MetadataMap::new(),
        }],
        graph_fact_kinds: vec!["source".to_string()],
        watch_supported: true,
        refresh_supported: true,
    }
}
