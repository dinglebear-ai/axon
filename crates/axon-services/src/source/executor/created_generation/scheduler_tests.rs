use super::*;

#[test]
fn small_prepared_envelopes_coalesce_until_the_chunk_budget_or_final_item() {
    let mut pending_chunks = 0;
    let mut pending_envelopes = 0;
    let mut published_batch_sizes = Vec::new();
    for item in 0..8 {
        pending_chunks += 1;
        pending_envelopes += 1;
        if should_flush(pending_chunks, pending_envelopes, 16, item == 7, false) {
            published_batch_sizes.push(pending_chunks);
            pending_chunks = 0;
            pending_envelopes = 0;
        }
    }
    assert_eq!(
        published_batch_sizes,
        vec![8],
        "small documents must share a provider batch instead of flushing every two envelopes"
    );
}

#[tokio::test]
async fn scheduler_publishes_eight_small_documents_in_one_provider_batch() {
    use crate::source::executor::generation_work::{
        PreparedBatchSideEffects, prepared_work_channel,
    };
    use axon_embedding::fake::FakeEmbeddingProvider;
    use axon_ledger::store::FakeLedgerStore;
    use axon_vectors::store::{FakeVectorStore, VectorStore};
    use std::sync::Arc;

    let embedding = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 3));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        embedding.clone(),
        vectors.clone(),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        3,
    );
    runtime.embed_pool_max_inputs = 16;
    runtime.embed_scheduler_flush_delay = std::time::Duration::from_secs(30);
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/scheduler".to_string(),
    ))
    .unwrap()
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        true,
        None,
        None,
    );
    let execution =
        crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter = axon_adapters::FakeSourceAdapter::new(route.adapter.clone());
    let input = SourcePipelineInput {
        graph_stage: None,
        adapter: &adapter,
        plan,
        collection: "scheduler-test",
        owner_id: "scheduler-test",
        auth_snapshot: None,
        execution: &execution,
    };
    let collection = collection_spec(input.collection, 3);
    runtime
        .ledger
        .upsert_source(crate::source::executor::metadata::source_summary(
            &input,
            LifecycleStatus::Running,
            empty_source_counts(),
            None,
        ))
        .await
        .unwrap();
    vectors.ensure_collection(collection.clone()).await.unwrap();
    let (mut sender, receiver) = prepared_work_channel(16).unwrap();
    let cancel = CancellationToken::new();
    let producer = async {
        for index in 0..8 {
            let mut document = axon_vectors::testing::test_prepared_document();
            document.document_id = DocumentId::new(format!("doc-{index}"));
            document.source_id = input.plan.route.source.source_id.clone();
            document.source_item_key = SourceItemKey::new(format!("item-{index}"));
            document.metadata.remove("embedding_batch_id");
            document.metadata.insert(
                "embedding_model".to_string(),
                serde_json::json!("fake-embedding"),
            );
            document.chunks.truncate(1);
            for chunk in &mut document.chunks {
                chunk.document_id = document.document_id.clone();
                chunk.chunk_id = ChunkId::new(format!("chunk-{index}"));
                chunk.chunk_key = format!("chunk-{index}");
            }
            sender
                .send_final(
                    vec![document],
                    PreparedBatchSideEffects::empty(),
                    index == 7,
                    &cancel,
                )
                .await?;
        }
        drop(sender);
        Ok::<_, anyhow::Error>(())
    };
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    let coordinator = ProgressCoordinator::test_noop();
    let mut accumulator = GenerationAccumulator::default();
    let mut progress = PipelineProgress::default();
    let consumer = run_generation_scheduler(
        &runtime,
        &input,
        &emitter,
        &coordinator,
        collection,
        receiver,
        &mut accumulator,
        &mut progress,
        &cancel,
    );
    let (sent, result) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(producer, consumer)
    })
    .await
    .expect("scheduler must drain without waiting for its flush timer");
    result.unwrap();
    sent.unwrap();
    assert_eq!(
        embedding
            .calls()
            .await
            .iter()
            .map(|batch| batch.items.len())
            .collect::<Vec<_>>(),
        vec![8]
    );
    assert_eq!(vectors.points(input.collection).await.len(), 8);
}

#[test]
fn scheduler_flush_policy_covers_full_group_final_and_close_without_a_timer() {
    let pool = 512;
    assert!(!should_flush(pool, 1, pool, false, false));
    // Prepared work retains permits from a three-pool semaphore until flush.
    // Flush at two pools so one maximum-sized envelope always
    // has enough headroom to reach the receiver and cannot deadlock its sender.
    assert!(should_flush(pool * 2, 2, pool, false, false));
    assert!(!should_flush(2, 2, pool, false, false));
    assert!(should_flush(0, 1024, pool, false, false));
    assert!(should_flush(1, 1, pool, true, false));
    assert!(should_flush(1, 1, pool, false, true));
}

#[tokio::test]
async fn oldest_item_deadline_flushes_when_no_more_work_can_arrive() {
    use crate::source::executor::generation_work::prepared_work_channel;
    let (_sender, mut receiver) = prepared_work_channel(16).unwrap();
    let cancel = CancellationToken::new();
    let wake = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        next_scheduler_wake(&mut receiver, &cancel, Some(tokio::time::Instant::now())),
    )
    .await
    .expect("byte-blocked or idle producers must not prevent a partial pool flush")
    .unwrap();
    assert!(matches!(wake, SchedulerWake::Flush));
}

#[tokio::test]
async fn partial_pool_wait_remains_cancellable() {
    use crate::source::executor::generation_work::prepared_work_channel;
    let (_sender, mut receiver) = prepared_work_channel(16).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = next_scheduler_wake(
        &mut receiver,
        &cancel,
        Some(tokio::time::Instant::now() + std::time::Duration::from_secs(30)),
    )
    .await;
    assert!(result.is_err());
}
