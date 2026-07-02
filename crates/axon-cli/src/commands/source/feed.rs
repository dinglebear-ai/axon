//! Feed-URL branch of `axon source <input>`.
//!
//! Acquisition (network fetch of the raw feed document to a stable cache path)
//! + dispatch to the feed bridge
//! ([`axon_services::index_feed_source_with_job`]). The original feed URL (with
//! any `rss:`/`feed:`/`atom:` prefix stripped) is the fetch target; the fetched
//! file is the prepared `feed_path` the adapter parses via `feed-rs`. The path
//! is a deterministic function of the feed URL so re-indexing the same feed
//! keeps a stable source id (the bridge hashes the path).

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    FeedSourceIndexInput, FeedSourceIndexOutput, fetch_feed_to_file, index_feed_source_with_job,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Fetch `input` (a feed URL or `rss:`/`feed:`/`atom:` prefixed target) and
/// index it through the feed bridge.
pub async fn run_feed_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=feed",
        cfg.collection
    ));

    // Fetch to a deterministic, feed-URL-derived cache path (stable source id).
    let feed_path = fetch_feed_to_file(input).await?;

    let index_input = build_feed_index_input(cfg, runtime, feed_path);
    let output = index_feed_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_feed_output(cfg, input, &output);
    Ok(())
}

fn build_feed_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    feed_path: PathBuf,
) -> FeedSourceIndexInput {
    FeedSourceIndexInput {
        feed_path,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_feed_source_with_job` creates the real job row
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

fn render_feed_output(cfg: &Config, input: &str, output: &FeedSourceIndexOutput) {
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
                "removed_entries": output.removed_entries,
                "target": input,
                "collection": cfg.collection,
                "kind": "feed",
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
            output.removed_entries,
        ))
    );
}
