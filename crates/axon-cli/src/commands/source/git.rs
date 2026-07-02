//! Git-repository branch of `axon source <input>`.
//!
//! Acquisition (shallow clone) + dispatch to the git bridge
//! ([`axon_services::index_git_source_with_job`]). The original git URL is the
//! identity (`target_url`); the cloned temp dir is the `repo_root`. The
//! [`tempfile::TempDir`] is kept alive until indexing finishes, then dropped to
//! clean up the checkout.

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    GitSourceIndexInput, GitSourceIndexOutput, clone_git_repo, index_git_source_with_job,
};
use std::error::Error;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Clone `input` (a validated git URL) and index it through the git bridge.
pub async fn run_git_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=git",
        cfg.collection
    ));

    // Keep the TempDir bound for the whole indexing pass; dropping it removes
    // the checkout.
    let checkout = clone_git_repo(input).await?;

    let index_input = build_git_index_input(cfg, runtime, input, checkout.path().to_path_buf());
    let output = index_git_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_git_output(cfg, input, &output);
    Ok(())
}

fn build_git_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    repo_root: std::path::PathBuf,
) -> GitSourceIndexInput {
    GitSourceIndexInput {
        target_url: input.to_string(),
        repo_root,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_git_source_with_job` creates the real job and
        // overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    }
}

fn render_git_output(cfg: &Config, input: &str, output: &GitSourceIndexOutput) {
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
                "kind": "git",
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
