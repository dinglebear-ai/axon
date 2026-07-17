use axon_api::source::*;

use super::*;

fn record(status: MemoryStatus) -> MemoryRecord {
    MemoryRecord {
        memory_id: MemoryId::new("mem_sync"),
        memory_type: MemoryType::Fact,
        status,
        body: "canonical memory publication".to_string(),
        confidence: 0.9,
        salience: 0.8,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status,
            message: "created".to_string(),
            timestamp: Timestamp("2026-07-16T00:00:00Z".to_string()),
        }],
        visibility: Visibility::Internal,
        title: None,
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

#[test]
fn mutation_handoff_is_a_forced_canonical_memory_source_request() {
    let request = source_request(&record(MemoryStatus::Active), "update");

    assert_eq!(request.source, "memory://mem_sync");
    assert_eq!(request.adapter.as_deref(), Some("memory"));
    assert_eq!(request.scope, Some(SourceScope::Api));
    assert_eq!(request.intent, SourceIntent::Refresh);
    assert_eq!(request.refresh, SourceRefreshPolicy::Force);
    assert!(request.embed);
    assert_eq!(request.metadata["memory_mutation"], "update");
    assert_eq!(request.metadata["memory_recovery_marker"], true);
    assert!(
        request
            .idempotency_key
            .as_deref()
            .unwrap()
            .starts_with("memory-source-sync:update:mem_sync:")
    );
}

#[test]
fn terminal_memory_uses_the_same_source_identity_for_cleanup() {
    let request = source_request(&record(MemoryStatus::Forgotten), "forget");
    assert_eq!(request.source, "memory://mem_sync");
    assert_eq!(request.refresh, SourceRefreshPolicy::Force);
}

#[test]
fn idempotency_changes_with_authoritative_record_content() {
    let first = source_request(&record(MemoryStatus::Active), "update");
    let mut changed = record(MemoryStatus::Active);
    changed.body.push_str(" changed");
    let second = source_request(&changed, "update");
    assert_ne!(first.idempotency_key, second.idempotency_key);
}

#[tokio::test]
async fn recovery_accepts_a_failed_inline_job_via_idempotency_dedup() {
    use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

    let store = FakeJobWatchStore::new();
    let record = record(MemoryStatus::Active);

    // Simulate the *inline* canonical-publication attempt: it creates a durable
    // `Source` job under the memory recovery idempotency key, then lands
    // terminal-`Failed` — exactly what happens when embedding fails because TEI
    // is down. This is the row the recovery enqueue below will idempotently
    // rediscover.
    let seeded = crate::source::enqueue::enqueue_source(
        source_request(&record, "remember"),
        &store,
        Some(AuthSnapshot::trusted_system(SYNC_POLICY_VERSION)),
    )
    .await
    .expect("seed inline publication job");
    let job_id = seeded.job.expect("seeded job descriptor").job_id;
    for status in [LifecycleStatus::Running, LifecycleStatus::Failed] {
        store
            .update_status(JobStatusUpdate {
                job_id,
                source_id: None,
                status,
                phase: PipelinePhase::Embedding,
                stage_id: None,
                counts: None,
                current: None,
                message: None,
                error: None,
            })
            .await
            .expect("drive seeded job to Failed");
    }

    // The recovery enqueue reuses the same idempotency key, so it idempotently
    // returns the already-`Failed` inline row (`job = Some`, `status = Failed`).
    // A durable recovery marker exists and job recovery will retry it once the
    // data plane returns, so this MUST be accepted — regressing here reproduces
    // the mcp-smoke `url_memory_remember` failure where memory remember raised
    // "canonical publication is pending" with TEI down.
    enqueue_memory_records(&store, std::slice::from_ref(&record), "remember")
        .await
        .expect("a Failed inline job is still a valid durable recovery marker");
}

#[tokio::test]
async fn unavailable_enqueue_leaves_a_same_status_recovery_marker() {
    use axon_memory::store::{FakeMemoryStore, MemoryStore};

    let store = FakeMemoryStore::new();
    let created = store
        .remember(MemoryRequest {
            memory_type: MemoryType::Fact,
            body: "durable before publication".to_string(),
            confidence: 0.9,
            salience: 0.8,
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            title: None,
            tags: Vec::new(),
            links: Vec::new(),
            decay: None,
            embed: true,
            visibility: None,
        })
        .await
        .expect("remember");
    let before = store
        .get(created.memory_id.clone())
        .await
        .expect("load")
        .expect("record");

    mark_sync_recovery(&store, &before, "remember", "job store unavailable")
        .await
        .expect("mark recovery");

    let after = store
        .get(created.memory_id)
        .await
        .expect("load")
        .expect("record");
    assert_eq!(after.status, before.status);
    assert!(
        after
            .history
            .last()
            .expect("history marker")
            .message
            .contains("memory.source_sync_pending operation=remember")
    );
}
