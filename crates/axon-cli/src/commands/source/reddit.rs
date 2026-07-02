//! Reddit branch of `axon source <input>`.
//!
//! Acquisition (Reddit OAuth + fetch of the subreddit listing or thread to a
//! prepared JSON dump on a stable, target-derived cache path) + dispatch to the
//! reddit bridge ([`axon_services::index_reddit_source_with_job`]). The reddit
//! bridge derives the source id from the *target* string (not the dump path);
//! the dump file is the `reddit_dump_path` option the adapter reads.
//!
//! Missing `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` is reported by
//! [`axon_services::fetch_reddit_dump`] before any network call, so the actual
//! Reddit fetch never runs without credentials.

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    RedditSourceIndexInput, RedditSourceIndexOutput, fetch_reddit_dump,
    index_reddit_source_with_job,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Fetch `input` (a `r/<name>` subreddit or reddit.com thread URL) into a
/// prepared dump and index it through the reddit bridge.
pub async fn run_reddit_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=reddit",
        cfg.collection
    ));

    // Acquire: OAuth + fetch to a deterministic, target-derived cache path.
    // Missing credentials fail here, before any Reddit request.
    let dump_path = fetch_reddit_dump(input).await?;

    let index_input = build_reddit_index_input(cfg, runtime, input, dump_path);
    let output = index_reddit_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_reddit_output(cfg, input, &output);
    Ok(())
}

fn build_reddit_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    dump_path: PathBuf,
) -> RedditSourceIndexInput {
    RedditSourceIndexInput {
        target: input.to_string(),
        dump_path,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_reddit_source_with_job` creates the real job row
        // and overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    }
}

fn render_reddit_output(cfg: &Config, input: &str, output: &RedditSourceIndexOutput) {
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
                "removed_items": output.removed_items,
                "target": input,
                "collection": cfg.collection,
                "kind": "reddit",
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
            output.removed_items,
        ))
    );
}
