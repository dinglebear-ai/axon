use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

fn warning_batch() -> PreparedBatchSideEffects {
    let mut batch = PreparedBatchSideEffects::empty();
    batch.warnings.push(SourceWarning {
        code: "spool.test".into(),
        severity: Severity::Warning,
        message: "x".repeat(1024),
        source_item_key: None,
        retryable: false,
    });
    batch
}

#[tokio::test]
async fn total_generation_side_effects_are_bounded_with_and_without_spill() {
    for spill in [false, true] {
        let mut accumulated = GenerationAccumulator::default();
        accumulated.side_effect_limit = Some(4096);
        if spill {
            accumulated.spool = Some(GenerationSpool::temporary("bounded-test").unwrap());
        }
        let mut rejected = false;
        for _ in 0..16 {
            if accumulated
                .absorb_pretracked_side_effects(warning_batch())
                .await
                .is_err()
            {
                rejected = true;
                break;
            }
        }
        assert!(
            rejected,
            "spilling must not bypass the total finalization budget (spill={spill})"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn blocking_spool_append_does_not_stall_the_runtime() {
    let mut accumulated = GenerationAccumulator::default();
    accumulated.spool = Some(GenerationSpool::temporary("runtime-progress").unwrap());
    let entered = Arc::new(tokio::sync::Notify::new());
    let notified = entered.clone();
    let (heartbeat, received) = std::sync::mpsc::channel();
    let heartbeat_task = tokio::spawn(async move {
        notified.notified().await;
        heartbeat.send(()).unwrap();
    });
    let progressed = Arc::new(AtomicBool::new(false));
    let observed = progressed.clone();
    accumulated.append_hook = Some(Box::new(move || {
        entered.notify_one();
        observed.store(
            received.recv_timeout(Duration::from_millis(200)).is_ok(),
            Ordering::SeqCst,
        );
    }));
    accumulated
        .absorb_pretracked_side_effects(warning_batch())
        .await
        .unwrap();
    assert!(
        progressed.load(Ordering::SeqCst),
        "heartbeat must progress while spool I/O is blocked"
    );
    heartbeat_task.await.unwrap();
}

#[tokio::test]
async fn ambiguous_spool_failure_replays_exactly_once_and_preserves_the_budget() {
    let mut state = GenerationAccumulator::default();
    state.spool = Some(GenerationSpool::temporary("ambiguous-append").unwrap());
    state
        .absorb_pretracked_side_effects(warning_batch())
        .await
        .unwrap();
    state.spool.as_mut().unwrap().inject_failure_after_flush();
    state
        .absorb_pretracked_side_effects(warning_batch())
        .await
        .unwrap();
    assert!(state.spool.is_none());
    assert_eq!(
        state.warnings.len(),
        2,
        "the readable failed append must not be replayed twice"
    );
    state.side_effect_limit = Some(state.side_effect_bytes);
    assert!(
        state
            .absorb_pretracked_side_effects(warning_batch())
            .await
            .is_err()
    );
    state
        .blocking_step(GenerationAccumulator::replay_spool)
        .await
        .unwrap();
    assert_eq!(state.warnings.len(), 2);
}

#[tokio::test]
async fn skipped_status_accounting_survives_spill_and_memory_paths() {
    for spill in [false, true] {
        let mut state = GenerationAccumulator::default();
        if spill {
            state.spool = Some(GenerationSpool::temporary("skip-test").unwrap());
        }
        let mut batch = PreparedBatchSideEffects::empty();
        batch.skipped_statuses.push(DocumentStatus {
            document_id: DocumentId::new("skip-document"),
            source_id: SourceId::new("skip-source"),
            source_item_key: SourceItemKey::new("asset"),
            generation: Some(SourceGenerationId::new("1")),
            status: DocumentLifecycleStatus::Skipped,
            updated_at: Timestamp("2026-01-01T00:00:00Z".into()),
            chunk_count: 0,
            vector_point_count: 0,
            error: None,
            cleanup_status: None,
        });
        state.absorb_pretracked_side_effects(batch).await.unwrap();
        state.replay_spool().unwrap();
        assert_eq!(state.vectorized.documents_skipped, 1);
        assert_eq!(state.vectorized.documents_prepared, 0);
        assert_eq!(state.vectorized.points_written, 0);
    }
}
