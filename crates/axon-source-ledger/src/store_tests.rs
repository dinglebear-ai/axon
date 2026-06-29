use crate::{ManifestItem, RefreshPreflight, SourceIdentity, SourceKind, SourceLedgerStore};

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
    store.commit_generation("source-a", 1).await.unwrap();

    let manifest = vec![
        ManifestItem::new("src/lib.rs", "hash-b", 11),
        ManifestItem::new("src/main.rs", "hash-c", 12),
    ];
    let diff = store.diff_manifest("source-a", &manifest).await.unwrap();

    assert_eq!(diff.modified[0].item_key, "src/lib.rs");
    assert_eq!(diff.added[0].item_key, "src/main.rs");
    assert_eq!(diff.removed, vec!["src/lib.rs".to_string()]);
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
