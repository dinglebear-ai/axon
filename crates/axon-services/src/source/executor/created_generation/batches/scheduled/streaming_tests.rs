use super::*;
use axon_adapters::{FakeSourceAdapter, SourceAdapter};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use std::sync::Arc;

#[tokio::test]
async fn slow_consumer_bounds_completed_file_acquisitions() {
    let source = "https://example.com/bounded";
    let route =
        crate::source::routing::resolve_source_route(&SourceRequest::new(source.to_owned()))
            .unwrap()
            .route;
    let mut plan = crate::source::dispatch::family_source_plan(source, &route, false, None, None);
    let mut adapter = FakeSourceAdapter::new(plan.route.adapter.clone());
    for i in 0..8 {
        adapter = adapter.with_item(format!("item-{i}"), ContentKind::PlainText, "body");
    }
    let adapter = Arc::new(adapter);
    let manifest = adapter.discover(&plan).await.unwrap();
    let mut diff = FakeLedgerStore::default()
        .diff_manifest(manifest)
        .await
        .unwrap();
    for item in &mut diff.added {
        item.size_bytes = Some(axon_adapters::acquisition::DEFAULT_ACQUISITION_BATCH_BYTES);
    }
    plan.route.source.source_kind = SourceKind::Git;
    let (tx, mut rx) = mpsc::channel(2);
    let observed = adapter.clone();
    let task = tokio::spawn(async move {
        let execution =
            crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
        let input = SourcePipelineInput {
            graph_stage: None,
            adapter: adapter.as_ref(),
            plan,
            collection: "bounded",
            owner_id: "test",
            auth_snapshot: None,
            execution: &execution,
        };
        acquire_files(&input, &diff, tx, &CancellationToken::new(), 1024).await
    });
    wait_for_acquisitions(&observed, 3).await;
    assert_eq!(
        rx.len(),
        2,
        "two ready batches plus one producer-held batch"
    );
    assert_eq!(
        observed
            .calls()
            .iter()
            .filter(|&&call| call == "acquire")
            .count(),
        3
    );
    drop(rx.recv().await.unwrap());
    wait_for_acquisitions(&observed, 4).await;
    assert_eq!(
        observed
            .calls()
            .iter()
            .filter(|&&call| call == "acquire")
            .count(),
        4
    );
    drop(rx);
    assert!(
        task.await.unwrap().is_err(),
        "producer settles when consumer closes"
    );
}

async fn wait_for_acquisitions(adapter: &FakeSourceAdapter, expected: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while adapter
            .calls()
            .iter()
            .filter(|&&call| call == "acquire")
            .count()
            < expected
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("producer advances to its bounded send");
}

#[test]
fn acquisition_error_wins_over_derived_stream_settlement() {
    for settlement in [
        anyhow::Error::new(ApiError::new(
            "source.acquire.incomplete",
            ErrorStage::Fetching,
            "incomplete",
        )),
        anyhow::anyhow!("prepared work send canceled"),
    ] {
        let mut acquired = ApiError::new(
            "adapter.test.read_failed",
            ErrorStage::Fetching,
            "read failed",
        )
        .with_source_item_key("src/file.rs");
        acquired.retryable = true;
        let error = settle_results(Err(settlement), Err(acquired.clone())).unwrap_err();
        assert_eq!(error.downcast_ref::<ApiError>(), Some(&acquired));
    }
}

#[test]
fn genuine_preparation_error_wins_over_acquisition_settlement_error() {
    let prepared = ApiError::new(
        "document.prepare_failed",
        ErrorStage::Preparing,
        "invalid document",
    )
    .with_source_item_key("first/file.rs");
    let acquired = ApiError::new(
        "source.acquire.canceled",
        ErrorStage::Fetching,
        "wave canceled",
    );
    let error =
        settle_results(Err(anyhow::Error::new(prepared.clone())), Err(acquired)).unwrap_err();
    assert_eq!(error.downcast_ref::<ApiError>(), Some(&prepared));
    assert!(format!("{error:#}").contains("wave canceled"));
}
