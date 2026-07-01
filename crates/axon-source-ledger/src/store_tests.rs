use crate::{
    CleanupDebtItem, ManifestItem, RefreshPreflight, SourceIdentity, SourceKind, SourceLedgerStore,
};
use std::collections::BTreeSet;

#[tokio::test]
async fn diff_manifest_reports_added_modified_removed_and_unchanged() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    store
        .record_manifest_item("source-a", 1, ManifestItem::new("src/lib.rs", "hash-a", 10))
        .await
        .unwrap();
    store
        .record_manifest_item(
            "source-a",
            1,
            ManifestItem::new("README.md", "hash-readme", 10),
        )
        .await
        .unwrap();
    store.commit_generation("source-a", 1).await.unwrap();

    let manifest = vec![
        ManifestItem::new("src/lib.rs", "hash-b", 11),
        ManifestItem::new("src/main.rs", "hash-c", 12),
        ManifestItem::new("README.md", "hash-readme", 10),
    ];
    let diff = store.diff_manifest("source-a", &manifest).await.unwrap();

    assert_eq!(diff.modified[0].item_key, "src/lib.rs");
    assert_eq!(diff.added[0].item_key, "src/main.rs");
    assert_eq!(diff.removed[0].item_key, "src/lib.rs");
    assert_eq!(diff.removed[0].indexed_generation, 1);
    assert!(!diff.removed.iter().any(|item| item.item_key == "README.md"));
}

#[tokio::test]
async fn failed_pending_generation_preserves_committed_baseline() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    store
        .record_manifest_item("source-a", 1, ManifestItem::new("src/lib.rs", "hash-a", 10))
        .await
        .unwrap();
    store.commit_generation("source-a", 1).await.unwrap();
    store
        .record_manifest_item("source-a", 2, ManifestItem::new("src/lib.rs", "hash-b", 11))
        .await
        .unwrap();

    let diff = store
        .diff_manifest("source-a", &[ManifestItem::new("src/lib.rs", "hash-a", 10)])
        .await
        .unwrap();

    assert!(diff.added.is_empty());
    assert!(diff.modified.is_empty());
    assert!(diff.removed.is_empty());
}

#[tokio::test]
async fn ownerless_generation_paths_reject_active_lease() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );

    let begin_err = store.begin_generation(&source).await.unwrap_err();
    assert!(
        begin_err.to_string().contains("lease"),
        "ownerless begin should fail while active lease exists: {begin_err}"
    );

    store
        .record_manifest_item("source-a", 1, ManifestItem::new("src/lib.rs", "hash-a", 10))
        .await
        .unwrap();
    let commit_err = store.commit_generation("source-a", 1).await.unwrap_err();
    assert!(
        commit_err.to_string().contains("active lease"),
        "ownerless commit should fail while active lease exists: {commit_err}"
    );
}

#[tokio::test]
async fn committed_generation_item_count_tracks_current_committed_rows() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    assert_eq!(
        store
            .committed_generation_item_count("source-a")
            .await
            .unwrap(),
        0
    );

    let generation = store.begin_generation(&source).await.unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation,
            ManifestItem::new("src/lib.rs", "hash-a", 10),
        )
        .await
        .unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation,
            ManifestItem::new("README.md", "hash-b", 10),
        )
        .await
        .unwrap();
    store
        .commit_generation("source-a", generation)
        .await
        .unwrap();

    assert_eq!(
        store
            .committed_generation_item_count("source-a")
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn stale_generation_commit_cannot_publish_after_newer_generation_exists() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();

    let generation_1 = store.begin_generation(&source).await.unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation_1,
            ManifestItem::new("src/lib.rs", "hash-a", 10),
        )
        .await
        .unwrap();
    store
        .commit_generation("source-a", generation_1)
        .await
        .unwrap();

    let stale_generation = store.begin_generation(&source).await.unwrap();
    store
        .record_manifest_item(
            "source-a",
            stale_generation,
            ManifestItem::new("src/lib.rs", "hash-stale", 10),
        )
        .await
        .unwrap();
    let newer_generation = store.begin_generation(&source).await.unwrap();
    store
        .record_manifest_item(
            "source-a",
            newer_generation,
            ManifestItem::new("src/lib.rs", "hash-newer", 10),
        )
        .await
        .unwrap();
    store
        .commit_generation("source-a", newer_generation)
        .await
        .unwrap();

    let err = store
        .commit_generation("source-a", stale_generation)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("stale"),
        "stale generation should fail clearly: {err}"
    );
    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        newer_generation
    );
    let diff = store
        .diff_manifest(
            "source-a",
            &[ManifestItem::new("src/lib.rs", "hash-newer", 10)],
        )
        .await
        .unwrap();
    assert_eq!(diff, Default::default());
}

#[tokio::test]
async fn first_generation_commit_requires_manifest_state() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    let generation = store.begin_generation(&source).await.unwrap();

    let err = store
        .commit_generation("source-a", generation)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("manifest"),
        "missing manifest should fail clearly: {err}"
    );
    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        0
    );
}

#[tokio::test]
async fn explicit_empty_first_generation_payload_can_commit() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();

    store
        .commit_generation_payload_for_owner("source-a", generation, "owner-a", &[], &[])
        .await
        .unwrap();

    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        generation
    );
}

#[tokio::test]
async fn explicit_empty_first_generation_delta_can_commit() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Crawl, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();

    store
        .commit_generation_delta_for_owner(
            "source-a",
            generation,
            "owner-a",
            &[],
            &BTreeSet::new(),
            &[],
        )
        .await
        .unwrap();

    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        generation
    );
}

#[tokio::test]
async fn preflight_backoff_blocks_generation_allocation() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    store
        .set_backoff("source-a", 10_000, "qdrant", "connection refused")
        .await
        .unwrap();

    assert!(matches!(
        store.preflight_refresh("source-a", 1_000).await.unwrap(),
        RefreshPreflight::BackingOff { .. }
    ));
    assert_eq!(store.max_generation("source-a").await.unwrap(), 0);
}

#[tokio::test]
async fn owner_guarded_paths_reject_expired_lease() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    assert!(store.acquire_lease(&source, "owner-a", 0).await.unwrap());

    let extend_err = store
        .extend_lease_for_owner("source-a", "owner-a", 60_000)
        .await
        .unwrap_err();
    assert!(
        extend_err.to_string().contains("expired"),
        "expired extend should fail clearly: {extend_err}"
    );

    let begin_err = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap_err();
    assert!(
        begin_err.to_string().contains("expired"),
        "expired generation allocation should fail clearly: {begin_err}"
    );
}

#[tokio::test]
async fn owner_guarded_commit_fails_after_lease_is_lost() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    assert!(store.acquire_lease(&source, "owner-a", 1).await.unwrap());
    store.release_lease("source-a", "owner-a").await.unwrap();
    assert!(
        store
            .acquire_lease(&source, "owner-b", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-b")
        .await
        .unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation,
            ManifestItem::new("src/lib.rs", "hash", 10),
        )
        .await
        .unwrap();

    let err = store
        .commit_generation_for_owner("source-a", generation, "owner-a")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("lease"),
        "owner mismatch must fail clearly: {err}"
    );
    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        0
    );
}

#[tokio::test]
async fn owner_guarded_commit_rejects_expired_lease() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation,
            ManifestItem::new("src/lib.rs", "hash", 10),
        )
        .await
        .unwrap();
    store.release_lease("source-a", "owner-a").await.unwrap();
    assert!(store.acquire_lease(&source, "owner-a", 0).await.unwrap());

    let err = store
        .commit_generation_for_owner("source-a", generation, "owner-a")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("expired"),
        "expired commit should fail clearly: {err}"
    );
    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        0
    );
}

#[tokio::test]
async fn release_lease_fails_when_owner_is_lost() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );

    let err = store
        .release_lease("source-a", "owner-b")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("no longer owned"),
        "lost ownership should be visible: {err}"
    );
}

#[tokio::test]
async fn payload_commit_atomically_commits_manifest_and_cleanup_debt() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();

    store
        .commit_generation_payload_for_owner(
            "source-a",
            generation,
            "owner-a",
            &[ManifestItem::new("src/lib.rs", "hash-a", 10)],
            &[CleanupDebtItem::new(
                3,
                "src/old.rs",
                r#"{"collection":"axon","source_id":"source-a","source_index_version":1,"source_generation":3,"item_key":"src/old.rs"}"#,
            )],
        )
        .await
        .unwrap();

    let status = store.source_status("source-a").await.unwrap();
    assert_eq!(status.committed_generation, generation);
    assert_eq!(status.cleanup_debt_count, 1);
    let diff = store
        .diff_manifest("source-a", &[ManifestItem::new("src/lib.rs", "hash-a", 10)])
        .await
        .unwrap();
    assert_eq!(diff, Default::default());
}

#[tokio::test]
async fn payload_commit_rejects_incomplete_cleanup_selector() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();

    let err = store
        .commit_generation_payload_for_owner(
            "source-a",
            generation,
            "owner-a",
            &[ManifestItem::new("src/lib.rs", "hash-a", 10)],
            &[CleanupDebtItem::new(
                generation,
                "src/old.rs",
                r#"{"kind":"source_cleanup_v1"}"#,
            )],
        )
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("missing source_id"),
        "incomplete cleanup selector should fail clearly: {err}"
    );
    assert_eq!(store.cleanup_debt_count("source-a").await.unwrap(), 0);
    assert_eq!(
        store
            .source_status("source-a")
            .await
            .unwrap()
            .committed_generation,
        0
    );
}

#[tokio::test]
async fn delta_commit_preserves_unchanged_item_generation_for_later_cleanup() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("crawl-source", SourceKind::Crawl, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );

    let generation_1 = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();
    store
        .commit_generation_payload_for_owner(
            "crawl-source",
            generation_1,
            "owner-a",
            &[
                ManifestItem::new("https://example.com/a", "hash-a1", 10),
                ManifestItem::new("https://example.com/b", "hash-b1", 20),
            ],
            &[],
        )
        .await
        .unwrap();

    let generation_2 = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();
    let live_keys = BTreeSet::from([
        "https://example.com/a".to_string(),
        "https://example.com/b".to_string(),
    ]);
    store
        .commit_generation_delta_for_owner(
            "crawl-source",
            generation_2,
            "owner-a",
            &[ManifestItem::new("https://example.com/a", "hash-a2", 11)],
            &live_keys,
            &[],
        )
        .await
        .unwrap();

    let diff = store
        .diff_manifest(
            "crawl-source",
            &[ManifestItem::new("https://example.com/a", "hash-a2", 11)],
        )
        .await
        .unwrap();

    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].item_key, "https://example.com/b");
    assert_eq!(diff.removed[0].indexed_generation, generation_1);
}

#[tokio::test]
async fn abort_generation_removes_pending_rows_and_restores_max_generation() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:")
        .await
        .unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::Git, "axon", 1);
    assert!(
        store
            .acquire_lease(&source, "owner-a", 60_000)
            .await
            .unwrap()
    );
    let generation = store
        .begin_generation_for_owner(&source, "owner-a")
        .await
        .unwrap();
    store
        .record_manifest_item(
            "source-a",
            generation,
            ManifestItem::new("src/lib.rs", "hash-a", 10),
        )
        .await
        .unwrap();

    store
        .abort_generation_for_owner("source-a", generation, "owner-a")
        .await
        .unwrap();

    let status = store.source_status("source-a").await.unwrap();
    assert_eq!(status.committed_generation, 0);
    assert_eq!(status.active_generation, None);
    assert_eq!(store.max_generation("source-a").await.unwrap(), 0);
    let diff = store
        .diff_manifest("source-a", &[ManifestItem::new("src/lib.rs", "hash-a", 10)])
        .await
        .unwrap();
    assert_eq!(diff.added[0].item_key, "src/lib.rs");
}
