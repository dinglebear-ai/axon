use super::*;
use crate::source::progress as source_progress;

struct DiscoveryState {
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    coordinator: progress::WebProgressCoordinator,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_web_pipeline(
    input: &WebSourceIndexInput,
    adapter: Arc<dyn SourceAdapter>,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    previous_source: Option<SourceSummary>,
    run: WebAdapterRun,
    lease: &LeaseGuard,
    events: &crate::source::events::SourceEventEmitter,
) -> anyhow::Result<WebSourceIndexOutput> {
    let DiscoveryState {
        mut manifest,
        diff,
        coordinator,
    } = discover_and_diff(input, adapter, ledger, &run, events).await?;
    if let Some(output) =
        unchanged_refresh_output(input, ledger, previous_source, &run, &manifest, &diff).await?
    {
        source_progress::published(
            events,
            &output.generation,
            manifest.items.len() as u64,
            &output.warnings,
            0,
            0,
        )
        .await;
        return Ok(output);
    }
    let diff = overlay_previous_web_etags(ledger, &diff).await?;
    let output = run_changed_generation(
        input,
        ledger,
        embedding_provider,
        vector_store,
        run,
        lease,
        &coordinator,
        events,
        &mut manifest,
        diff,
    )
    .await?;
    record_published_progress(&coordinator, events, manifest.items.len() as u64, &output).await;
    Ok(output)
}

async fn discover_and_diff(
    input: &WebSourceIndexInput,
    adapter: Arc<dyn SourceAdapter>,
    ledger: &dyn LedgerStore,
    run: &WebAdapterRun,
    events: &crate::source::events::SourceEventEmitter,
) -> anyhow::Result<DiscoveryState> {
    let coordinator = progress::WebProgressCoordinator::new(input, run.source_id.clone());
    coordinator
        .report(
            events,
            PipelinePhase::Discovering,
            progress::stage_counts(None, 0, None, 0, None, 0),
            "discovering web source items",
        )
        .await;
    let manifest = adapter.discover(&run.plan).await?;
    let total = manifest.items.len() as u64;
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Discovering,
            progress::stage_counts(Some(total), total, None, 0, None, 0),
            "discovered web source items",
        )
        .await;
    source_progress::discovered(events, &manifest).await;
    coordinator
        .report(
            events,
            PipelinePhase::Diffing,
            progress::stage_counts(Some(total), 0, None, 0, None, 0),
            "diffing web source manifest",
        )
        .await;
    let diff = ledger.diff_manifest(manifest.clone()).await?;
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Diffing,
            progress::stage_counts(Some(total), total, None, 0, None, 0),
            "diffed web source manifest",
        )
        .await;
    source_progress::diffed(events, &diff).await;
    Ok(DiscoveryState {
        manifest,
        diff,
        coordinator,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_changed_generation(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    run: WebAdapterRun,
    lease: &LeaseGuard,
    coordinator: &progress::WebProgressCoordinator,
    events: &crate::source::events::SourceEventEmitter,
    manifest: &mut SourceManifest,
    diff: SourceManifestDiff,
) -> anyhow::Result<WebSourceIndexOutput> {
    let changed_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    let mut pipeline_progress = progress::WebPipelineProgress::new(changed_total);
    coordinator
        .report(
            events,
            PipelinePhase::Fetching,
            pipeline_progress.fetch_start(),
            "fetching changed web source items",
        )
        .await;
    let generation = ledger.create_generation(run.source_id.clone()).await?;
    manifest.generation = generation.generation.clone();
    let diff = retarget_diff_generation(diff, &generation.generation);
    ledger.put_manifest(manifest.clone()).await?;
    match (input.scope, input.embed) {
        (SourceScope::Map, _) => {
            coordinator
                .report(
                    events,
                    PipelinePhase::Publishing,
                    progress::stage_counts(Some(1), 0, None, 0, None, 0),
                    "publishing web source generation",
                )
                .await;
            publish_map_generation(input, ledger, run, generation, manifest.clone(), diff).await
        }
        (_, false) => {
            no_vector_publish::publish_prepared_generation_without_vectors(
                no_vector_publish::NoVectorGenerationRequest {
                    input,
                    ledger,
                    run,
                    lease,
                    generation,
                    manifest: manifest.clone(),
                    diff,
                    events,
                    coordinator,
                    progress: &mut pipeline_progress,
                },
            )
            .await
        }
        (_, true) => {
            publish_vector_generation(VectorGenerationRequest {
                input,
                ledger,
                embedding_provider,
                vector_store,
                run,
                lease,
                generation,
                manifest: manifest.clone(),
                diff,
                events,
                coordinator,
                progress: &mut pipeline_progress,
            })
            .await
        }
    }
}

async fn record_published_progress(
    coordinator: &progress::WebProgressCoordinator,
    events: &crate::source::events::SourceEventEmitter,
    manifest_items: u64,
    output: &WebSourceIndexOutput,
) {
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Publishing,
            progress::stage_counts(
                Some(1),
                1,
                Some(output.documents_prepared),
                output.documents_prepared,
                Some(output.chunks_prepared),
                output.chunks_prepared,
            ),
            "published web source generation",
        )
        .await;
    source_progress::published(
        events,
        &output.generation,
        manifest_items,
        &output.warnings,
        output.documents_prepared,
        output.chunks_prepared,
    )
    .await;
}
