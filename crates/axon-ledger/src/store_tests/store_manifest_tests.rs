use super::*;

#[tokio::test]
async fn fake_ledger_diffs_manifests_and_tracks_committed_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let first = ledger.diff_manifest(manifest("a")).await.unwrap();
    assert_eq!(first.counts.added, 1);
    let first_manifest = manifest("a");
    ledger.put_manifest(first_manifest.clone()).await.unwrap();

    let generation = completed_generation_for_manifest(&first_manifest);
    let generation = complete_and_publish(&ledger, generation.clone()).await;
    assert_eq!(generation.publish_state, PublishState::Committed);
    assert!(generation.published_at.is_some());

    let refreshed = ledger.diff_manifest(manifest("b")).await.unwrap();
    assert_eq!(refreshed.counts.modified, 1);
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(generation.generation)
    );
}

#[tokio::test]
async fn fake_ledger_diffs_only_against_committed_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    ledger.put_manifest(manifest("uncommitted")).await.unwrap();

    let diff = ledger.diff_manifest(manifest("next")).await.unwrap();
    assert_eq!(diff.previous_generation, None);
    assert_eq!(diff.counts.added, 1);
    assert_eq!(diff.counts.modified, 0);
    assert_eq!(diff.counts.unchanged, 0);
}

#[tokio::test]
async fn fake_rejects_invalid_manifest_item_ownership_and_duplicates() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let mut wrong_source = manifest("wrong-source");
    wrong_source.items[0].source_id = SourceId::new("other");
    let error = ledger.put_manifest(wrong_source).await.unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "source.ledger.manifest_item_source_mismatch"
    );

    let mut duplicate = manifest_with_items(
        "gen_duplicate",
        vec![
            manifest_item("src/lib.rs", "a"),
            manifest_item("src/lib.rs", "b"),
        ],
    );
    duplicate.items[1].canonical_uri = "file:///repo/src/lib-copy.rs".to_string();
    let error = ledger.put_manifest(duplicate).await.unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "source.ledger.manifest_duplicate_item"
    );
}

#[tokio::test]
async fn fake_ledger_scopes_generation_ids_per_source() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let mut src_b = source();
    src_b.source_id = SourceId::new("src_b");
    ledger.upsert_source(src_b).await.unwrap();

    let src_a_first = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    let src_b_first = ledger
        .create_generation(SourceId::new("src_b"))
        .await
        .unwrap();
    assert_eq!(src_a_first.generation, SourceGenerationId::new("gen_1"));
    assert_eq!(src_b_first.generation, SourceGenerationId::new("gen_1"));

    ledger
        .put_manifest(manifest_for_generation(&src_a_first, "src-a-first"))
        .await
        .unwrap();
    complete_and_publish(&ledger, completed_generation(src_a_first.clone())).await;
    let src_a_second = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    assert_eq!(src_a_second.generation, SourceGenerationId::new("gen_2"));
    assert_eq!(
        src_a_second.previous_generation,
        Some(src_a_first.generation)
    );
}

#[tokio::test]
async fn fake_ledger_skips_manifest_created_generation_ids() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    ledger
        .put_manifest(manifest_with_items(
            "gen_1",
            vec![manifest_item("src/lib.rs", "manifest-created")],
        ))
        .await
        .unwrap();

    let generated = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();

    assert_eq!(generated.generation, SourceGenerationId::new("gen_2"));
}

#[tokio::test]
async fn fake_ledger_diffs_version_and_mtime_changes() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let previous = manifest_with_freshness("a", Some("v1"), ts());
    ledger.put_manifest(previous.clone()).await.unwrap();
    complete_and_publish(&ledger, completed_generation_for_manifest(&previous)).await;

    let version_changed = ledger
        .diff_manifest(manifest_with_freshness("a", Some("v2"), ts()))
        .await
        .unwrap();
    assert_eq!(version_changed.counts.modified, 1);
    assert_eq!(version_changed.counts.unchanged, 0);

    let mtime_changed = ledger
        .diff_manifest(manifest_with_freshness(
            "a",
            Some("v1"),
            Timestamp("2026-07-02T00:00:00Z".to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(mtime_changed.counts.modified, 1);
    assert_eq!(mtime_changed.counts.unchanged, 0);
}

#[tokio::test]
async fn fake_ledger_rejects_non_publishable_generation_statuses() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let running = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();

    let error = ledger
        .publish_generation(publish_request(&running))
        .await
        .unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_not_publishable"
    );
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        None
    );

    ledger
        .put_manifest(manifest_for_generation(&running, "running"))
        .await
        .unwrap();
    complete_and_publish(&ledger, completed_generation(running.clone())).await;
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(running.generation)
    );
}

#[tokio::test]
async fn fake_ledger_rejects_recompleting_published_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let running = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    ledger
        .put_manifest(manifest_for_generation(&running, "published"))
        .await
        .unwrap();
    let published = complete_and_publish(&ledger, completed_generation(running.clone())).await;
    assert_eq!(published.publish_state, PublishState::Committed);
    assert!(published.published_at.is_some());

    let error = ledger
        .complete_generation(completed_generation(running.clone()))
        .await
        .expect_err("published generation cannot be completed again");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_already_published"
    );
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(running.generation)
    );
}
