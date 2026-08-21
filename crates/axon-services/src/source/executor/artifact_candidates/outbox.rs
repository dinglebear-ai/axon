//! Durable outbox draining for committed ArtifactCandidate batches.

use axon_api::source::PipelinePhase;

use super::{CandidateDeliveryDisposition, submit_committed_candidates_with_outcome};
use crate::context::TargetLocalSourceRuntime;
use crate::source::events::SourceEventEmitter;

pub(crate) fn spawn_outbox_drain(runtime: &TargetLocalSourceRuntime) {
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
        let mut retry_attempt = 0_u32;
        loop {
            outbox.start_drain_pass();
            let drain = async {
                let mut retryable_remaining = false;
                let scan = outbox.scan().await?;
                for finding in scan.findings {
                    tracing::warn!(
                        code = finding.code,
                        file_name = finding.file_name,
                        "artifact candidate outbox finding observed by drain"
                    );
                }
                for pending in scan.deliveries {
                    let committed = ledger
                        .committed_generation(pending.source_id.clone())
                        .await?;
                    if committed.as_ref() != Some(&pending.generation) {
                        if delivery_is_stale(pending.staged_at_unix_ms) {
                            outbox.complete(&pending.delivery_key).await?;
                        } else {
                            retryable_remaining = true;
                        }
                        continue;
                    }
                    let outcome = submit_committed_candidates_with_outcome(
                        sink.as_ref(),
                        pending.job_id,
                        pending.source_id.clone(),
                        &pending.generation,
                        pending.candidates,
                    )
                    .await;
                    let emitter = SourceEventEmitter::new(Some(jobs.clone()), Some(pending.job_id));
                    for warning in &outcome.warnings {
                        emitter
                            .warning(
                                PipelinePhase::Publishing,
                                warning.clone(),
                                Some(pending.generation.clone()),
                            )
                            .await;
                    }
                    match outcome.disposition {
                        CandidateDeliveryDisposition::Terminal => {
                            outbox.complete(&pending.delivery_key).await?;
                        }
                        CandidateDeliveryDisposition::Retryable => {
                            retryable_remaining = true;
                        }
                        CandidateDeliveryDisposition::Disabled => {
                            // Configuration can be restored later. Preserve the durable
                            // delivery intent without spinning a retry loop while disabled.
                        }
                    }
                }
                Ok::<bool, anyhow::Error>(retryable_remaining)
            }
            .await;
            let retryable_remaining = match drain {
                Ok(retryable_remaining) => retryable_remaining,
                Err(error) => {
                    tracing::warn!(error = %error, "artifact candidate outbox drain failed");
                    true
                }
            };
            if retryable_remaining {
                retry_attempt = retry_attempt.saturating_add(1);
                tokio::time::sleep(retry_delay(retry_attempt)).await;
                continue;
            }
            retry_attempt = 0;
            if !outbox.continue_or_finish_drain() {
                break;
            }
        }
    });
}

fn retry_delay(attempt: u32) -> std::time::Duration {
    #[cfg(test)]
    const BASE_MS: u64 = 10;
    #[cfg(not(test))]
    const BASE_MS: u64 = 1_000;
    const MAX_MS: u64 = 60_000;
    let exponent = attempt.saturating_sub(1).min(16);
    std::time::Duration::from_millis(BASE_MS.saturating_mul(1_u64 << exponent).min(MAX_MS))
}

fn delivery_is_stale(staged_at_unix_ms: u64) -> bool {
    const STALE_AFTER_MS: u64 = 24 * 60 * 60 * 1_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok());
    now.is_some_and(|now| now.saturating_sub(staged_at_unix_ms) >= STALE_AFTER_MS)
}
