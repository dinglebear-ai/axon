//! Registry branch of `axon source <input>`.
//!
//! Acquisition (fetch of the package's metadata from npm/PyPI/crates.io into a
//! prepared JSON dump on a stable, target-derived cache path) + dispatch to the
//! registry bridge ([`axon_services::index_registry_source_with_job`]). The
//! registry bridge derives the source id from the *dump path* (stable per
//! `registry/package`); the dump file is the `registry_dump_path` option the
//! adapter reads.
//!
//! An unknown registry or an unreachable package is reported by
//! [`axon_services::fetch_registry_dump`] before any indexing work, so the
//! caller gets a clear, actionable error rather than an empty result.

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    RegistrySourceIndexInput, RegistrySourceIndexOutput, fetch_registry_dump,
    index_registry_source_with_job, parse_registry_target,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Fetch `input` (a `pkg:<registry>/<package>` target) into a prepared dump and
/// index it through the registry bridge.
pub async fn run_registry_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=registry",
        cfg.collection
    ));

    // Parse the selector, then acquire the package metadata into a deterministic,
    // target-derived cache path. An unknown registry / unreachable package fails
    // here, before any indexing work.
    let (registry, package) = parse_registry_target(input)?;
    let dump_path = fetch_registry_dump(&registry, &package).await?;

    let index_input = build_registry_index_input(cfg, runtime, dump_path);
    let output = index_registry_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_registry_output(cfg, input, &output);
    Ok(())
}

fn build_registry_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    dump_path: PathBuf,
) -> RegistrySourceIndexInput {
    RegistrySourceIndexInput {
        registry_dump_path: dump_path,
        include_all_versions: false,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_registry_source_with_job` creates the real job
        // row and overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    }
}

fn render_registry_output(cfg: &Config, input: &str, output: &RegistrySourceIndexOutput) {
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
                "removed_versions": output.removed_versions,
                "target": input,
                "collection": cfg.collection,
                "kind": "registry",
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
            output.removed_versions,
        ))
    );
}
