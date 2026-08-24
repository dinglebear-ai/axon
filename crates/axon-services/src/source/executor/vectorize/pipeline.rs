use super::super::vector_points::VectorPointBuild;
use super::*;
use crate::reserved_call::ProviderCallContext;
use std::future::Future;

pub(super) struct BuiltVectorBatch {
    documents: Vec<PreparedDocument>,
    embedding_warnings: Vec<SourceWarning>,
    point_batch: VectorPointBatch,
    points_by_document: std::collections::BTreeMap<DocumentId, u32>,
    skipped_redaction: u64,
    redaction_skips_by_source_item: std::collections::BTreeMap<SourceItemKey, u64>,
}

async fn join_upsert_and_embedding<Write, Embeddings, Upsert, Embed>(
    upsert: Upsert,
    embedding: Embed,
) -> (anyhow::Result<Write>, anyhow::Result<Embeddings>)
where
    Upsert: Future<Output = anyhow::Result<Write>>,
    Embed: Future<Output = anyhow::Result<Embeddings>>,
{
    tokio::join!(upsert, embedding)
}

fn resolve_upsert_completion<Write, Embeddings>(
    write: anyhow::Result<Write>,
    embeddings: &anyhow::Result<Embeddings>,
) -> anyhow::Result<Write> {
    match write {
        Ok(write) => Ok(write),
        Err(primary) => {
            let Err(secondary) = embeddings else {
                return Err(primary);
            };
            tracing::error!(
                primary_error = %primary,
                secondary_error = %secondary,
                "vector upsert and overlapped embedding both failed"
            );
            Err(primary.context(format!(
                "overlapped next-batch embedding also failed: {secondary:#}"
            )))
        }
    }
}

async fn resolve_and_checkpoint_overlap(
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    output: &mut VectorizeResult,
    write: anyhow::Result<VectorStoreWriteResult>,
    embeddings: anyhow::Result<EmbeddingResult>,
    absorb: impl FnOnce(VectorStoreWriteResult) -> VectorizeResult,
) -> anyhow::Result<EmbeddingResult> {
    let write = resolve_upsert_completion(write, &embeddings)?;
    finish_upsert(coordinator, progress, &write).await;
    // The current batch's write is durably checkpointed; absorb its
    // accounting before the speculative embedding result can fail the step,
    // mirroring `batches.rs`'s absorb-before-error policy (2026-08-23
    // adversarial pipeline review, low: dropped overlapped upsert
    // accounting).
    merge_vectorize_result(output, absorb(write));
    let embeddings = embeddings?;
    finish_embedding(coordinator, progress, &embeddings).await;
    Ok(embeddings)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn embed_and_build_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<PreparedDocument>,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    is_final_vector_batch: bool,
) -> anyhow::Result<BuiltVectorBatch> {
    let mut embeddings =
        embed_prepared_batch(runtime, input, &documents, emitter, coordinator, progress).await?;
    build_vector_batch(
        documents,
        collection,
        &mut embeddings,
        emitter,
        coordinator,
        progress,
        is_final_vector_batch,
    )
    .await
}

async fn build_vector_batch(
    documents: Vec<PreparedDocument>,
    collection: CollectionSpec,
    embeddings: &mut EmbeddingResult,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    is_final_vector_batch: bool,
) -> anyhow::Result<BuiltVectorBatch> {
    let VectorPointBuild {
        batch: point_batch,
        skipped_redaction,
        redaction_skips_by_source_item,
        points_by_document,
    } = point_batch(collection, &documents, embeddings)?;
    coordinator
        .report(
            emitter,
            PipelinePhase::Vectorizing,
            progress.vectorized(point_batch.points.len() as u64, is_final_vector_batch),
            "built vector point batch",
        )
        .await;
    Ok(BuiltVectorBatch {
        documents,
        embedding_warnings: std::mem::take(&mut embeddings.warnings),
        point_batch,
        points_by_document,
        skipped_redaction,
        redaction_skips_by_source_item,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn publish_and_build_next(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    current: BuiltVectorBatch,
    next_documents: Vec<PreparedDocument>,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    output: &mut VectorizeResult,
    is_final_vector_batch: bool,
) -> anyhow::Result<BuiltVectorBatch> {
    let BuiltVectorBatch {
        documents: current_documents,
        embedding_warnings: current_warnings,
        point_batch: current_points,
        points_by_document: current_points_by_document,
        skipped_redaction: current_skipped_redaction,
        redaction_skips_by_source_item: current_redaction_skips,
    } = current;
    let upsert_counts = begin_upsert(emitter, coordinator, progress).await;
    // The next batch embeds speculatively while the current batch remains the
    // externally active Upserting phase. Publish Embedding only after the
    // current write has been accounted, preserving monotonic phase order —
    // including in the durable provider heartbeats, which report the still-
    // published Upserting phase for the speculative call (finding M2).
    let heartbeat_counts = upsert_counts.clone();

    let upsert = call_upsert(runtime, input, current_points, upsert_counts);
    let embedding = call_embedding(
        runtime,
        input,
        &next_documents,
        PipelinePhase::Upserting,
        heartbeat_counts,
    );
    let (write, embeddings) = join_upsert_and_embedding(upsert, embedding).await;

    // Preserve batch ordering even though provider work overlaps: account for
    // the current publication before exposing the next embedding result.
    let mut embeddings =
        resolve_and_checkpoint_overlap(coordinator, progress, output, write, embeddings, |write| {
            vectorize_result(
                current_documents,
                current_warnings,
                &current_points_by_document,
                write,
                current_skipped_redaction,
                &current_redaction_skips,
            )
        })
        .await?;

    build_vector_batch(
        next_documents,
        collection,
        &mut embeddings,
        emitter,
        coordinator,
        progress,
        is_final_vector_batch,
    )
    .await
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

pub(super) async fn publish_built_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    built: BuiltVectorBatch,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<VectorizeResult> {
    let BuiltVectorBatch {
        documents,
        embedding_warnings,
        point_batch,
        points_by_document,
        skipped_redaction,
        redaction_skips_by_source_item,
    } = built;
    let write =
        upsert_vector_batch(runtime, input, point_batch, emitter, coordinator, progress).await?;
    Ok(vectorize_result(
        documents,
        embedding_warnings,
        &points_by_document,
        write,
        skipped_redaction,
        &redaction_skips_by_source_item,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn embed_prepared_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<EmbeddingResult> {
    let counts = begin_embedding(emitter, coordinator, progress).await;
    let result =
        call_embedding(runtime, input, documents, PipelinePhase::Embedding, counts).await?;
    finish_embedding(coordinator, progress, &result).await;
    Ok(result)
}

async fn begin_embedding(
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &PipelineProgress,
) -> StageCounts {
    let counts = progress.embedding_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Embedding,
            counts.clone(),
            "embedding prepared chunks",
        )
        .await;
    counts
}

async fn finish_embedding(
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    result: &EmbeddingResult,
) {
    coordinator
        .checkpoint(
            PipelinePhase::Embedding,
            progress.embedded(result.vectors.len() as u64),
            "embedded prepared chunks",
        )
        .await;
}

async fn call_embedding(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
    heartbeat_phase: PipelinePhase,
    counts: StageCounts,
) -> anyhow::Result<EmbeddingResult> {
    let batch = embedding_batch(runtime, input, documents)?;
    let operation = format!("embed:{}", batch.batch_id.0);
    // Reservation identity (stage id / fence) always stays Embedding; only
    // the durable heartbeat reports `heartbeat_phase`, so a speculative
    // embedding overlapped with a still-active Upserting write never
    // publishes a job phase ahead of the ProgressCoordinator's snapshots
    // (finding M2).
    let mut context = ProviderCallContext::for_phase(
        input.plan.job_id,
        input.execution.attempt,
        PipelinePhase::Embedding,
        input.execution.priority,
        operation,
    )
    .with_counts(counts);
    context.phase = Some(heartbeat_phase);
    Ok(reserved_call::embed(runtime, context, batch).await?)
}

#[allow(clippy::too_many_arguments)]
async fn upsert_vector_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    batch: VectorPointBatch,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<VectorStoreWriteResult> {
    let counts = begin_upsert(emitter, coordinator, progress).await;
    let write = call_upsert(runtime, input, batch, counts).await?;
    finish_upsert(coordinator, progress, &write).await;
    Ok(write)
}

async fn begin_upsert(
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &PipelineProgress,
) -> StageCounts {
    let counts = progress.upserting_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Upserting,
            counts.clone(),
            "upserting vector point batch",
        )
        .await;
    counts
}

async fn finish_upsert(
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    write: &VectorStoreWriteResult,
) {
    coordinator
        .checkpoint(
            PipelinePhase::Upserting,
            progress.upserted(write.points_written),
            "upserted vector point batch",
        )
        .await;
}

async fn call_upsert(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    batch: VectorPointBatch,
    counts: StageCounts,
) -> anyhow::Result<VectorStoreWriteResult> {
    let expected_points = batch.points.len() as u64;
    let operation = format!("upsert:{}", batch.batch_id.0);
    let write = reserved_call::upsert(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Upserting,
            input.execution.priority,
            operation,
        )
        .with_counts(counts),
        batch,
    )
    .await?;
    validate_upsert_counts(
        expected_points,
        write.points_attempted,
        write.points_written,
    )?;
    Ok(write)
}
