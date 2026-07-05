use axon_api::source::*;
use uuid::Uuid;

use crate::{AdapterCapability, FakeSourceAdapter, SourceAdapter, SourceAdapterRegistry};

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
    assert_eq!(capability.adapter.name, "local");

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

    assert_eq!(capability.adapter.name, "web");
    assert_eq!(capability.source_kind, SourceKind::Web);
    assert_eq!(capability.default_scope, SourceScope::Site);
    assert!(capability.scopes.contains(&SourceScope::Page));
    assert!(capability.watch_supported);
    assert!(capability.refresh_supported);
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
