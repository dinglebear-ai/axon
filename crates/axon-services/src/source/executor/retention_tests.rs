use super::*;
use axon_adapters::{FakeSourceAdapter, SourceAdapter};
use axon_ledger::store::FakeLedgerStore;

fn status(key: &str, chunks: u32, skipped: bool) -> DocumentStatus {
    DocumentStatus {
        document_id: DocumentId::new(format!("doc-{key}")),
        source_id: SourceId::new("source"),
        source_item_key: SourceItemKey::new(key),
        generation: Some(SourceGenerationId::new("committed")),
        status: if skipped {
            DocumentLifecycleStatus::Skipped
        } else {
            DocumentLifecycleStatus::Published
        },
        updated_at: Timestamp("2026-09-27T00:00:00Z".into()),
        chunk_count: chunks,
        vector_point_count: chunks,
        error: None,
        cleanup_status: None,
    }
}

async fn fixture() -> (FakeLedgerStore, SourceManifest, SourceManifestDiff) {
    let ledger = FakeLedgerStore::new();
    let adapter = FakeSourceAdapter::new(AdapterRef {
        name: "local".into(),
        version: "test".into(),
    })
    .with_item("large", ContentKind::Markdown, "body")
    .with_item("small", ContentKind::Markdown, "body");
    let request = SourceRequest::local_path("/tmp/retention", true);
    let plan = crate::source::routing::resolve_source_route(&request).unwrap();
    let plan = SourcePlan {
        job_id: JobId::new(uuid::Uuid::nil()),
        request,
        route: plan.route,
        limits: EffectiveLimits {
            request: Default::default(),
            adapter_defaults: Default::default(),
            config_defaults: Default::default(),
            effective: Default::default(),
        },
        stage_plan: Vec::new(),
        config_snapshot_id: ConfigSnapshotId::new("test"),
        provider_reservations: Vec::new(),
    };
    let manifest = adapter.discover(&plan).await.unwrap();
    ledger
        .upsert_source(SourceSummary {
            source_id: manifest.source_id.clone(),
            canonical_uri: "/tmp/retention".into(),
            display_name: "retention".into(),
            source_kind: SourceKind::Local,
            adapter: manifest.adapter.clone(),
            authority: AuthorityLevel::UserPinned,
            status: LifecycleStatus::Running,
            counts: super::super::helpers::empty_source_counts(),
            created_at: super::super::timestamp(),
            updated_at: super::super::timestamp(),
            tags: Vec::new(),
            watch_id: None,
            graph_node_ids: Vec::new(),
            last_job_id: None,
            last_refreshed_at: None,
            user_label: None,
        })
        .await
        .unwrap();
    let mut diff = ledger.diff_manifest(manifest.clone()).await.unwrap();
    diff.previous_generation = Some(SourceGenerationId::new("committed"));
    diff.unchanged = std::mem::take(&mut diff.added);
    diff.counts.unchanged = diff.unchanged.len() as u64;
    diff.counts.added = 0;
    (ledger, manifest, diff)
}

#[tokio::test]
async fn partial_git_refresh_drops_previously_indexed_non_code_items() {
    let (_, mut manifest, _) = fixture().await;
    let mut request = SourceRequest::new("https://github.com/pallets/flask".to_string());
    request
        .options
        .values
        .insert("exclude_paths".to_string(), serde_json::json!(["vendor/"]));
    let route = crate::source::routing::resolve_source_route(&request).unwrap();
    let plan = SourcePlan {
        job_id: JobId::new(uuid::Uuid::nil()),
        request,
        route: route.route,
        limits: EffectiveLimits {
            request: Default::default(),
            adapter_defaults: Default::default(),
            config_defaults: Default::default(),
            effective: Default::default(),
        },
        stage_plan: Vec::new(),
        config_snapshot_id: ConfigSnapshotId::new("test"),
        provider_reservations: Vec::new(),
    };
    let template = manifest.items[0].clone();
    let mut prior = manifest.clone();
    prior.items = [
        "assets/logo.png",
        "Cargo.lock",
        "src/lib.rs",
        "docs/guide.md",
        "vendor/dep.rs",
    ]
    .into_iter()
    .map(|path| {
        let mut item = template.clone();
        item.source_item_key = SourceItemKey::new(path);
        item.display_path = Some(path.to_string());
        item
    })
    .collect();
    manifest.items.clear();
    let existing = ["src/lib.rs", "docs/guide.md"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let unvisited = retain_unvisited(&plan, &mut manifest, prior, Some(&existing)).unwrap();
    let paths: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();
    assert_eq!(paths, ["src/lib.rs", "docs/guide.md"]);
    assert_eq!(unvisited.len(), 2);
}

#[tokio::test]
async fn retained_totals_use_uneven_document_counts_and_skips() {
    let (_, manifest, diff) = fixture().await;
    let retained = [status("small", 2, false), status("binary", 0, true)];
    let totals = source_counts(&manifest, &diff, &retained, &Default::default());
    assert_eq!(totals.chunks_total, 2);
    assert_eq!(totals.vector_points_total, 2);
    assert_eq!(totals.documents_total, 1);
    assert_eq!(totals.documents_skipped, 1);
}

#[tokio::test]
async fn failed_generation_sibling_forces_whole_visited_item_to_reprepare() {
    let (ledger, _, mut diff) = fixture().await;
    let item = diff.unchanged[0].clone();
    let mut valid = status("valid", 100, false);
    valid.source_id = diff.source_id.clone();
    valid.source_item_key = item.source_item_key.clone();
    let mut failed = valid.clone();
    failed.document_id = DocumentId::new("sibling");
    failed.generation = Some(SourceGenerationId::new("failed-attempt"));
    ledger
        .update_document_statuses(vec![valid, failed])
        .await
        .unwrap();
    validate_retained(&ledger, &mut diff, &BTreeSet::new())
        .await
        .unwrap();
    assert!(
        diff.modified
            .iter()
            .any(|row| row.source_item_key == item.source_item_key)
    );
    assert!(diff.unchanged.is_empty());
}

#[tokio::test]
async fn missing_unvisited_provenance_rejects_partial_refresh() {
    let (ledger, _, mut diff) = fixture().await;
    let unvisited = diff
        .unchanged
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let error = validate_retained(&ledger, &mut diff, &unvisited)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("complete refresh required"));
}

#[tokio::test]
async fn central_cap_changes_completeness_only_when_it_truncates() {
    let (_, mut manifest, _) = fixture().await;
    manifest.set_inventory_completeness(InventoryCompleteness::Complete);
    super::super::helpers::apply_max_items(&mut manifest, Some(2));
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Complete
    );
    super::super::helpers::apply_max_items(&mut manifest, Some(1));
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Partial
    );
    super::super::helpers::apply_max_items(&mut manifest, None);
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Partial
    );
}

#[tokio::test]
async fn partial_git_refresh_requires_complete_prior_alias_inventory() {
    let (_, mut prior, _) = fixture().await;
    prior.items[0]
        .metadata
        .insert("source_item_aliases".into(), serde_json::json!(["copy.md"]));
    let error = require_complete_alias_inventory(true, &prior).unwrap_err();
    assert!(error.to_string().contains("max_items unset"));
    // Other source adapters do not use repository alias acquisition.
    require_complete_alias_inventory(false, &prior).unwrap();
    prior.items[0].metadata.remove("source_item_aliases");
    require_complete_alias_inventory(true, &prior).unwrap();
}
