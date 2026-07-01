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

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);
    let debt_json: String = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_one(&store.pool)
        .await
        .expect("read cleanup debt");
    let debt: CleanupDebt = serde_json::from_str(&debt_json).expect("parse cleanup debt");
    assert_eq!(debt.kind, CleanupDebtKind::VectorDelete);
    assert_eq!(debt.generation, Some(gen1.generation.clone()));
    assert_eq!(
        debt.selector,
        CleanupSelector::SourceItem {
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/old.rs"),
            generation: gen1.generation,
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
    assert_eq!(stored_generation.cleanup_debt, vec![debt.debt_id]);
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

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 2);
    let rows = sqlx::query_scalar::<_, String>("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let selectors = rows
        .into_iter()
        .map(|json| {
            let debt: CleanupDebt = serde_json::from_str(&json).expect("parse cleanup debt");
            debt.selector
        })
        .collect::<Vec<_>>();
    assert!(selectors.contains(&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen1.generation,
    }));
    assert!(selectors.contains(&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen3.generation,
    }));
}
