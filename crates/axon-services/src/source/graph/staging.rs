//! Concurrent private graph construction and cross-domain publication.
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ProviderCallContext};
use crate::source::{executor::SourcePipelineInput, result_map::IndexCounts};
use axon_api::source::*;
use axon_graph::stage::GraphStage;
use std::sync::Arc;

pub(in crate::source) async fn stage_prepared(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
    heartbeat: PipelinePhase,
) -> anyhow::Result<()> {
    let Some(stage) = &input.graph_stage else {
        return Ok(());
    };
    let candidates = super::filter_valid_candidates(
        documents
            .iter()
            .flat_map(|d| d.graph_candidates.iter().cloned())
            .collect(),
        &input.plan.route.source.source_id,
    );
    if candidates.is_empty() {
        return Ok(());
    }
    let stage = Arc::clone(stage);
    let mut context = graph_context(input, format!("stage:{}", stage.id()));
    context.phase = Some(heartbeat);
    let started = std::time::Instant::now();
    let runtime = runtime.clone();
    let job_id = input.plan.job_id;
    run_staging_independently(async move {
        let result = reserved_call::graph_operation(&runtime, context, move || async move {
            stage.write_candidates(candidates).await
        })
        .await??;
        tracing::info!(job_id=?job_id, nodes=result.nodes_upserted,
            edges=result.edges_upserted, elapsed_ms=started.elapsed().as_millis() as u64,
            "private generation graph batch staged");
        Ok(())
    })
    .await
}

pub(in crate::source) async fn stage_baseline(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
    manifest: &SourceManifest,
    statuses: &[DocumentStatus],
) -> anyhow::Result<()> {
    let Some(stage) = &input.graph_stage else {
        return Ok(());
    };
    let graph_manifest = eligible_manifest(runtime, input, generation, manifest, statuses).await?;
    let counts = IndexCounts {
        documents_skipped: 0,
        job_id: input.plan.job_id,
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        items_discovered: manifest.items.len() as u64,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        published_manifest: None,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    };
    for candidate in super::baseline_candidates(
        input.plan.route.source.source_kind,
        &counts,
        &input.plan.route.source.canonical_uri,
        &graph_manifest,
    ) {
        let stage = Arc::clone(stage);
        reserved_call::graph_operation(
            runtime,
            graph_context(input, format!("baseline:{}", stage.id())),
            move || async move { stage.write_candidates(vec![candidate]).await },
        )
        .await??;
    }
    Ok(())
}

async fn eligible_manifest(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
    manifest: &SourceManifest,
    statuses: &[DocumentStatus],
) -> anyhow::Result<SourceManifest> {
    let mut graph_manifest = manifest.clone();
    // Mapping records discovered inventory without preparing or indexing documents.
    if input.plan.route.scope == SourceScope::Map {
        graph_manifest.items.clear();
        return Ok(graph_manifest);
    }
    let historical = runtime
        .ledger
        .document_statuses_for_items(
            manifest.source_id.clone(),
            manifest
                .items
                .iter()
                .map(|i| i.source_item_key.clone())
                .collect(),
        )
        .await?;
    let mut dispositions =
        std::collections::BTreeMap::<SourceItemKey, Vec<DocumentLifecycleStatus>>::new();
    let mut current =
        std::collections::BTreeMap::<SourceItemKey, Vec<DocumentLifecycleStatus>>::new();
    for status in historical {
        if status.generation.as_ref() == Some(&manifest.generation) {
            current
                .entry(status.source_item_key)
                .or_default()
                .push(status.status);
        } else if status.generation.as_ref() == generation.previous_generation.as_ref() {
            dispositions
                .entry(status.source_item_key)
                .or_default()
                .push(status.status);
        }
    }
    for status in statuses {
        if status.source_id != manifest.source_id
            || status.generation.as_ref() != Some(&manifest.generation)
        {
            return Err(ApiError::new(
                "graph.publication_status_stale",
                ErrorStage::Graphing,
                "graph item disposition does not belong to the publishing generation",
            )
            .into());
        }
        current
            .entry(status.source_item_key.clone())
            .or_default()
            .push(status.status);
    }
    dispositions.extend(current);
    for item in &manifest.items {
        let states = dispositions.get(&item.source_item_key).ok_or_else(|| ApiError::new(
            "graph.publication_status_unknown", ErrorStage::Graphing,
            "staging cannot establish final item disposition; inspect document statuses before retry",
        ).with_source_id(manifest.source_id.0.clone()))?;
        if states.is_empty()
            || states.iter().any(|s| {
                !matches!(
                    s,
                    DocumentLifecycleStatus::Prepared
                        | DocumentLifecycleStatus::Embedded
                        | DocumentLifecycleStatus::Vectorized
                        | DocumentLifecycleStatus::Published
                        | DocumentLifecycleStatus::Skipped
                )
            })
        {
            return Err(ApiError::new(
                "graph.publication_status_unknown",
                ErrorStage::Graphing,
                "graph item disposition is not final; inspect document statuses before retry",
            )
            .into());
        }
    }
    graph_manifest.items.retain(|item| {
        dispositions[&item.source_item_key]
            .iter()
            .any(|s| *s != DocumentLifecycleStatus::Skipped)
    });
    Ok(graph_manifest)
}

fn graph_context(input: &SourcePipelineInput<'_>, operation: String) -> ProviderCallContext {
    let mut context = ProviderCallContext::for_phase(
        input.plan.job_id,
        input.execution.attempt,
        PipelinePhase::Graphing,
        input.execution.priority,
        operation,
    );
    context.phase = None;
    context
}

pub(in crate::source) async fn publish_ledger_and_graph(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    request: PublishGenerationRequest,
) -> anyhow::Result<SourceGeneration> {
    let mut context = graph_context(input, "activate-generation-graph".into());
    context.phase = Some(PipelinePhase::Publishing);
    let runtime = runtime.clone();
    let stage = input
        .graph_stage
        .clone()
        .ok_or_else(|| anyhow::anyhow!("graph stage missing at publication"))?;
    // Register settlement before spawning, including time queued for GraphLane admission.
    let settlement = runtime.publication_settlement_gate.lock().await;
    let owner = input.owner_id.to_owned();
    let cancellation = input.execution.cancellation.clone();
    let job = input.plan.job_id;
    // Once SQLite publication starts, its writer ownership survives caller cancellation.
    // Failure finalization waits on the same gate before authorizing vector deletion.
    tokio::spawn(async move {
        let _settlement = settlement;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let atomic_runtime = runtime.clone();
        let operation = reserved_call::graph_operation(&runtime, context, move || async move {
            // Reservation renewal may abandon its provider future. SQLite settlement
            // retains independent ownership and delivers its outcome before cleanup.
            tokio::spawn(async move {
                let result = publish_atomic(&atomic_runtime, &stage, job, &owner,
                    cancellation.as_ref(), request).await;
                let _ = sender.send(result);
            }).await
        }).await;
        match receiver.await {
            Ok(Ok(generation)) => {
                if let Err(error) = operation {
                    tracing::warn!(error=%error, "graph reservation settlement failed after committed publication");
                }
                Ok(generation)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(ApiError::new("graph.publication_commit_unknown", ErrorStage::Publishing,
                "publication settlement task returned no outcome; retain vectors and inspect durable graph receipt before retry").into()),
        }
    }).await?
}

async fn publish_atomic(
    runtime: &TargetLocalSourceRuntime,
    stage: &GraphStage,
    job: axon_api::JobId,
    owner: &str,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    request: PublishGenerationRequest,
) -> anyhow::Result<SourceGeneration> {
    let pool = runtime
        .graph_stage_pool
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("graph stage pool missing at publication"))?;
    stage.seal().await?;
    let _permit = runtime.db_stage_slots.clone().acquire_owned().await?;
    let _writer = runtime.sqlite_write_gate.lock().await;
    let mut connection = pool.acquire().await?;
    // An aborted attachment must never leak into a reused pooled connection.
    connection.close_on_drop();
    stage.prepare_activation(&mut connection).await?;
    let mut tx = sqlx::Transaction::<sqlx::Sqlite>::begin(
        &mut *connection,
        Some(std::borrow::Cow::Borrowed("BEGIN IMMEDIATE")),
    )
    .await?;
    let ledger = axon_ledger::sqlite::SqliteLedgerStore::from_pool(pool.clone());
    let outcome = async {
        ensure_not_cancelled(cancellation)?;
        ledger
            .ensure_publication_lease_in_tx(&mut tx, &request.source_id, owner)
            .await?;
        let generation = ledger.publish_generation_in_tx(&mut tx, request).await?;
        let start = std::time::Instant::now();
        stage.activate_in_tx(&mut tx).await?;
        tracing::info!(job_id=?job, activation_ms=start.elapsed().as_millis() as u64,
            "generation graph activation prepared in publication transaction");
        ensure_not_cancelled(cancellation)?;
        ledger
            .ensure_publication_lease_in_tx(&mut tx, &generation.source_id, owner)
            .await?;
        Ok::<_, anyhow::Error>(generation)
    }
    .await;
    let mut uncertain_generation = None;
    let settled = match outcome {
        Ok(generation) => match tx.commit().await {
            Ok(()) => Ok(generation),
            Err(error) => {
                uncertain_generation = Some(generation);
                Err(anyhow::Error::from(error))
            }
        },
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    };
    if let Err(error) = stage.detach(&mut connection).await {
        tracing::warn!(error=%error, "graph stage attachment disposal deferred to connection close");
    }
    drop(connection);
    drop(_writer);
    drop(_permit);
    if let Some(generation) = uncertain_generation {
        let receipt =
            GraphStage::activation_summary(pool, &generation.source_id, &generation.generation)
                .await;
        use axon_ledger::store::LedgerStore;
        let committed = ledger
            .committed_generation(generation.source_id.clone())
            .await;
        return match (receipt, committed) {
            (Ok(Some(_)), Ok(Some(current))) if current == generation.generation => Ok(generation),
            (Ok(None), Ok(current)) if current.as_ref() != Some(&generation.generation) => settled,
            _ => Err(ApiError::new("graph.publication_commit_unknown", ErrorStage::Publishing,
                "atomic publication outcome could not be established; retain vectors and inspect the durable graph receipt before retry").into()),
        };
    }
    settled
}

fn ensure_not_cancelled(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> anyhow::Result<()> {
    if cancellation.is_some_and(|token| token.is_cancelled()) {
        return Err(ApiError::new("graph.publication_canceled", ErrorStage::Publishing,
            "generation cancelled before atomic graph/source commit; previous generation remains visible").into());
    }
    Ok(())
}

pub(crate) async fn activated_summary(
    runtime: &TargetLocalSourceRuntime,
    counts: &IndexCounts,
) -> anyhow::Result<Option<GraphWriteSummary>> {
    let Some(pool) = &runtime.graph_stage_pool else {
        return Ok(None);
    };
    Ok(
        GraphStage::activation_summary(pool, &counts.source_id, &counts.generation)
            .await?
            .map(|summary| GraphWriteSummary {
                nodes_upserted: summary.nodes_upserted,
                edges_upserted: summary.edges_upserted,
                evidence_records: summary.evidence_records,
                degraded: false,
            }),
    )
}

async fn run_staging_independently<T: Send + 'static>(
    work: impl std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
) -> anyhow::Result<T> {
    // A buffered caller can stop polling while an earlier result awaits the
    // live writer held by this reservation's completion. Keep completion
    // progressing independently, and abort owned work when its caller drops.
    tokio_util::task::AbortOnDropHandle::new(tokio::spawn(work)).await?
}

#[cfg(test)]
mod tests;
