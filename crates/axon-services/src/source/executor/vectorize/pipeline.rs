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
    write: anyhow::Result<VectorStoreWriteResult>,
    embeddings: anyhow::Result<EmbeddingResult>,
) -> anyhow::Result<(VectorStoreWriteResult, EmbeddingResult)> {
    let write = resolve_upsert_completion(write, &embeddings)?;
    coordinator
        .checkpoint(
            PipelinePhase::Upserting,
            progress.upserted(write.points_written),
            "upserted vector point batch",
        )
        .await;
    let embeddings = embeddings?;
    coordinator
        .checkpoint(
            PipelinePhase::Embedding,
            progress.embedded(embeddings.vectors.len() as u64),
            "embedded prepared chunks",
        )
        .await;
    Ok((write, embeddings))
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
    is_final_vector_batch: bool,
) -> anyhow::Result<(VectorizeResult, BuiltVectorBatch)> {
    let BuiltVectorBatch {
        documents: current_documents,
        embedding_warnings: current_warnings,
        point_batch: current_points,
        points_by_document: current_points_by_document,
        skipped_redaction: current_skipped_redaction,
        redaction_skips_by_source_item: current_redaction_skips,
    } = current;
    let upsert_counts = progress.upserting_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Upserting,
            upsert_counts.clone(),
            "upserting vector point batch",
        )
        .await;
    let embedding_counts = progress.embedding_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Embedding,
            embedding_counts.clone(),
            "embedding prepared chunks",
        )
        .await;

    let upsert = call_upsert(runtime, input, current_points, upsert_counts);
    let embedding = call_embedding(runtime, input, &next_documents, embedding_counts);
    let (write, embeddings) = join_upsert_and_embedding(upsert, embedding).await;

    // Preserve batch ordering even though provider work overlaps: account for
    // the current publication before exposing the next embedding result.
    let (write, mut embeddings) =
        resolve_and_checkpoint_overlap(coordinator, progress, write, embeddings).await?;

    let result = vectorize_result(
        current_documents,
        current_warnings,
        &current_points_by_document,
        write,
        current_skipped_redaction,
        &current_redaction_skips,
    );
    let next = build_vector_batch(
        next_documents,
        collection,
        &mut embeddings,
        emitter,
        coordinator,
        progress,
        is_final_vector_batch,
    )
    .await?;
    Ok((result, next))
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
    let counts = progress.embedding_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Embedding,
            counts.clone(),
            "embedding prepared chunks",
        )
        .await;
    let result = call_embedding(runtime, input, documents, counts).await?;
    coordinator
        .checkpoint(
            PipelinePhase::Embedding,
            progress.embedded(result.vectors.len() as u64),
            "embedded prepared chunks",
        )
        .await;
    Ok(result)
}

async fn call_embedding(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
    counts: StageCounts,
) -> anyhow::Result<EmbeddingResult> {
    let batch = embedding_batch(runtime, input, documents)?;
    let operation = format!("embed:{}", batch.batch_id.0);
    Ok(reserved_call::embed(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Embedding,
            input.execution.priority,
            operation,
        )
        .with_counts(counts),
        batch,
    )
    .await?)
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
    let counts = progress.upserting_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Upserting,
            counts.clone(),
            "upserting vector point batch",
        )
        .await;
    let write = call_upsert(runtime, input, batch, counts).await?;
    coordinator
        .checkpoint(
            PipelinePhase::Upserting,
            progress.upserted(write.points_written),
            "upserted vector point batch",
        )
        .await;
    Ok(write)
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
