use axon_api::source::*;
use axon_ledger::store::LedgerStore;

use super::vectorize::VectorizeResult;
use super::{SOURCE_LEASE_TTL_SECONDS, SourcePipelineInput, timestamp};
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ProviderCallContext};

pub(super) struct PublishOutcome {
    pub(super) generation: SourceGeneration,
    pub(super) warnings: Vec<SourceWarning>,
}

pub(super) async fn ensure_lease(
    ledger: &dyn LedgerStore,
    input: &SourcePipelineInput<'_>,
    lease: &LeaseGuard,
) -> anyhow::Result<()> {
    if ledger
        .heartbeat_lease(
            lease.lease_id.clone(),
            input.owner_id.to_string(),
            SOURCE_LEASE_TTL_SECONDS,
        )
        .await?
        .is_some()
    {
        return Ok(());
    }
    anyhow::bail!("source refresh lost lease before publish")
}

pub(super) async fn complete_generation(
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    diff: &SourceManifestDiff,
    discovered: u64,
    vectorized: &VectorizeResult,
) -> anyhow::Result<SourceGeneration> {
    Ok(ledger
        .complete_generation(SourceGeneration {
            status: LifecycleStatus::Completed,
            publish_state: PublishState::Publishing,
            published_at: None,
            item_counts: ItemCounts {
                added: diff.counts.added,
                modified: diff.counts.modified,
                removed: diff.counts.removed,
                unchanged: diff.counts.unchanged,
                failed: diff.counts.failed,
            },
            document_counts: DocumentCounts {
                discovered,
                prepared: vectorized.documents_prepared,
                embedded: if vectorized.points_written > 0 {
                    vectorized.documents_prepared
                } else {
                    0
                },
                published: vectorized.documents_prepared,
                failed: 0,
            },
            ..generation
        })
        .await?)
}

pub(super) async fn publish(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    generation: &SourceGeneration,
    diff: &SourceManifestDiff,
    embed: bool,
    expected_new_points: u64,
) -> anyhow::Result<PublishOutcome> {
    if !embed {
        return Ok(PublishOutcome {
            generation: publish_ledger(runtime.ledger.as_ref(), input, generation).await?,
            warnings: Vec::new(),
        });
    }

    if let Err(error) = stage_vector_visibility(
        runtime,
        input,
        collection,
        generation,
        diff,
        expected_new_points,
    )
    .await
    {
        return Err(
            rollback_new_generation_vectors(runtime, input, collection, generation, error).await,
        );
    }

    let published = match publish_ledger(runtime.ledger.as_ref(), input, generation).await {
        Ok(published) => published,
        Err(error) => {
            return Err(rollback_new_generation_vectors(
                runtime, input, collection, generation, error,
            )
            .await);
        }
    };

    let mut warnings = Vec::new();
    if let Some(previous) = generation.previous_generation.clone()
        && let Err(error) = reserved_call::retire_generation(
            runtime,
            publish_context(input, generation, "retire-previous-generation"),
            collection.collection.clone(),
            generation.source_id.clone(),
            previous.clone(),
            generation.generation.clone(),
        )
        .await
    {
        warnings.push(
            record_retirement_debt(
                runtime,
                input,
                generation,
                previous,
                &collection.collection,
                error,
            )
            .await,
        );
    }

    Ok(PublishOutcome {
        generation: published,
        warnings,
    })
}

async fn stage_vector_visibility(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    generation: &SourceGeneration,
    diff: &SourceManifestDiff,
    expected_new_points: u64,
) -> anyhow::Result<()> {
    let new_write = reserved_call::mark_generation_committed(
        runtime,
        publish_context(input, generation, "mark-generation-committed"),
        collection.collection.clone(),
        generation.source_id.clone(),
        generation.generation.clone(),
    )
    .await?;
    ensure_full_write("mark_generation_committed", expected_new_points, &new_write)?;

    if let Some(previous) = generation.previous_generation.clone()
        && !diff.unchanged.is_empty()
    {
        let unchanged_write = reserved_call::mark_unchanged_items_committed(
            runtime,
            publish_context(input, generation, "carry-unchanged-items"),
            collection.collection.clone(),
            generation.source_id.clone(),
            previous,
            generation.generation.clone(),
            diff.unchanged
                .iter()
                .map(|item| item.source_item_key.clone())
                .collect(),
        )
        .await?;
        ensure_full_write(
            "mark_unchanged_items_committed",
            unchanged_write.points_attempted,
            &unchanged_write,
        )?;
    }
    Ok(())
}

async fn publish_ledger(
    ledger: &dyn LedgerStore,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
) -> anyhow::Result<SourceGeneration> {
    Ok(ledger
        .publish_generation(PublishGenerationRequest {
            job_id: input.plan.job_id,
            attempt: input.execution.attempt,
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation.clone(),
        })
        .await?)
}

async fn rollback_new_generation_vectors(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &CollectionSpec,
    generation: &SourceGeneration,
    cause: impl Into<anyhow::Error>,
) -> anyhow::Error {
    let error = cause.into();
    match reserved_call::delete_vectors(
        runtime,
        publish_context(input, generation, "rollback-new-generation"),
        VectorDeleteSelector::Generation {
            collection: collection.collection.clone(),
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
        },
    )
    .await
    {
        Ok(_) => error,
        Err(rollback) => error.context(format!(
            "also failed to rollback vector generation {} from collection {}: {rollback}",
            generation.generation.0, collection.collection
        )),
    }
}

pub(super) async fn cleanup_failed_generation_vectors(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    collection: &str,
    generation: &SourceGeneration,
) -> anyhow::Result<()> {
    match reserved_call::delete_vectors(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Cleaning,
            input.execution.priority,
            format!("delete-failed-generation:{}", generation.generation.0),
        ),
        VectorDeleteSelector::Generation {
            collection: collection.to_string(),
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
        },
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(error) => {
            let debt = CleanupDebt {
                debt_id: CleanupDebtId::new(format!(
                    "debt_{}",
                    uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_URL,
                        format!(
                            "failed-generation-vector-delete:{}:{}",
                            generation.source_id.0, generation.generation.0
                        )
                        .as_bytes(),
                    )
                )),
                job_id: input.plan.job_id,
                origin_attempt: input.execution.attempt,
                source_id: generation.source_id.clone(),
                generation: Some(generation.generation.clone()),
                kind: CleanupDebtKind::VectorDelete,
                selector: CleanupSelector::Generation {
                    source_id: generation.source_id.clone(),
                    generation: generation.generation.clone(),
                },
                vector_collection: Some(collection.to_string()),
                status: LifecycleStatus::Pending,
                created_at: timestamp(),
                attempts: 0,
                last_error: Some(SourceError {
                    code: error.code.to_string(),
                    severity: Severity::Warning,
                    message: error.message.clone(),
                    source_item_key: None,
                    retryable: error.retryable,
                    provider_id: error.provider_id.clone().map(ProviderId::new),
                    cause: Some(error.to_string()),
                }),
                next_retry_at: None,
                completed_at: None,
            };
            match runtime.ledger.record_cleanup_debt(debt).await {
                Ok(()) => Err(anyhow::Error::new(error).context(
                    "failed-generation vector cleanup was deferred to durable cleanup debt",
                )),
                Err(debt_error) => Err(anyhow::Error::new(error).context(format!(
                    "also failed to record failed-generation vector cleanup debt: {debt_error}"
                ))),
            }
        }
    }
}

async fn record_retirement_debt(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
    previous: SourceGenerationId,
    collection: &str,
    error: ApiError,
) -> SourceWarning {
    let debt = CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "retire:{}:{}:{}",
                    generation.source_id.0, previous.0, generation.generation.0
                )
                .as_bytes(),
            )
        )),
        job_id: input.plan.job_id,
        origin_attempt: input.execution.attempt,
        source_id: generation.source_id.clone(),
        generation: Some(previous.clone()),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::Generation {
            source_id: generation.source_id.clone(),
            generation: previous.clone(),
        },
        vector_collection: Some(collection.to_string()),
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: Some(SourceError {
            code: error.code.to_string(),
            severity: Severity::Warning,
            message: error.message.clone(),
            source_item_key: None,
            retryable: error.retryable,
            provider_id: error.provider_id.clone().map(ProviderId::new),
            cause: Some(error.to_string()),
        }),
        next_retry_at: None,
        completed_at: None,
    };
    let debt_error = runtime.ledger.record_cleanup_debt(debt).await.err();
    let message = match debt_error {
        Some(debt_error) => format!(
            "published generation {}, but retiring previous generation {} failed: {}; cleanup debt could not be recorded: {}",
            generation.generation.0, previous.0, error.message, debt_error
        ),
        None => format!(
            "published generation {}, but retiring previous generation {} failed and was deferred to cleanup debt: {}",
            generation.generation.0, previous.0, error.message
        ),
    };
    SourceWarning {
        code: "source.publish.retirement_deferred".to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: None,
        retryable: true,
    }
}

fn ensure_full_write(
    operation: &str,
    expected_points: u64,
    write: &VectorStoreWriteResult,
) -> anyhow::Result<()> {
    if write.points_attempted != write.points_written || write.points_written != expected_points {
        anyhow::bail!(
            "{operation} wrote {} of {} attempted points; expected {expected_points}",
            write.points_written,
            write.points_attempted
        );
    }
    Ok(())
}

fn publish_context(
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
    operation: &str,
) -> ProviderCallContext {
    ProviderCallContext::for_phase(
        input.plan.job_id,
        input.execution.attempt,
        PipelinePhase::Publishing,
        input.execution.priority,
        format!("{operation}:{}", generation.generation.0),
    )
}
