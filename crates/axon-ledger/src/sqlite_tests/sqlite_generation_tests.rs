use super::*;

#[tokio::test]
async fn sqlite_generation_sequence_is_unique_per_source() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    let duplicate = sqlx::query(
        r#"
        INSERT INTO source_generations (
            source_id,
            generation,
            sequence,
            status,
            publish_state,
            generation_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind("src_sqlite")
    .bind("gen_duplicate")
    .bind(1_i64)
    .bind("running")
    .bind("writing")
    .bind("{}")
    .bind(ts().0)
    .execute(&store.pool)
    .await;

    assert!(
        duplicate.is_err(),
        "duplicate sequence for {:?} should violate the unique index",
        gen1.generation
    );
}

#[tokio::test]
async fn sqlite_generation_publish_controls_committed_baseline() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let running = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");
    assert_eq!(running.generation, SourceGenerationId::new("gen_1"));
    assert_eq!(running.previous_generation, None);

    let error = store
        .publish_generation(publish_request(&running))
        .await
        .expect_err("running generation is not publishable");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_not_publishable"
    );

    let missing_manifest = completed_generation_from(&running);
    let error = store
        .complete_generation(missing_manifest)
        .await
        .expect_err("completed generation without manifest is not publishable");
    assert_eq!(error.code.to_string(), "source.ledger.manifest_missing");

    let committed_manifest = manifest_with_items(
        &running.generation.0,
        vec![manifest_item("src/lib.rs", "committed")],
    );
    store
        .put_manifest(committed_manifest)
        .await
        .expect("put committed manifest");
    let published = complete_and_publish(
        &store,
        completed_generation_for_manifest(&manifest_with_items(
            &running.generation.0,
            vec![manifest_item("src/lib.rs", "committed")],
        )),
    )
    .await;
    assert_eq!(published.publish_state, PublishState::Committed);
    assert!(published.published_at.is_some());

    let generation_row: (String, String) = sqlx::query_as(
        "SELECT publish_state, generation_json FROM source_generations WHERE generation = ?1",
    )
    .bind(&running.generation.0)
    .fetch_one(&store.pool)
    .await
    .expect("read stored published generation");
    assert_eq!(generation_row.0, "committed");
    let stored_generation: SourceGeneration =
        serde_json::from_str(&generation_row.1).expect("parse generation json");
    assert_eq!(stored_generation.publish_state, PublishState::Committed);
    assert!(stored_generation.published_at.is_some());

    let error = store
        .complete_generation(completed_generation_from(&running))
        .await
        .expect_err("published generation cannot be completed again");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_already_published"
    );
    let generation_row: (String, Option<String>) = sqlx::query_as(
        "SELECT publish_state, published_at FROM source_generations WHERE generation = ?1",
    )
    .bind(&running.generation.0)
    .fetch_one(&store.pool)
    .await
    .expect("read stored generation after rejected duplicate completion");
    assert_eq!(generation_row.0, "committed");
    assert!(generation_row.1.is_some());

    let next = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create next generation");
    assert_eq!(next.generation, SourceGenerationId::new("gen_2"));
    assert_eq!(
        next.previous_generation,
        Some(SourceGenerationId::new("gen_1"))
    );

    store
        .put_manifest(manifest_with_items(
            "gen_2",
            vec![manifest_item("src/lib.rs", "interrupted")],
        ))
        .await
        .expect("put interrupted manifest");
    let diff = store
        .diff_manifest(manifest_with_items(
            "gen_3",
            vec![manifest_item("src/lib.rs", "committed")],
        ))
        .await
        .expect("diff against committed generation");
    assert_eq!(diff.counts.unchanged, 1);
    assert_eq!(diff.counts.added, 0);
}

#[tokio::test]
async fn sqlite_fail_generation_persists_failed_status_and_rejects_published_generations() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let running = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");
    store
        .put_manifest(manifest_with_items(
            &running.generation.0,
            vec![manifest_item("src/lib.rs", "failed")],
        ))
        .await
        .expect("put manifest");
    let completed = store
        .complete_generation(completed_generation_from(&running))
        .await
        .expect("complete generation");

    let failed = store
        .fail_generation(completed)
        .await
        .expect("fail generation");

    assert_eq!(failed.status, LifecycleStatus::Failed);
    assert_eq!(failed.publish_state, PublishState::Writing);
    assert!(failed.published_at.is_none());
    let row: (String, String, Option<String>, String) = sqlx::query_as(
        "SELECT status, publish_state, published_at, generation_json FROM source_generations WHERE generation = ?1",
    )
    .bind(&running.generation.0)
    .fetch_one(&store.pool)
    .await
    .expect("read failed generation");
    assert_eq!(row.0, "failed");
    assert_eq!(row.1, "writing");
    assert!(row.2.is_none());
    let stored_generation: SourceGeneration =
        serde_json::from_str(&row.3).expect("parse generation json");
    assert_eq!(stored_generation.status, LifecycleStatus::Failed);

    let published_running = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create published generation");
    store
        .put_manifest(manifest_with_items(
            &published_running.generation.0,
            vec![manifest_item("src/lib.rs", "published")],
        ))
        .await
        .expect("put published manifest");
    let published =
        complete_and_publish(&store, completed_generation_from(&published_running)).await;
    let error = store
        .fail_generation(published)
        .await
        .expect_err("published generation cannot be failed");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_already_published"
    );
}

#[tokio::test]
async fn sqlite_publish_rejects_stale_generation_baseline() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/lib.rs", "gen1")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let stale = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create stale gen2");
    let fresh = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create fresh gen3");
    store
        .put_manifest(manifest_with_items(
            &fresh.generation.0,
            vec![manifest_item("src/lib.rs", "gen3")],
        ))
        .await
        .expect("put fresh");
    complete_and_publish(&store, completed_generation_from(&fresh)).await;

    store
        .put_manifest(manifest_with_items(
            &stale.generation.0,
            vec![manifest_item("src/lib.rs", "gen2")],
        ))
        .await
        .expect("put stale");
    let completed_stale = store
        .complete_generation(completed_generation_from(&stale))
        .await
        .expect("complete stale generation");
    let error = store
        .publish_generation(publish_request(&completed_stale))
        .await
        .expect_err("stale generation cannot rewind committed baseline");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_baseline_changed"
    );

    let diff = store
        .diff_manifest(manifest_with_items(
            "gen_next",
            vec![manifest_item("src/lib.rs", "gen3")],
        ))
        .await
        .expect("diff");
    assert_eq!(diff.previous_generation, Some(fresh.generation));
    assert_eq!(diff.counts.unchanged, 1);
}

#[tokio::test]
async fn sqlite_publish_creates_cleanup_debt_for_removed_items() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![
                manifest_item("README.md", "same"),
                manifest_item("src/old.rs", "removed"),
            ],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("README.md", "same")],
        ))
        .await
        .expect("put gen2");
    let published = complete_and_publish(&store, completed_generation_from(&gen2)).await;

    // "src/old.rs" is genuinely removed (absent from gen2's manifest, not
    // merely modified) so it produces both a `VectorDelete` debt (points
    // always need deletion across a generation change) and a `GraphPrune`
    // debt (the item's own document-node stable key is now orphaned).
    assert_eq!(store.cleanup_debt_count().await.expect("count"), 2);
    let debt_rows: Vec<String> = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let debts: Vec<CleanupDebt> = debt_rows
        .iter()
        .map(|json| serde_json::from_str(json).expect("parse cleanup debt"))
        .collect();
    let vector_debt = debts
        .iter()
        .find(|debt| debt.kind == CleanupDebtKind::VectorDelete)
        .expect("vector delete debt");
    assert_eq!(vector_debt.generation, Some(gen1.generation.clone()));
    assert_eq!(
        vector_debt.selector,
        CleanupSelector::SourceItem {
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/old.rs"),
            generation: gen1.generation.clone(),
        }
    );
    let graph_debt = debts
        .iter()
        .find(|debt| debt.kind == CleanupDebtKind::GraphPrune)
        .expect("graph prune debt");
    assert_eq!(graph_debt.generation, Some(gen1.generation.clone()));
    assert_eq!(
        graph_debt.selector,
        CleanupSelector::GraphNodes {
            stable_keys: vec!["src/old.rs".to_string()],
        }
    );

    let generation_json: String =
        sqlx::query_scalar("SELECT generation_json FROM source_generations WHERE generation = ?1")
            .bind(&gen2.generation.0)
            .fetch_one(&store.pool)
            .await
            .expect("read generation json");
    let stored_generation: SourceGeneration =
        serde_json::from_str(&generation_json).expect("parse generation json");
    let mut expected_ids = vec![vector_debt.debt_id.clone(), graph_debt.debt_id.clone()];
    let mut actual_ids = stored_generation.cleanup_debt.clone();
    expected_ids.sort_by(|a, b| a.0.cmp(&b.0));
    actual_ids.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(actual_ids, expected_ids);
    assert_eq!(
        stored_generation.publish_state,
        PublishState::CleanupPending
    );
    assert_eq!(published.publish_state, PublishState::CleanupPending);
}

#[tokio::test]
async fn sqlite_publish_creates_cleanup_debt_for_modified_items() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/lib.rs", "old")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("src/lib.rs", "new")],
        ))
        .await
        .expect("put gen2");
    complete_and_publish(&store, completed_generation_from(&gen2)).await;

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);
    let debt_json: String = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_one(&store.pool)
        .await
        .expect("read cleanup debt");
    let debt: CleanupDebt = serde_json::from_str(&debt_json).expect("parse cleanup debt");
    assert_eq!(
        debt.selector,
        CleanupSelector::SourceItem {
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            generation: gen1.generation,
        }
    );
}

#[tokio::test]
async fn sqlite_publish_creates_artifact_and_cache_cleanup_debt_from_manifest_metadata() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    let mut old_item = manifest_item("src/lib.rs", "old");
    let clean_artifact = artifact_ref("art_clean", ArtifactKind::NormalizedContent);
    old_item.metadata.insert(
        "_axon_artifacts".to_string(),
        serde_json::json!([clean_artifact.clone()]),
    );
    let cache_key = DocumentCacheKey {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: Some(gen1.generation.clone()),
    };
    old_item.metadata.insert(
        "_axon_document_cache_key".to_string(),
        serde_json::to_value(&cache_key).unwrap(),
    );
    let warc_artifact = artifact_ref("art_warc", ArtifactKind::Warc);
    let mut gen1_manifest = manifest_with_items(&gen1.generation.0, vec![old_item]);
    gen1_manifest.metadata.insert(
        "_axon_artifacts".to_string(),
        serde_json::json!([warc_artifact.clone()]),
    );
    store.put_manifest(gen1_manifest).await.expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("src/lib.rs", "new")],
        ))
        .await
        .expect("put gen2");
    complete_and_publish(&store, completed_generation_from(&gen2)).await;

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 4);
    let debt_rows: Vec<String> = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let debts: Vec<CleanupDebt> = debt_rows
        .iter()
        .map(|json| serde_json::from_str(json).expect("parse cleanup debt"))
        .collect();
    assert!(
        debts
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::ArtifactDelete
                && debt.generation == Some(gen1.generation.clone())
                && matches!(
                    &debt.selector,
                    CleanupSelector::Artifact { artifact_id }
                        if artifact_id == &warc_artifact.artifact_id
                )),
        "manifest-level WARC artifact should get cleanup debt"
    );
    assert!(
        debts
            .iter()
            .any(|debt| debt.kind == CleanupDebtKind::ArtifactDelete
                && debt.generation == Some(gen1.generation.clone())
                && matches!(
                    &debt.selector,
                    CleanupSelector::Artifact { artifact_id }
                        if artifact_id == &clean_artifact.artifact_id
                )),
        "item-level clean artifact should get cleanup debt"
    );
    assert!(
        debts.iter().any(|debt| {
            debt.kind == CleanupDebtKind::CachePrune
                && debt.generation == Some(gen1.generation.clone())
                && matches!(
                    &debt.selector,
                    CleanupSelector::CacheKeys { keys }
                        if keys.contains(&serde_json::to_string(&cache_key).unwrap())
                )
        }),
        "item cache key should get cache-prune cleanup debt"
    );
}

#[tokio::test]
async fn sqlite_publish_keeps_distinct_cleanup_debt_for_readded_item_generations() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/old.rs", "first")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(&gen2.generation.0, vec![]))
        .await
        .expect("put gen2");
    complete_and_publish(&store, completed_generation_from(&gen2)).await;

    let gen3 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen3");
    store
        .put_manifest(manifest_with_items(
            &gen3.generation.0,
            vec![manifest_item("src/old.rs", "second")],
        ))
        .await
        .expect("put gen3");
    complete_and_publish(&store, completed_generation_from(&gen3)).await;

    let gen4 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen4");
    store
        .put_manifest(manifest_with_items(&gen4.generation.0, vec![]))
        .await
        .expect("put gen4");
    complete_and_publish(&store, completed_generation_from(&gen4)).await;

    // Each removal of "src/old.rs" (gen1->gen2, gen3->gen4) creates a distinct
    // `VectorDelete` + `GraphPrune` pair (4 rows), plus one `LedgerPrune` row
    // once the chain leaves gen2 (an always-empty generation, never
    // re-touched by any later debt) past the retention window — see
    // `sqlite_publish_creates_ledger_prune_debt_past_retention` for an
    // isolated LedgerPrune-only trace of this same mechanism.
    assert_eq!(store.cleanup_debt_count().await.expect("count"), 5);
    let rows = sqlx::query_scalar::<_, String>("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let debts = rows
        .into_iter()
        .map(|json| serde_json::from_str::<CleanupDebt>(&json).expect("parse cleanup debt"))
        .collect::<Vec<_>>();
    let selectors = debts.iter().map(|debt| &debt.selector).collect::<Vec<_>>();
    assert!(selectors.contains(&&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen1.generation.clone(),
    }));
    assert!(selectors.contains(&&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen3.generation.clone(),
    }));
    assert!(selectors.contains(&&CleanupSelector::GraphNodes {
        stable_keys: vec!["src/old.rs".to_string()],
    }));
    let ledger_prune_targets = debts
        .iter()
        .filter(|debt| debt.kind == CleanupDebtKind::LedgerPrune)
        .map(|debt| match &debt.selector {
            CleanupSelector::LedgerGenerations {
                up_to_generation, ..
            } => up_to_generation.clone(),
            other => panic!("expected LedgerGenerations selector, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(ledger_prune_targets, vec![gen2.generation]);
}

/// A source that publishes generation after generation with an always-
/// unchanged item never produces `VectorDelete`/`GraphPrune` debt (nothing
/// removed or modified), which isolates `LedgerPrune` production: once a
/// supersede chain leaves more than `LEDGER_GENERATION_RETENTION_COMMITTED`
/// (2 — the just-published generation plus its immediate predecessor) old
/// generations behind, the oldest ones become `LedgerPrune` candidates, one
/// debt row per stale generation.
#[tokio::test]
async fn sqlite_publish_creates_ledger_prune_debt_past_retention() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let mut generations = Vec::new();
    for _ in 0..4 {
        let generation = store
            .create_generation(SourceId::new("src_sqlite"))
            .await
            .expect("create generation");
        store
            .put_manifest(manifest_with_items(
                &generation.generation.0,
                vec![manifest_item("README.md", "stable")],
            ))
            .await
            .expect("put manifest");
        complete_and_publish(&store, completed_generation_from(&generation)).await;
        generations.push(generation);
    }

    // No item was ever removed or modified, so the only debt possible is
    // `LedgerPrune`. Retention keeps generations 3 and 4 (the newest
    // committed plus its predecessor); generations 1 and 2 age out.
    assert_eq!(store.cleanup_debt_count().await.expect("count"), 2);
    let rows = sqlx::query_scalar::<_, String>("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let debts = rows
        .into_iter()
        .map(|json| serde_json::from_str::<CleanupDebt>(&json).expect("parse cleanup debt"))
        .collect::<Vec<_>>();
    for debt in &debts {
        assert_eq!(debt.kind, CleanupDebtKind::LedgerPrune);
    }
    let up_to_generations: Vec<SourceGenerationId> = debts
        .iter()
        .map(|debt| match &debt.selector {
            CleanupSelector::LedgerGenerations {
                up_to_generation, ..
            } => up_to_generation.clone(),
            other => panic!("expected LedgerGenerations selector, got {other:?}"),
        })
        .collect();
    assert!(up_to_generations.contains(&generations[0].generation));
    assert!(up_to_generations.contains(&generations[1].generation));
    assert!(!up_to_generations.contains(&generations[2].generation));
    assert!(!up_to_generations.contains(&generations[3].generation));
}
