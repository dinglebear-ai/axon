//! Durable outbox draining for committed ArtifactCandidate batches.

use axon_api::source::PipelinePhase;

use super::{CandidateDeliveryDisposition, submit_committed_candidates_with_outcome};
use crate::context::TargetLocalSourceRuntime;
use crate::source::events::SourceEventEmitter;

pub(crate) fn spawn_outbox_drain(runtime: &TargetLocalSourceRuntime) {
    const MAX_RETRY_ATTEMPTS_PER_DRAIN: u32 = 10;
    let Some(outbox) = runtime.artifact_candidate_outbox.clone() else {
        return;
    };
    if !outbox.begin_drain() {
        return;
    }
    let sink = runtime.artifact_candidate_sink.clone();
    let ledger = runtime.ledger.clone();
    let jobs = runtime.jobs.clone();
    let task_outbox = outbox.clone();
    let task = tokio::spawn(async move {
        let mut retry_attempt = 0_u32;
        let mut retryable_in_cycle = false;
        let mut cycle_started = false;
        loop {
            if task_outbox.drain_cancelled() {
                break;
            }
            if !cycle_started {
                task_outbox.start_drain_pass();
                cycle_started = true;
            }
            let drain = drain_page(&task_outbox, sink.as_ref(), ledger.as_ref(), &jobs).await;
            let retryable_remaining = match drain {
                Ok((retryable_remaining, true)) => {
                    retryable_in_cycle |= retryable_remaining;
                    continue;
                }
                Ok((retryable_remaining, false)) => {
                    let remaining = retryable_remaining || retryable_in_cycle;
                    retryable_in_cycle = false;
                    remaining
                }
                Err(error) => {
                    tracing::warn!(error = %error, "artifact candidate outbox drain failed");
                    true
                }
            };
            cycle_started = false;
            if retryable_remaining {
                retry_attempt = retry_attempt.saturating_add(1);
                if retry_attempt >= MAX_RETRY_ATTEMPTS_PER_DRAIN {
                    tracing::warn!(
                        attempts = retry_attempt,
                        "artifact candidate outbox drain exhausted its retry budget; durable deliveries remain for the next supervised pass"
                    );
                    if task_outbox.finish_exhausted_drain() {
                        retry_attempt = 0;
                        continue;
                    }
                    break;
                }
                tokio::time::sleep(retry_delay(retry_attempt)).await;
                continue;
            }
            retry_attempt = 0;
            if !task_outbox.continue_or_finish_drain() {
                break;
            }
        }
    });
    outbox.register_drain_task(task);
}

async fn drain_page(
    outbox: &crate::artifact_candidate_outbox::ArtifactCandidateOutbox,
    sink: &dyn axon_adapters::ArtifactCandidateSink,
    ledger: &dyn axon_ledger::store::LedgerStore,
    jobs: &std::sync::Arc<dyn axon_jobs::boundary::JobStore>,
) -> anyhow::Result<(bool, bool)> {
    let mut retryable_remaining = false;
    let scan = outbox.scan().await?;
    let has_more = scan.has_more;
    for finding in scan.findings {
        tracing::warn!(
            code = finding.code,
            file_name = finding.file_name,
            "artifact candidate outbox finding observed by drain"
        );
    }
    for pending in scan.deliveries {
        let receipt = ledger
            .generation_receipt(pending.source_id.clone(), pending.generation.clone())
            .await?;
        let discardable = can_discard_stale(receipt.as_ref(), pending.staged_at_unix_ms);
        let published = if let Some(receipt) = receipt {
            receipt.published
        } else {
            // Compatibility for custom stores without the bounded receipt read.
            ledger
                .committed_generation(pending.source_id.clone())
                .await?
                .as_ref()
                == Some(&pending.generation)
        };
        if !published {
            if discardable {
                outbox.complete(&pending.delivery_key).await?;
            } else {
                retryable_remaining = true;
            }
            continue;
        }
        let outcome = submit_committed_candidates_with_outcome(
            sink,
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
    Ok::<_, anyhow::Error>((retryable_remaining, has_more))
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

fn can_discard_stale(
    receipt: Option<&axon_ledger::store::GenerationReceipt>,
    staged_at: u64,
) -> bool {
    use axon_api::source::LifecycleStatus;
    delivery_is_stale(staged_at)
        && receipt.is_some_and(|receipt| {
            !receipt.published
                && matches!(
                    receipt.status,
                    LifecycleStatus::Failed | LifecycleStatus::Canceled | LifecycleStatus::Expired
                )
        })
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use axon_api::source::{LifecycleStatus, PublishState};
    #[test]
    fn age_never_substitutes_for_a_terminal_nonpublication_receipt() {
        assert!(!can_discard_stale(None, 0));
        let mut receipt = axon_ledger::store::GenerationReceipt {
            status: LifecycleStatus::Running,
            publish_state: PublishState::Writing,
            published: false,
        };
        assert!(!can_discard_stale(Some(&receipt), 0));
        receipt.status = LifecycleStatus::Failed;
        assert!(can_discard_stale(Some(&receipt), 0));
        receipt.published = true;
        assert!(!can_discard_stale(Some(&receipt), 0));
    }
}
