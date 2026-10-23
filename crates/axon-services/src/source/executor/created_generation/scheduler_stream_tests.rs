use super::*;
use crate::source::executor::generation_work::{
    PreparedBatchSender, PreparedBatchSideEffects, prepared_work_channel,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorStore, VectorStore};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, Semaphore, mpsc};

#[path = "scheduler_test_providers.rs"]
mod providers;
use providers::{ObservedEmbedding, ObservedVectors};

struct Harness {
    runtime: TargetLocalSourceRuntime,
    embedding: Arc<ObservedEmbedding>,
    vectors: Arc<ObservedVectors>,
    ledger: Arc<FakeLedgerStore>,
    embeds: mpsc::UnboundedReceiver<usize>,
    writes: mpsc::UnboundedReceiver<usize>,
}

impl Harness {
    fn new(
        overlap: bool,
        embed_gate: Option<Arc<Semaphore>>,
        write_gate: Option<Arc<Semaphore>>,
        fail_at: Option<usize>,
    ) -> Self {
        let (embed_tx, embeds) = mpsc::unbounded_channel();
        let (write_tx, writes) = mpsc::unbounded_channel();
        let active = Arc::new(AtomicBool::new(false));
        let embedding = Arc::new(ObservedEmbedding {
            inner: FakeEmbeddingProvider::new("fake-embedding", 3),
            started: embed_tx,
            first_gate: embed_gate,
            fail_at,
            upsert_active: active.clone(),
            overlapped: AtomicBool::new(false),
        });
        let vectors = Arc::new(ObservedVectors {
            inner: FakeVectorStore::new("fake-vector"),
            started: write_tx,
            first_gate: write_gate,
            active,
            order: Mutex::new(Vec::new()),
        });
        let ledger = Arc::new(FakeLedgerStore::new());
        let mut runtime = TargetLocalSourceRuntime::new(
            Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
            ledger.clone(),
            embedding.clone(),
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            3,
        );
        runtime.embed_pool_max_inputs = 1;
        runtime.vector_upsert_embed_overlap = overlap;
        runtime.embed_scheduler_flush_delay = std::time::Duration::from_millis(1);
        Self {
            runtime,
            embedding,
            vectors,
            ledger,
            embeds,
            writes,
        }
    }
}

async fn send_document(
    sender: &mut PreparedBatchSender,
    index: usize,
    final_item: bool,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let mut document = axon_vectors::testing::test_prepared_document();
    document.document_id = DocumentId::new(format!("doc-{index}"));
    document.source_item_key = SourceItemKey::new(format!("item-{index}"));
    document.metadata.remove("embedding_batch_id");
    document.metadata.insert(
        "embedding_model".into(),
        serde_json::json!("fake-embedding"),
    );
    document.chunks.truncate(1);
    document.chunks[0].document_id = document.document_id.clone();
    document.chunks[0].chunk_id = ChunkId::new(format!("chunk-{index}"));
    document.chunks[0].chunk_key = format!("chunk-{index}");
    sender
        .send_final(
            vec![document],
            PreparedBatchSideEffects::empty(),
            final_item,
            cancel,
        )
        .await
}

async fn run(
    runtime: TargetLocalSourceRuntime,
    receiver: crate::source::executor::generation_work::PreparedBatchReceiver,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/scheduler",
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
    let mut source = crate::source::executor::metadata::source_summary(
        &input,
        LifecycleStatus::Running,
        empty_source_counts(),
        None,
    );
    source.source_id = SourceId::new("src-web");
    runtime.ledger.upsert_source(source).await.unwrap();
    let collection = collection_spec(input.collection, 3);
    runtime
        .vector_store
        .ensure_collection(collection.clone())
        .await
        .unwrap();
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    run_generation_scheduler(
        &runtime,
        &input,
        &emitter,
        &ProgressCoordinator::test_noop(),
        collection,
        receiver,
        &mut GenerationAccumulator::default(),
        &mut PipelineProgress::default(),
        &cancel,
    )
    .await
}

async fn signal(receiver: &mut mpsc::UnboundedReceiver<usize>) -> usize {
    tokio::time::timeout(std::time::Duration::from_secs(5), receiver.recv())
        .await
        .expect("scheduler signal deadline")
        .expect("provider signal")
}

#[tokio::test]
async fn scheduled_overlap_setting_controls_provider_calls() {
    for overlap in [false, true] {
        let gate = Arc::new(Semaphore::new(0));
        let mut h = Harness::new(overlap, None, Some(gate.clone()), None);
        let (mut sender, receiver) = prepared_work_channel(1).unwrap();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
        send_document(&mut sender, 0, false, &cancel).await.unwrap();
        assert_eq!(signal(&mut h.embeds).await, 0);
        assert_eq!(signal(&mut h.writes).await, 0);
        // Submit only after publication starts: this catches both admission
        // and speculative work that bypass the overlap switch.
        send_document(&mut sender, 1, true, &cancel).await.unwrap();
        drop(sender);
        if overlap {
            assert_eq!(signal(&mut h.embeds).await, 1);
            assert!(h.embedding.overlapped.load(Ordering::Acquire));
        } else {
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), h.embeds.recv())
                    .await
                    .is_err(),
                "disabled overlap must keep the second provider call out of active publication"
            );
        }
        gate.add_permits(1);
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(h.embedding.overlapped.load(Ordering::Acquire), overlap);
        assert_eq!(*h.vectors.order.lock().await, vec![0, 1]);
    }
}

#[tokio::test]
async fn rolling_admission_replenishes_before_slow_first_pool_finishes() {
    let gate = Arc::new(Semaphore::new(0));
    let mut h = Harness::new(true, Some(gate.clone()), None, None);
    let (mut sender, receiver) = prepared_work_channel(1).unwrap();
    let cancel = CancellationToken::new();
    let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
    for index in 0..3 {
        send_document(&mut sender, index, index == 2, &cancel)
            .await
            .unwrap();
    }
    drop(sender);
    let mut started = vec![
        signal(&mut h.embeds).await,
        signal(&mut h.embeds).await,
        signal(&mut h.embeds).await,
    ];
    started.sort();
    assert_eq!(
        started,
        vec![0, 1, 2],
        "third pool must enter before releasing the first"
    );
    assert!(
        h.vectors.order.lock().await.is_empty(),
        "fast later results cannot publish ahead of the first"
    );
    gate.add_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(*h.vectors.order.lock().await, vec![0, 1, 2]);
}

#[tokio::test]
async fn scheduled_embedding_failure_keeps_prior_publication_checkpoint() {
    for overlap in [false, true] {
        let h = Harness::new(overlap, None, None, Some(1));
        let (mut sender, receiver) = prepared_work_channel(1).unwrap();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
        for index in 0..2 {
            send_document(&mut sender, index, index == 1, &cancel)
                .await
                .unwrap();
        }
        drop(sender);
        let error = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(format!("{error:#}").contains("scheduler test embedding failure"));
        let status = h
            .ledger
            .document_status(&DocumentId::new("doc-0"))
            .await
            .unwrap();
        assert_eq!(status.status, DocumentLifecycleStatus::Vectorized);
        assert_eq!(status.vector_point_count, 1);
        assert_eq!(*h.vectors.order.lock().await, vec![0]);
    }
}

#[tokio::test]
async fn scheduled_cancellation_preserves_completed_publication() {
    let mut h = Harness::new(false, None, None, None);
    let (mut sender, receiver) = prepared_work_channel(1).unwrap();
    let cancel = CancellationToken::new();
    let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
    send_document(&mut sender, 0, false, &cancel).await.unwrap();
    assert_eq!(signal(&mut h.writes).await, 0);
    // The provider's completion is not itself the durable checkpoint.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if h.ledger
                .document_status(&DocumentId::new("doc-0"))
                .await
                .is_some_and(|status| status.status == DocumentLifecycleStatus::Vectorized)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    drop(sender);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        h.ledger
            .document_status(&DocumentId::new("doc-0"))
            .await
            .unwrap()
            .status,
        DocumentLifecycleStatus::Vectorized
    );
}

#[tokio::test]
async fn oldest_item_deadline_flushes_without_another_envelope() {
    let mut h = Harness::new(true, None, None, None);
    h.runtime.embed_pool_max_inputs = 16;
    let (mut sender, receiver) = prepared_work_channel(16).unwrap();
    let cancel = CancellationToken::new();
    let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
    send_document(&mut sender, 0, false, &cancel).await.unwrap();
    assert_eq!(signal(&mut h.embeds).await, 0);
    assert_eq!(signal(&mut h.writes).await, 0);
    drop(sender);
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn partial_pool_wait_remains_cancellable_at_scheduler_entrypoint() {
    let mut h = Harness::new(true, None, None, None);
    h.runtime.embed_pool_max_inputs = 16;
    h.runtime.embed_scheduler_flush_delay = std::time::Duration::from_secs(30);
    let (mut sender, receiver) = prepared_work_channel(16).unwrap();
    let cancel = CancellationToken::new();
    send_document(&mut sender, 0, false, &cancel).await.unwrap();
    let task = tokio::spawn(run(h.runtime, receiver, cancel.clone()));
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert!(h.embedding.inner.calls().await.is_empty());
    drop(sender);
}
