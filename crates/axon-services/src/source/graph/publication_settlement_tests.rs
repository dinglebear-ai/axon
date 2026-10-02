use super::*;
use axon_adapters::{FakeSourceAdapter, SourceAdapter};
use axon_graph::GraphStage;
use std::time::Duration;

#[tokio::test]
async fn canceled_finalizer_waits_for_publication_settlement_and_preserves_committed_vectors() {
    let harness = Box::pin(crate::test_support::source_context_with_fake_web())
        .await
        .unwrap();
    let runtime = harness.ctx().target_local_source_runtime().unwrap();
    let pool = harness.ctx().sqlite_pool().unwrap();
    let (adapter, plan, execution) = publication_plan();
    let input = SourcePipelineInput {
        graph_stage: None,
        adapter: &adapter,
        plan,
        collection: "test",
        owner_id: "publication-settlement",
        auth_snapshot: None,
        execution: &execution,
    };
    let generation = completed_generation(runtime, &input).await;
    let stage = GraphStage::begin(
        pool.as_ref().clone(),
        generation.source_id.clone(),
        generation.generation.clone(),
        input.plan.job_id,
        0,
    )
    .await
    .unwrap();
    let settlement = runtime.publication_settlement_gate.lock().await;
    let writer = runtime.sqlite_write_gate.lock().await;
    let mut connection = pool.acquire().await.unwrap();
    connection.close_on_drop();
    stage.prepare_activation(&mut connection).await.unwrap();
    let primary = anyhow::Error::new(ApiError::new(
        "source.test_canceled",
        ErrorStage::Publishing,
        "caller canceled while publication settles",
    ));
    let mut finalizer = Box::pin(finalize_failed_generation(
        runtime,
        &input,
        generation.clone(),
        primary,
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut finalizer)
            .await
            .is_err(),
        "cancellation finalization must wait until the publisher releases writer ownership"
    );
    assert!(!harness.vectors().calls().await.contains(&"delete"));
    commit_publication(
        &mut connection,
        pool.as_ref(),
        &stage,
        &generation,
        input.plan.job_id,
    )
    .await;
    stage.detach(&mut connection).await.unwrap();
    drop(connection);
    drop(writer);
    drop(settlement);
    let error = tokio::time::timeout(Duration::from_secs(2), finalizer)
        .await
        .unwrap();
    assert_settled(&harness, &generation, error).await;
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(pool.as_ref(), &[stage.id().to_string()])
        .await
        .unwrap();
}

async fn completed_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
) -> SourceGeneration {
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Running,
            empty_source_counts(),
            None,
        ))
        .await
        .unwrap();
    let mut generation = runtime
        .ledger
        .create_generation(input.plan.route.source.source_id.clone())
        .await
        .unwrap();
    let mut manifest = input.adapter.discover(&input.plan).await.unwrap();
    manifest.generation = generation.generation.clone();
    runtime.ledger.put_manifest(manifest).await.unwrap();
    generation.status = LifecycleStatus::Completed;
    runtime
        .ledger
        .complete_generation(generation)
        .await
        .unwrap()
}

async fn assert_settled(
    harness: &crate::test_support::SourceWebJobIdentityHarness,
    generation: &SourceGeneration,
    error: anyhow::Error,
) {
    let runtime = harness.ctx().target_local_source_runtime().unwrap();
    let pool = harness.ctx().sqlite_pool().unwrap();
    assert_eq!(
        error.chain().count(),
        1,
        "published cancellation cannot add failed-generation marking errors"
    );
    assert_eq!(
        error.downcast_ref::<ApiError>().unwrap().code.to_string(),
        "source.test_canceled"
    );
    assert!(
        !harness.vectors().calls().await.contains(&"delete"),
        "late cancellation must preserve committed vectors"
    );
    assert_eq!(
        runtime
            .ledger
            .committed_generation(generation.source_id.clone())
            .await
            .unwrap(),
        Some(generation.generation.clone())
    );
    let stored: String = sqlx::query_scalar(
        "SELECT generation_json FROM source_generations WHERE source_id=? AND generation=?",
    )
    .bind(&generation.source_id.0)
    .bind(&generation.generation.0)
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    let stored: SourceGeneration = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored.status, LifecycleStatus::Completed);
    assert!(
        GraphStage::activation_summary(
            pool.as_ref(),
            &generation.source_id,
            &generation.generation
        )
        .await
        .unwrap()
        .is_some()
    );
}

fn publication_plan() -> (FakeSourceAdapter, SourcePlan, SourceExecutionContext) {
    let request = SourceRequest::local_path("/tmp/publication-settlement", true);
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        true,
        None,
        None,
    );
    let execution = SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter =
        FakeSourceAdapter::new(route.adapter).with_item("readme.md", ContentKind::Markdown, "body");
    (adapter, plan, execution)
}

async fn commit_publication(
    connection: &mut sqlx::SqliteConnection,
    pool: &sqlx::SqlitePool,
    stage: &GraphStage,
    generation: &SourceGeneration,
    job: JobId,
) {
    let mut tx = sqlx::Transaction::<sqlx::Sqlite>::begin(
        connection,
        Some(std::borrow::Cow::Borrowed("BEGIN IMMEDIATE")),
    )
    .await
    .unwrap();
    let ledger = axon_ledger::sqlite::SqliteLedgerStore::from_pool(pool.clone());
    ledger
        .publish_generation_in_tx(
            &mut tx,
            PublishGenerationRequest {
                job_id: job,
                attempt: 0,
                source_id: generation.source_id.clone(),
                generation: generation.generation.clone(),
                expected_previous_generation: generation.previous_generation.clone(),
                retained_statuses: Vec::new(),
            },
        )
        .await
        .unwrap();
    stage.activate_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn dropping_publication_caller_leaves_owned_publisher_to_commit_and_preserve_vectors() {
    let harness = Box::pin(crate::test_support::source_context_with_fake_web())
        .await
        .unwrap();
    let runtime = harness.ctx().target_local_source_runtime().unwrap();
    let pool = harness.ctx().sqlite_pool().unwrap();
    let (adapter, plan, execution) = publication_plan();
    let mut input = SourcePipelineInput {
        graph_stage: None,
        adapter: &adapter,
        plan,
        collection: "test",
        owner_id: "publication-settlement",
        auth_snapshot: None,
        execution: &execution,
    };
    let generation = completed_generation(runtime, &input).await;
    runtime
        .ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("publication:{}", generation.source_id.0),
            owner_id: input.owner_id.to_string(),
            ttl_seconds: 30,
            job_id: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap()
        .unwrap();
    let stage = std::sync::Arc::new(
        GraphStage::begin(
            pool.as_ref().clone(),
            generation.source_id.clone(),
            generation.generation.clone(),
            input.plan.job_id,
            0,
        )
        .await
        .unwrap(),
    );
    input.graph_stage = Some(stage.clone());
    let writer = runtime.sqlite_write_gate.lock().await;
    let mut caller = Box::pin(crate::source::graph::staging::publish_ledger_and_graph(
        runtime,
        &input,
        PublishGenerationRequest {
            job_id: input.plan.job_id,
            attempt: 0,
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation.clone(),
            retained_statuses: Vec::new(),
        },
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut caller)
            .await
            .is_err(),
        "publisher must remain blocked on the held live SQLite writer"
    );
    assert!(
        runtime.publication_settlement_gate.try_lock().is_none(),
        "the actual publisher must own settlement before its caller drops"
    );
    drop(caller);
    assert!(
        runtime.publication_settlement_gate.try_lock().is_none(),
        "caller drop cannot release the owned publication settlement"
    );
    drop(writer);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let receipt = GraphStage::activation_summary(
                pool.as_ref(),
                &generation.source_id,
                &generation.generation,
            )
            .await
            .unwrap();
            let committed = runtime
                .ledger
                .committed_generation(generation.source_id.clone())
                .await
                .unwrap();
            if receipt.is_some() && committed.as_ref() == Some(&generation.generation) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned publisher commits both receipt and ledger after caller drop");
    let error = finalize_failed_generation(
        runtime,
        &input,
        generation.clone(),
        anyhow::Error::new(ApiError::new(
            "source.test_canceled",
            ErrorStage::Publishing,
            "caller canceled while publication settles",
        )),
    )
    .await;
    assert_settled(&harness, &generation, error).await;
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(pool.as_ref(), &[stage.id().to_string()])
        .await
        .unwrap();
}
