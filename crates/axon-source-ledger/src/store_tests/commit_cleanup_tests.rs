use crate::{CleanupDebtItem, ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore};
use std::collections::BTreeSet;

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
async fn repeated_owner_payload_commit_rejects_already_committed_generation() {
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
            &[],
        )
        .await
        .unwrap();

    let err = store
        .commit_generation_payload_for_owner(
            "source-a",
            generation,
            "owner-a",
            &[ManifestItem::new("src/lib.rs", "hash-b", 11)],
            &[],
        )
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("stale"),
        "already committed generation should fail clearly: {err}"
    );
    let diff = store
        .diff_manifest("source-a", &[ManifestItem::new("src/lib.rs", "hash-a", 10)])
        .await
        .unwrap();
    assert_eq!(diff, Default::default());
}

#[tokio::test]
async fn repeated_owner_delta_commit_rejects_already_committed_generation() {
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
            &[],
        )
        .await
        .unwrap();

    let err = store
        .commit_generation_delta_for_owner(
            "source-a",
            generation,
            "owner-a",
            &[ManifestItem::new("src/lib.rs", "hash-b", 11)],
            &BTreeSet::from(["src/lib.rs".to_string()]),
            &[],
        )
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("stale"),
        "already committed generation should fail clearly: {err}"
    );
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
