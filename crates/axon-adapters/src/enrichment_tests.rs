use axon_api::source::*;
use uuid::Uuid;

use super::{NoopSourceEnricher, SourceEnricher};
use crate::testing::{FakeSourceEnricher, FakeSourceEnricherMode};

fn source_plan() -> SourcePlan {
    let source = ResolvedSource {
        source: "local://redacted".to_string(),
        canonical_uri: "local://lp_test".to_string(),
        source_id: SourceId::from("src_local"),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        default_scope: SourceScope::Directory,
        available_scopes: vec![SourceScope::Directory],
        authority: AuthorityLevel::Inferred,
        confidence: 1.0,
        reason: "test source".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };
    let route = RoutePlan {
        source: source.clone(),
        adapter: source.adapter.clone(),
        scope: SourceScope::Directory,
        provider_requirements: Vec::new(),
        credential_requirements: Vec::new(),
        execution_affinity: ExecutionAffinity::Worker,
        safety_class: SafetyClass::LocalFilesystem,
        option_schema_id: "adapter:local:options:v1".to_string(),
        validated_options: AdapterOptions::default(),
        chunking_hints: Vec::new(),
        parser_hints: Vec::new(),
        graph_fact_kinds: Vec::new(),
        watch_supported: false,
        refresh_supported: true,
    };
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

fn acquired_item(key: &str) -> AcquiredSourceItem {
    AcquiredSourceItem {
        manifest_item: ManifestItem {
            source_id: SourceId::from("src_local"),
            source_item_key: SourceItemKey::from(key),
            canonical_uri: format!("local://lp_test/{key}"),
            item_kind: ItemKind::LocalFile,
            content_kind: Some(ContentKind::Markdown),
            display_path: Some(key.to_string()),
            parent_key: None,
            size_bytes: Some(4),
            content_hash: None,
            mtime: None,
            version: None,
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        },
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineText {
            text: "# Axon".to_string(),
        },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn noop_enricher_reports_not_needed_for_every_item() {
    let plan = source_plan();
    let item = acquired_item("README.md");
    let enricher = NoopSourceEnricher::new();

    let enrichment = enricher.enrich(&plan, &item).await.unwrap();

    assert_eq!(enrichment.source_id, plan.route.source.source_id);
    assert_eq!(
        enrichment.source_item_key,
        item.manifest_item.source_item_key
    );
    assert_eq!(enrichment.enrichment_kind, EnrichmentKind::None);
    assert_eq!(enrichment.status, EnrichmentStatus::NotNeeded);
    assert!(enrichment.parse_hints.is_empty());
    assert!(enrichment.chunk_hints.is_empty());
    assert!(enrichment.graph_candidates.is_empty());
    assert!(enrichment.artifacts.is_empty());
    assert!(enrichment.warnings.is_empty());
    assert_eq!(enrichment.header.phase, PipelinePhase::Enriching);
    assert_eq!(enrichment.header.status, LifecycleStatus::Completed);
}

#[tokio::test]
async fn noop_enricher_capabilities_report_healthy() {
    let enricher = NoopSourceEnricher::new();

    let capability = enricher.capabilities().await.unwrap();

    assert_eq!(capability.0.owner_crate, "axon-adapters");
    assert_eq!(capability.0.health, HealthStatus::Healthy);
}

#[tokio::test]
async fn noop_enricher_is_deterministic_across_calls() {
    let plan = source_plan();
    let item = acquired_item("README.md");
    let enricher = NoopSourceEnricher::new();

    let first = enricher.enrich(&plan, &item).await.unwrap();
    let second = enricher.enrich(&plan, &item).await.unwrap();

    assert_eq!(first.source_id, second.source_id);
    assert_eq!(first.source_item_key, second.source_item_key);
    assert_eq!(first.enrichment_kind, second.enrichment_kind);
    assert_eq!(first.status, second.status);
}

#[tokio::test]
async fn fake_enricher_success_mode_records_calls_and_returns_configured_result() {
    let plan = source_plan();
    let item = acquired_item("README.md");
    let enricher =
        FakeSourceEnricher::new().with_result(EnrichmentKind::Summary, EnrichmentStatus::Completed);

    let enrichment = enricher.enrich(&plan, &item).await.unwrap();

    assert_eq!(enrichment.enrichment_kind, EnrichmentKind::Summary);
    assert_eq!(enrichment.status, EnrichmentStatus::Completed);
    assert!(enrichment.warnings.is_empty());
    assert_eq!(
        enricher.calls(),
        vec![item.manifest_item.source_item_key.clone()]
    );
}

#[tokio::test]
async fn fake_enricher_failure_mode_returns_error() {
    let plan = source_plan();
    let item = acquired_item("README.md");
    let enricher = FakeSourceEnricher::new().with_mode(FakeSourceEnricherMode::Failure);

    let result = enricher.enrich(&plan, &item).await;

    assert!(result.is_err());
    // Failure mode still records the call before returning the error.
    assert_eq!(enricher.calls().len(), 1);
}

#[tokio::test]
async fn fake_enricher_degraded_mode_attaches_warning() {
    let plan = source_plan();
    let item = acquired_item("README.md");
    let enricher = FakeSourceEnricher::new().with_mode(FakeSourceEnricherMode::Degraded);

    let enrichment = enricher.enrich(&plan, &item).await.unwrap();

    assert_eq!(enrichment.warnings.len(), 1);
    assert_eq!(enrichment.warnings[0].code, "adapter.enrich.fake_degraded");
    assert_eq!(enrichment.header.warnings.len(), 1);
}

#[tokio::test]
async fn fake_enricher_capability_override_is_honored() {
    let override_capability = SourceEnricherCapability(CapabilityBase {
        name: "custom-enricher".to_string(),
        version: "9.9.9".to_string(),
        owner_crate: "axon-adapters".to_string(),
        health: HealthStatus::Degraded,
        features: vec!["custom".to_string()],
        limits: MetadataMap::new(),
    });
    let enricher = FakeSourceEnricher::new().with_capability_override(override_capability.clone());

    let capability = enricher.capabilities().await.unwrap();

    assert_eq!(capability, override_capability);
}
