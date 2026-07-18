use anyhow::Context as _;
use axon_api::source::{AuthSnapshot, MetadataMap, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_info;

use super::super::SourceExecutionContext;
use super::super::result_map::IndexCounts;
use super::placeholder_job_id;
use super::web_options::{merge_caller_web_options, web_crawl_options};
use crate::WebSourceIndexInput;
use crate::context::TargetLocalSourceRuntime;
use crate::web_source::{
    WebSourceJobExecution, index_web_source_with_execution, web_source_job_create_request,
};

/// Web source: drive the `WebSourceAdapter`'s discover→acquire→normalize
/// pipeline directly — no legacy crawl-to-disk handoff.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_web(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_pages: Option<u64>,
    max_depth: Option<u32>,
    output: &axon_api::source::OutputPolicy,
    route: &axon_api::source::RoutePlan,
    source_execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=web scope={scope:?} embed={embed} max_pages={max_pages:?} max_depth={max_depth:?}"
    ));
    let mut crawl_options = web_crawl_options(cfg, max_pages, max_depth);
    merge_caller_web_options(
        &mut crawl_options,
        &route.validated_options.values,
        auth_snapshot,
    )?;
    let execution_job_id = source_execution
        .existing_job_id
        .unwrap_or_else(placeholder_job_id);
    let index_input = WebSourceIndexInput {
        source: input.to_string(),
        scope,
        map_urls: Vec::new(),
        crawl_options,
        output: output.clone(),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: execution_job_id,
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        auth_snapshot: auth_snapshot.cloned(),
        attempt: source_execution.attempt,
        embed,
        fetch_provider: runtime.fetch_provider.clone(),
        render_provider: runtime.render_provider.clone(),
        artifact_store: runtime.artifact_store.clone(),
        document_cache: runtime.document_cache.clone(),
        event_store: Some(runtime.jobs.clone()),
    };
    let output = if let Some(job_id) = source_execution.existing_job_id {
        let execution = WebSourceJobExecution {
            job_id,
            owns_status: false,
        };
        index_web_source_with_execution(
            index_input,
            execution,
            runtime.jobs.as_ref(),
            runtime.ledger.as_ref(),
            runtime.embedding_provider.as_ref(),
            runtime.vector_store.as_ref(),
        )
        .await
    } else {
        let descriptor = runtime
            .jobs
            .create(web_source_job_create_request(
                &index_input,
                source_execution.priority,
                source_execution.idempotency_key.clone(),
                MetadataMap::new(),
            ))
            .await?;
        let execution = WebSourceJobExecution {
            job_id: descriptor.job_id,
            owns_status: true,
        };
        index_web_source_with_execution(
            index_input,
            execution,
            runtime.jobs.as_ref(),
            runtime.ledger.as_ref(),
            runtime.embedding_provider.as_ref(),
            runtime.vector_store.as_ref(),
        )
        .await
    }
    // Both branches above already return `anyhow::Result`, so `.context()`
    // applies directly and preserves the existing chain. The prior
    // `.map_err(|e| anyhow::anyhow!(e.to_string()))` round-tripped the error
    // through a bare string first, discarding whatever chain it already
    // carried before this frame was even added.
    .context("web source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_pages,
        graph_candidates: output.graph_candidates,
        warnings: output.warnings,
        artifacts: output.artifacts,
        inline: output.inline,
    })
}
