use super::*;

#[tokio::test]
async fn rejected_candidates_remain_degraded_in_bulk_and_replay_receipts() {
    for memory in [false, true] {
        let directory = std::env::temp_dir().join(format!("axon-rejections-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let live = if memory {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            crate::migration::ensure_schema(&pool).await.unwrap();
            SqliteGraphStore::from_pool(pool)
        } else {
            SqliteGraphStore::connect(directory.join("live.sqlite").to_str().unwrap())
                .await
                .unwrap()
        };
        let source = SourceId::new("s");
        let generation = SourceGenerationId::new("g");
        let stage = GraphStage::begin(
            live.pool().clone(),
            source.clone(),
            generation.clone(),
            JobId::new(Uuid::new_v4()),
            1,
        )
        .await
        .unwrap();
        stage.record_rejected_candidates(2).await.unwrap();
        stage
            .write_candidates(vec![repo_docs_candidate(
                "valid",
                "s",
                vec![ev("valid", "sitemap", 0.8)],
            )])
            .await
            .unwrap();
        stage.record_rejected_candidates(3).await.unwrap();
        let before = stage.summary().await.unwrap();
        assert_eq!(before.warnings.len(), 1, "bounded aggregate across batches");
        assert!(before.warnings[0].message.contains("rejected 5 invalid"));
        assert_eq!(
            before.header.status,
            axon_api::source::LifecycleStatus::CompletedDegraded
        );
        let mut conn = live.pool().acquire().await.unwrap();
        stage.prepare_activation(&mut conn).await.unwrap();
        assert!(stage.record_rejected_candidates(1).await.is_err());
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .unwrap();
        let activated = stage.activate_in_tx(&mut conn).await.unwrap();
        sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
        stage.detach(&mut conn).await.unwrap();
        drop(conn);
        assert_eq!(activated.warnings, before.warnings);
        assert!(activated.nodes_upserted > 0, "valid facts still publish");
        let receipt = GraphStage::activation_summary(live.pool(), &source, &generation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.warnings, before.warnings);
        assert_eq!(
            receipt.header.status,
            axon_api::source::LifecycleStatus::CompletedDegraded
        );
        stage.mark_disposable().await.unwrap();
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap();
        live.pool().close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }
}
