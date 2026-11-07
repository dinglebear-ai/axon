use super::*;
use axon_api::source::{LeaseRequest, MetadataMap};
use axon_ledger::sqlite::SqliteLedgerStore;

#[tokio::test]
async fn failed_summary_write_still_releases_source_lease() {
    let directory = tempfile::tempdir().unwrap();
    let database = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("ledger.db").display()
    );
    let ledger = Arc::new(SqliteLedgerStore::connect(&database).await.unwrap());
    let connection = sqlx::SqlitePool::connect(&database).await.unwrap();
    let runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(axon_embedding::fake::FakeEmbeddingProvider::new(
            "fake-embedding",
            3,
        )),
        Arc::new(axon_vectors::store::FakeVectorStore::new("fake-vector")),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        3,
    );
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/lease-release".to_string(),
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
    let execution = SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter = axon_adapters::FakeSourceAdapter::new(route.adapter.clone());
    let emitter = SourceEventEmitter::new(None, Some(plan.job_id));
    let lease_key = format!("source:{}", route.source.source_id.0);
    let mut input = SourcePipelineInput {
        graph_stage: None,
        adapter: &adapter,
        plan,
        collection: "lease-test",
        owner_id: "failed-worker",
        auth_snapshot: None,
        execution: &execution,
    };
    input.plan.job_id = runtime
        .jobs
        .create(job_create_request(&input))
        .await
        .unwrap()
        .job_id;
    let result = run_with_lease(&runtime, &mut input, &emitter, |_| async {
        // Fail only subsequent source-summary writes; the lease table remains usable.
        sqlx::query("CREATE TRIGGER reject_summary BEFORE INSERT ON sources BEGIN SELECT RAISE(FAIL, 'summary write unavailable'); END")
            .execute(&connection).await.unwrap();
        Err(anyhow::anyhow!("primary acquisition failure"))
    }).await;
    let error = format!("{:#}", result.unwrap_err());
    assert!(error.contains("primary acquisition failure"), "{error}");
    assert!(error.contains("summary"));
    let replacement = ledger
        .acquire_lease(LeaseRequest {
            lease_key,
            owner_id: "next-worker".to_string(),
            ttl_seconds: 30,
            job_id: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert!(
        replacement.is_some(),
        "failure reporting must not retain the source lease"
    );
    connection.close().await;
}

fn request(owner: &str) -> LeaseRequest {
    LeaseRequest {
        lease_key: "source:long-crawl".to_string(),
        owner_id: owner.to_string(),
        ttl_seconds: 1,
        job_id: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn active_source_cannot_be_reclaimed_after_original_lease_deadline() {
    let ledger = Arc::new(SqliteLedgerStore::in_memory().await.unwrap());
    let lease = ledger
        .acquire_lease(request("source-worker"))
        .await
        .unwrap()
        .unwrap();
    maintain(ledger.clone(), &lease, 1, None, async {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(
            ledger
                .acquire_lease(request("competing-worker"))
                .await
                .unwrap()
                .is_none(),
            "active source lost its lease after the original TTL"
        );
    })
    .await;
}

#[tokio::test]
async fn completed_operation_stops_renewing_its_lease() {
    let ledger = Arc::new(SqliteLedgerStore::in_memory().await.unwrap());
    let lease = ledger
        .acquire_lease(request("source-worker"))
        .await
        .unwrap()
        .unwrap();
    let result = maintain(ledger.clone(), &lease, 1, None, async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        42
    })
    .await;
    assert_eq!(result, 42);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        ledger
            .acquire_lease(request("next-worker"))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn canceled_operation_does_not_leave_a_renewal_task() {
    let ledger = Arc::new(SqliteLedgerStore::in_memory().await.unwrap());
    let lease = ledger
        .acquire_lease(request("source-worker"))
        .await
        .unwrap()
        .unwrap();
    let task_ledger = ledger.clone();
    let task = tokio::spawn(async move {
        maintain(task_ledger, &lease, 1, None, std::future::pending::<()>()).await;
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_millis(250), async {
        loop {
            if ledger
                .acquire_lease(request("next-worker"))
                .await
                .unwrap()
                .is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancellation must release the lease without waiting for its TTL");
}

#[tokio::test]
async fn definitive_lease_loss_cancels_work_and_allows_its_cleanup_to_finish() {
    let ledger = Arc::new(SqliteLedgerStore::in_memory().await.unwrap());
    let lease = ledger
        .acquire_lease(request("old-owner"))
        .await
        .unwrap()
        .unwrap();
    ledger
        .release_lease(lease.lease_id.clone(), lease.owner_id.clone())
        .await
        .unwrap();
    let replacement = ledger
        .acquire_lease(request("new-owner"))
        .await
        .unwrap()
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut cleaned_up = false;
    let operation = maintain(ledger.clone(), &lease, 1, Some(cancel.clone()), async {
        cancel.cancelled().await;
        // Cleanup must remain inside the operation, not be dropped by maintain.
        tokio::task::yield_now().await;
        cleaned_up = true;
    });
    assert!(
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .is_ok(),
        "definitive ownership loss must cancel the expensive operation"
    );
    assert!(cleaned_up);
    assert!(
        ledger
            .heartbeat_lease(replacement.lease_id, replacement.owner_id, 30)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn renewal_storage_error_cancels_guarded_graph_work() {
    let directory = tempfile::tempdir().unwrap();
    let database = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("lease.db").display()
    );
    let ledger = Arc::new(SqliteLedgerStore::connect(&database).await.unwrap());
    let lease = ledger
        .acquire_lease(request("graph-worker"))
        .await
        .unwrap()
        .unwrap();
    let connection = sqlx::SqlitePool::connect(&database).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_heartbeat BEFORE UPDATE ON leases BEGIN SELECT RAISE(FAIL, 'renewal unavailable'); END").execute(&connection).await.unwrap();
    let cancellation = tokio_util::sync::CancellationToken::new();
    tokio::time::timeout(
        Duration::from_secs(2),
        maintain(
            ledger,
            &lease,
            1,
            Some(cancellation.clone()),
            cancellation.cancelled(),
        ),
    )
    .await
    .expect("renewal error must cancel before lease expiry");
}
