use super::*;

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;

pub(in crate::source) async fn index_materialized_source<'a, F, Fut>(
    runtime: &'a TargetLocalSourceRuntime,
    mut input: SourcePipelineInput<'a>,
    materialize: F,
) -> anyhow::Result<IndexCounts>
where
    F: FnOnce(SourcePlan) -> Fut + Send + 'a,
    Fut: Future<Output = anyhow::Result<MaterializedSource>> + Send + 'a,
{
    artifact_candidates::spawn_outbox_drain(runtime);
    let config_snapshot = crate::config_snapshot_hash::JobConfigSnapshot {
        source_kind: input.adapter.name(),
        source_ref: &input.plan.route.source.canonical_uri,
        collection: input.collection,
        embedding_provider_id: &runtime.embedding_provider_id.0,
        vector_provider_id: &runtime.vector_provider_id.0,
        embedding_model: &runtime.embedding_model,
        embedding_dimensions: runtime.embedding_dimensions,
        embed: input.plan.request.embed,
        max_items: input.plan.limits.effective.max_items,
    };
    let config_snapshot_material = config_snapshot.canonical_material();
    input.plan.config_snapshot_id =
        crate::config_snapshot_hash::config_snapshot_id(&config_snapshot);
    let owns_status = input.execution.existing_job_id.is_none();
    let job_id = match input.execution.existing_job_id {
        Some(job_id) => job_id,
        None => {
            runtime
                .jobs
                .create_with_config_snapshot(
                    job_create_request(&input),
                    Some(&config_snapshot_material),
                )
                .await?
                .job_id
        }
    };
    input.plan.job_id = job_id;
    input.plan.request.execution.priority = input.execution.priority;
    axon_api::source::stamp_provider_execution_metadata(
        &mut input.plan.request.metadata,
        axon_api::source::ProviderExecutionMetadata {
            job_id,
            attempt: input.execution.attempt,
            priority: input.execution.priority,
        },
    );
    if let Some(foreground) = &input.execution.foreground {
        foreground.job_started(job_id);
    }
    let emitter = SourceEventEmitter::new(Some(runtime.jobs.clone()), Some(job_id))
        .with_route(
            input.plan.route.source.source_kind,
            input.plan.route.scope,
            input.plan.route.adapter.clone(),
        )
        .with_source(
            input.plan.route.source.source_id.clone(),
            input.plan.route.source.canonical_uri.clone(),
        )
        .with_attempt(input.execution.attempt)
        .with_optional_foreground(input.execution.foreground.clone());

    let result = run_with_lease(runtime, &mut input, &emitter, materialize).await;
    let release_result = release_adapter(runtime, &input).await;
    let merged = merge_pipeline_results(runtime, result, Ok(()), release_result).await;
    if owns_status && merged.is_err() {
        let status_result = record_terminal_status(runtime.jobs.as_ref(), &input, &merged).await;
        return merge_pipeline_results(
            runtime,
            merged,
            status_result,
            Ok(AdapterReleaseOutcome::Released),
        )
        .await;
    }
    merged
}

enum AdapterReleaseOutcome {
    Released,
    Deferred(SourceWarning),
}

async fn release_adapter(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
) -> anyhow::Result<AdapterReleaseOutcome> {
    let release_request = AdapterReleaseRequest {
        job_id: input.plan.job_id,
        source_id: input.plan.route.source.source_id.clone(),
        source_kind: input.plan.route.source.source_kind,
    };
    match input.adapter.release(&release_request) {
        Ok(()) => Ok(AdapterReleaseOutcome::Released),
        Err(release_error) => {
            let debt = CleanupDebt {
                debt_id: CleanupDebtId::new(format!(
                    "debt_{}",
                    uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_URL,
                        format!(
                            "adapter-release:{}:{}",
                            input.plan.job_id.0, input.plan.route.source.source_id.0
                        )
                        .as_bytes(),
                    )
                )),
                job_id: input.plan.job_id,
                origin_attempt: input.execution.attempt,
                source_id: input.plan.route.source.source_id.clone(),
                generation: None,
                kind: CleanupDebtKind::AdapterRelease,
                selector: CleanupSelector::Source {
                    source_id: input.plan.route.source.source_id.clone(),
                },
                vector_collection: None,
                status: LifecycleStatus::Pending,
                created_at: timestamp(),
                attempts: 1,
                last_error: Some(SourceError {
                    code: release_error.code.to_string(),
                    severity: Severity::Warning,
                    message: release_error.message.clone(),
                    source_item_key: None,
                    retryable: release_error.retryable,
                    provider_id: release_error.provider_id.clone().map(ProviderId::new),
                    cause: Some(release_error.to_string()),
                }),
                next_retry_at: Some(Timestamp::from(
                    chrono::Utc::now() + chrono::Duration::seconds(2),
                )),
                completed_at: None,
            };
            match runtime.ledger.record_cleanup_debt(debt).await {
                Ok(()) => Ok(AdapterReleaseOutcome::Deferred(deferred_warning(
                    "source.adapter.release_deferred",
                    format!(
                        "adapter release failed and was persisted as durable cleanup debt: {release_error}"
                    ),
                ))),
                Err(debt_error) => Err(anyhow::Error::new(release_error).context(format!(
                    "also failed to persist adapter cleanup debt: {debt_error}"
                ))),
            }
        }
    }
}

async fn merge_pipeline_results(
    runtime: &TargetLocalSourceRuntime,
    result: anyhow::Result<IndexCounts>,
    status_result: anyhow::Result<()>,
    release_result: anyhow::Result<AdapterReleaseOutcome>,
) -> anyhow::Result<IndexCounts> {
    match (result, status_result, release_result) {
        (Ok(output), Ok(()), Ok(AdapterReleaseOutcome::Released)) => Ok(output),
        (Err(error), Ok(()), Ok(_)) => Err(error),
        (Ok(mut output), Ok(()), Ok(AdapterReleaseOutcome::Deferred(warning))) => {
            output.warnings.push(warning);
            persist_degraded_summary(runtime, &mut output).await;
            Ok(output)
        }
        (Ok(_), Ok(()), Err(release_error)) => Err(release_error),
        (Err(error), Ok(()), Err(release_error)) => Err(error.context(format!(
            "adapter cleanup also failed: {release_error}"
        ))),
        (Ok(mut output), Err(status_error), Ok(release_outcome)) => {
            if let AdapterReleaseOutcome::Deferred(warning) = release_outcome {
                output.warnings.push(warning);
            }
            output.warnings.push(deferred_warning(
                "source.job.terminal_status_deferred",
                format!(
                    "generation {} was published, but persisting the terminal job status failed: {status_error}",
                    output.generation.0
                ),
            ));
            persist_degraded_summary(runtime, &mut output).await;
            Ok(output)
        }
        (Err(error), Err(status_error), Ok(_)) => Err(error.context(format!(
            "terminal job status update also failed: {status_error}"
        ))),
        (Ok(_), Err(status_error), Err(release_error)) => Err(anyhow::anyhow!(
            "terminal job status update failed: {status_error}; adapter cleanup also failed: {release_error}"
        )),
        (Err(error), Err(status_error), Err(release_error)) => Err(error.context(format!(
            "terminal job status update also failed: {status_error}; adapter cleanup also failed: {release_error}"
        ))),
    }
}
