use super::*;
use axon_api::source::{DocumentLifecycleStatus, SourceLimits};

struct LocalRefresh {
    root: tempfile::TempDir,
    runtime: TargetLocalSourceRuntime,
    ledger: Arc<FakeLedgerStore>,
    vectors: Arc<FakeVectorStore>,
}
impl LocalRefresh {
    fn new() -> Self {
        let ledger = Arc::new(FakeLedgerStore::new());
        let vectors = Arc::new(FakeVectorStore::new("refresh"));
        Self {
            root: tempfile::tempdir().unwrap(),
            runtime: test_runtime(vectors.clone(), ledger.clone()),
            ledger,
            vectors,
        }
    }
    async fn run(&self, max_items: Option<u64>) -> anyhow::Result<IndexCounts> {
        let source = self.root.path().to_string_lossy().into_owned();
        let request = SourceRequest::local_path(&source, true);
        let route = crate::source::routing::resolve_source_route(&request)?.route;
        let mut cfg = axon_core::config::Config::default();
        cfg.source_local_allowed_roots = vec![self.root.path().to_path_buf()];
        let auth = AuthSnapshot {
            auth_mode: axon_api::source::AuthMode::TrustedLocal,
            granted_scopes: vec![AuthScope::Read, AuthScope::Write, AuthScope::Local],
            ..Default::default()
        };
        dispatch_local(
            Arc::new(LocalSourceAdapter::new()),
            &cfg,
            &self.runtime,
            &source,
            "refresh",
            "refresh-owner",
            Some(&auth),
            true,
            &SourceLimits {
                max_items,
                ..Default::default()
            },
            &route,
            &test_execution(&source),
        )
        .await
    }
    fn write(&self, name: &str, body: impl AsRef<[u8]>) {
        std::fs::write(self.root.path().join(name), body).unwrap();
    }
}

#[tokio::test]
async fn partial_and_zero_scans_preserve_inventory_then_complete_scan_deletes() {
    let fixture = LocalRefresh::new();
    fixture.write("a.txt", "first source body. This document provides searchable details for inventory lifecycle verification.");
    fixture.write("b.txt", "second source body. This document provides searchable details for inventory lifecycle verification.");
    fixture.write("c.txt", b"\x00\x01\x02");
    let initial = fixture.run(None).await.unwrap();
    let partial = fixture.run(Some(1)).await.unwrap();
    let zero = fixture.run(Some(0)).await.unwrap();
    assert_eq!(partial.removed, 0);
    assert_eq!(zero.removed, 0);
    assert_eq!(zero.items_discovered, 3);
    assert_eq!(
        fixture
            .vectors
            .points("refresh")
            .await
            .iter()
            .filter(|point| point
                .payload
                .get("retired_epoch")
                .is_none_or(|epoch| epoch.is_null()))
            .count() as u64,
        initial.vector_points_written
    );
    std::fs::remove_file(fixture.root.path().join("a.txt")).unwrap();
    let removed = fixture.run(None).await.unwrap();
    assert_eq!(removed.removed, 1);
    let summary = fixture
        .ledger
        .get_source(removed.source_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.counts.documents_total, 1);
    assert_eq!(summary.counts.documents_skipped, 1);
    assert_eq!(
        summary.counts.vector_points_total,
        fixture
            .vectors
            .points("refresh")
            .await
            .iter()
            .filter(|point| point
                .payload
                .get("retired_epoch")
                .is_none_or(|epoch| epoch.is_null()))
            .count() as u64
    );
}

#[tokio::test]
async fn text_skip_text_and_unchanged_refresh_preserve_exact_status_counts() {
    let fixture = LocalRefresh::new();
    fixture.write("a.txt", "first text body. This document provides searchable details for inventory lifecycle verification.");
    fixture.write("b.txt", "retained text body. This document provides searchable details for inventory lifecycle verification.");
    fixture.run(None).await.unwrap();
    fixture.write("a.txt", b"\x00\x01\x02");
    let skipped = fixture.run(None).await.unwrap();
    let repeat = fixture.run(None).await.unwrap();
    assert_eq!(repeat.documents_prepared, 0);
    assert_eq!(skipped.documents_skipped, 1);
    let manifest = repeat.published_manifest.unwrap();
    let rows = fixture
        .ledger
        .document_statuses_for_items(
            repeat.source_id.clone(),
            manifest
                .items
                .iter()
                .map(|item| item.source_item_key.clone())
                .collect(),
        )
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row.status == DocumentLifecycleStatus::Skipped)
            .count(),
        1
    );
    assert!(
        rows.iter()
            .all(|row| row.generation.as_ref() == Some(&repeat.generation))
    );
    fixture.write("a.txt", "restored text body. This document provides searchable details for inventory lifecycle verification.");
    let restored = fixture.run(None).await.unwrap();
    let summary = fixture
        .ledger
        .get_source(restored.source_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.counts.documents_total, 2);
    assert_eq!(summary.counts.documents_skipped, 0);
    assert_eq!(
        summary.counts.vector_points_total,
        fixture
            .vectors
            .points("refresh")
            .await
            .iter()
            .filter(|point| point
                .payload
                .get("retired_epoch")
                .is_none_or(|epoch| epoch.is_null()))
            .count() as u64
    );
}

#[tokio::test]
async fn failed_status_provenance_forces_reprepare_and_rejects_unvisited_partial() {
    let fixture = LocalRefresh::new();
    fixture.write("a.txt", "first text body. This document provides searchable details for inventory lifecycle verification.");
    fixture.write("b.txt", "second text body. This document provides searchable details for inventory lifecycle verification.");
    let initial = fixture.run(None).await.unwrap();
    let manifest = initial.published_manifest.unwrap();
    let keys = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let mut rows = fixture
        .ledger
        .document_statuses_for_items(initial.source_id.clone(), keys)
        .await
        .unwrap();
    for row in &mut rows {
        row.generation = Some(SourceGenerationId::new("failed-attempt"));
        row.status = DocumentLifecycleStatus::Failed;
    }
    fixture.ledger.update_document_statuses(rows).await.unwrap();
    let error = fixture.run(Some(0)).await.unwrap_err();
    assert!(format!("{error:#}").contains("complete refresh required"));
    assert_eq!(
        fixture
            .ledger
            .committed_generation(&initial.source_id)
            .await,
        Some(initial.generation)
    );
    let restored = fixture.run(None).await.unwrap();
    assert_eq!(restored.documents_prepared, 2);
    let repeat = fixture.run(None).await.unwrap();
    assert_eq!(repeat.documents_prepared, 0);
}

#[tokio::test]
async fn complete_feed_refresh_still_removes_absent_entries() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let mut feed = server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("content-type", "application/rss+xml")
            .body(RSS_TWO_ITEMS);
    });
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("feed-refresh"));
    let runtime = test_runtime(vectors, ledger.clone());
    let source = server.url("/feed.xml");
    let route = route_for(&source);
    let execution = test_execution(&source);
    let run = || {
        dispatch_feed(
            Arc::new(FeedSourceAdapter::new()),
            &runtime,
            &source,
            "feed-refresh",
            "feed-owner",
            None,
            false,
            None,
            &route,
            &execution,
        )
    };
    let initial = run().await.unwrap();
    assert_eq!(initial.items_discovered, 2);
    feed.delete();
    let _updated = server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200).header("content-type", "application/rss+xml")
            .body(r#"<rss version="2.0"><channel><title>Example</title><link>https://example.com/</link><item><title>First Post</title><link>https://example.com/a</link><description>This feed article provides searchable details for complete inventory removal verification.</description><pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate></item></channel></rss>"#);
    });
    let refreshed = run().await.unwrap();
    assert_eq!(refreshed.removed, 1);
    assert_eq!(refreshed.items_discovered, 1);
    let summary = ledger
        .get_source(refreshed.source_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.counts.documents_total, 1);
}

#[tokio::test]
async fn incompatible_partial_refresh_rejects_then_complete_reprepares_every_item() {
    let fixture = LocalRefresh::new();
    fixture.write("a.txt", "first unchanged body. This document provides searchable details for inventory lifecycle verification.");
    fixture.write("b.txt", "second unchanged body. This document provides searchable details for inventory lifecycle verification.");
    let initial = fixture.run(None).await.unwrap();
    let mut manifest = initial.published_manifest.unwrap();
    manifest.metadata.insert(
        "axon_publication_config_snapshot_id".into(),
        "legacy-policy".into(),
    );
    fixture.ledger.put_manifest(manifest).await.unwrap();
    let error = fixture.run(Some(1)).await.unwrap_err();
    assert!(format!("{error:#}").contains("complete refresh required"));
    assert_eq!(
        fixture
            .ledger
            .committed_generation(&initial.source_id)
            .await,
        Some(initial.generation)
    );
    let fixed = fixture.run(None).await.unwrap();
    assert_eq!(fixed.documents_prepared, 2);
    let unchanged = fixture.run(None).await.unwrap();
    assert_eq!(unchanged.documents_prepared, 0);
    assert_eq!(unchanged.generation, fixed.generation);
    assert_eq!(
        unchanged.published_manifest.unwrap().generation,
        fixed.generation
    );
}

#[tokio::test]
async fn old_placeholder_is_retired_even_when_binary_manifest_is_unchanged() {
    use sha2::{Digest, Sha256};
    for scheduled in [false, true] {
        let mut fixture = LocalRefresh::new();
        fixture.runtime.embed_scheduler_enabled = scheduled;
        fixture.write(
            "asset.txt",
            "binary file placeholder from legacy preparation. This document provides searchable details for inventory lifecycle verification.",
        );
        let initial = fixture.run(None).await.unwrap();
        assert!(initial.vector_points_written > 0);
        let bytes = b"\x00\x01\x02";
        fixture.write("asset.txt", bytes);
        let mut manifest = initial.published_manifest.unwrap();
        // Model a legacy acquisition: its inventory describes raw binary bytes,
        // while the committed vector output contains synthetic placeholder text.
        manifest.items[0].content_hash = Some(format!("sha256:{:x}", Sha256::digest(bytes)));
        manifest.items[0].size_bytes = Some(bytes.len() as u64);
        manifest.items[0].mtime = Some(Timestamp(
            chrono::DateTime::<chrono::Utc>::from(
                std::fs::metadata(fixture.root.path().join("asset.txt"))
                    .unwrap()
                    .modified()
                    .unwrap(),
            )
            .to_rfc3339(),
        ));
        manifest.metadata.insert(
            "axon_publication_config_snapshot_id".into(),
            "legacy-policy".into(),
        );
        fixture.ledger.put_manifest(manifest).await.unwrap();
        let fixed = fixture.run(None).await.unwrap();
        assert_eq!(fixed.documents_prepared, 0);
        assert_eq!(fixed.documents_skipped, 1);
        assert!(
            fixture
                .vectors
                .points("refresh")
                .await
                .iter()
                .all(|point| !point.payload["retired_epoch"].is_null())
        );
        let repeated = fixture.run(None).await.unwrap();
        assert_eq!(repeated.generation, fixed.generation);
        assert_eq!(repeated.documents_prepared, 0);
    }
}

#[tokio::test]
async fn partial_policy_upgrade_is_safe_when_every_previous_item_was_visited() {
    let fixture = LocalRefresh::new();
    fixture.write("a.txt", "existing text body. This document provides searchable details for inventory lifecycle verification.");
    let initial = fixture.run(None).await.unwrap();
    let mut manifest = initial.published_manifest.unwrap();
    manifest.metadata.insert(
        "axon_publication_config_snapshot_id".into(),
        "legacy-policy".into(),
    );
    fixture.ledger.put_manifest(manifest).await.unwrap();
    fixture.write("b.txt", "new unvisited body has no previous output. This document provides searchable details for inventory lifecycle verification.");
    let partial = fixture.run(Some(1)).await.unwrap();
    assert_eq!(partial.documents_prepared, 1);
    assert_eq!(partial.items_discovered, 1);
    assert_eq!(
        partial.published_manifest.unwrap().inventory_completeness(),
        axon_api::source::InventoryCompleteness::Partial
    );
    let full = fixture.run(None).await.unwrap();
    assert_eq!(full.documents_prepared, 1);
    assert_eq!(full.items_discovered, 2);
}
