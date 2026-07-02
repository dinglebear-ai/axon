//! `axon source <path>` — index a LOCAL source through the unified pipeline.
//!
//! This is the first user-facing surface of the new source pipeline. For this
//! slice it only handles LOCAL paths: it dispatches to the already-wired target
//! local-source runtime via [`axon_services::index_local_source_with_job`].
//! Non-local inputs (git/web/feed) require an acquisition step and return a
//! clear "not yet wired" error (a later P10 slice).

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::{ServiceContext, TargetLocalSourceRuntime};
use axon_services::{
    LocalSourceIndexInput, LocalSourceIndexOutput, LocalSourceSelectionPolicy,
    index_local_source_with_job,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

/// Stable owner id used to lease local sources indexed from the CLI.
const CLI_OWNER_ID: &str = "cli";

pub async fn run_source(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let input = resolve_source_input(cfg)?;
    let runtime =
        service_context
            .target_local_source_runtime()
            .ok_or_else(|| -> Box<dyn Error> {
                "source indexing requires a running data plane (set qdrant_url + tei_url; \
             available under serve/mcp/--wait)"
                    .into()
            })?;

    let root = resolve_local_root(&input).await?;
    log_info(&format!(
        "command=source collection={} kind=local",
        cfg.collection
    ));

    let index_input = build_index_input(cfg, runtime, root);
    let output = index_local_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_source_output(cfg, &input, &output);
    Ok(())
}

/// Read the positional path argument, mirroring how `run_embed` resolves input.
fn resolve_source_input(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .first()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "axon source requires a local path argument".into())
}

/// Resolve the input to an existing local path, or return a clear "local only"
/// error naming what is not yet wired.
async fn resolve_local_root(input: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = PathBuf::from(input);
    match tokio::fs::metadata(&path).await {
        Ok(_) => Ok(path),
        Err(_) => Err(format!(
            "axon source currently supports local paths only; {input} is not a local path \
             (git/web/feed acquisition is a P10 follow-up)"
        )
        .into()),
    }
}

fn build_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    root: PathBuf,
) -> LocalSourceIndexInput {
    LocalSourceIndexInput {
        root,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_local_source_with_job` creates the real job and
        // overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    }
}

fn render_source_output(cfg: &Config, input: &str, output: &LocalSourceIndexOutput) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "job_id": output.job_id.0.to_string(),
                "source_id": output.source_id.0,
                "generation": output.generation.0,
                "documents_prepared": output.documents_prepared,
                "chunks_prepared": output.chunks_prepared,
                "vector_points_written": output.vector_points_written,
                "removed_files": output.removed_files,
                "target": input,
                "collection": cfg.collection,
            })
        );
        return;
    }

    println!(
        "  {} {}",
        primary("Source Indexed"),
        accent(&output.source_id.0)
    );
    println!("  {}", muted(&format!("Input: {input}")));
    println!(
        "  {}",
        muted(&format!("Generation: {}", output.generation.0))
    );
    println!(
        "  {}",
        muted(&format!(
            "Documents: {}  Chunks: {}  Vector points: {}  Removed: {}",
            output.documents_prepared,
            output.chunks_prepared,
            output.vector_points_written,
            output.removed_files,
        ))
    );
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
