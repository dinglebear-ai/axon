//! Durable outbox draining for committed ArtifactCandidate batches.

use axon_api::source::{PipelinePhase, SourceWarning};

use super::submit_committed_candidates;
use crate::context::TargetLocalSourceRuntime;
use crate::source::events::SourceEventEmitter;

pub(in crate::source::executor) fn spawn_outbox_drain(runtime: &TargetLocalSourceRuntime) {
    let Some(outbox) = runtime.artifact_candidate_outbox.clone() else {
        return;
    };
    if !outbox.begin_drain() {
        return;
    }
    let sink = runtime.artifact_candidate_sink.clone();
    let ledger = runtime.ledger.clone();
    let jobs = runtime.jobs.clone();
    tokio::spawn(async move {
        loop {
            outbox.start_drain_pass();
            let drain = async {
                for pending in outbox.pending().await? {
                    let committed = ledger
                        .committed_generation(pending.source_id.clone())
                        .await?;
                    if committed.as_ref() != Some(&pending.generation) {
                        if delivery_is_stale(pending.staged_at_unix_ms) {
                            outbox.complete(&pending.delivery_key).await?;
                        }
                        continue;
                    }
                    let warnings = submit_committed_candidates(
                        sink.as_ref(),
                        pending.job_id,
                        pending.source_id.clone(),
                        &pending.generation,
                        pending.candidates,
                    )
                    .await;
                    let emitter = SourceEventEmitter::new(Some(jobs.clone()), Some(pending.job_id));
                    for warning in &warnings {
                        emitter
                            .warning(
                                PipelinePhase::Publishing,
                                warning.clone(),
                                Some(pending.generation.clone()),
                            )
                            .await;
                    }
                    if delivery_is_terminal(&warnings) {
                        outbox.complete(&pending.delivery_key).await?;
                    }
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if let Err(error) = drain {
                tracing::warn!(error = %error, "artifact candidate outbox drain failed");
            }
            if !outbox.continue_or_finish_drain() {
                break;
            }
        }
    });
}

fn delivery_is_stale(staged_at_unix_ms: u64) -> bool {
    const STALE_AFTER_MS: u64 = 24 * 60 * 60 * 1_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok());
    now.is_some_and(|now| now.saturating_sub(staged_at_unix_ms) >= STALE_AFTER_MS)
}

fn delivery_is_terminal(warnings: &[SourceWarning]) -> bool {
    let terminal = warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "source.artifact_candidate.sink_accepted" | "source.artifact_candidate.sink_disabled"
        )
    });
    let failed = warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "source.artifact_candidate.sink_capability_failed"
                | "source.artifact_candidate.sink_contract_unsupported"
                | "source.artifact_candidate.sink_idempotency_unsupported"
                | "source.artifact_candidate.sink_zero_batch_limit"
                | "source.artifact_candidate.sink_failed"
                | "source.artifact_candidate.sink_receipt_invalid"
                | "source.artifact_candidate.sink_partial"
                | "source.artifact_candidate.sink_rejected"
                | "source.artifact_candidate.sink_delivery_skipped"
        )
    });
    terminal && !failed
}
