//! YouTube branch of `axon source <input>`.
//!
//! Acquisition (yt-dlp fetch of the video/playlist/channel's metadata +
//! English subtitles into a prepared JSON dump on a stable, target-derived
//! cache path) + dispatch to the youtube bridge
//! ([`axon_services::index_youtube_source_with_job`]). The youtube bridge
//! derives the source id from the *target*'s canonical URI (not the dump path);
//! the dump file is the `youtube_dump_path` option the adapter reads.
//!
//! A missing `yt-dlp` binary is reported by
//! [`axon_services::fetch_youtube_dump`] before any dump is written, so the
//! caller gets a clear, actionable error rather than an empty result.

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    YoutubeSourceIndexInput, YoutubeSourceIndexOutput, fetch_youtube_dump,
    index_youtube_source_with_job,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Fetch `input` (a YouTube video/playlist/channel URL, `@handle`, or bare
/// 11-char video id) into a prepared dump and index it through the youtube
/// bridge.
pub async fn run_youtube_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=youtube",
        cfg.collection
    ));

    // Acquire: yt-dlp fetch to a deterministic, target-derived cache path.
    // A missing yt-dlp binary (or an invalid target) fails here, before any
    // indexing work.
    let dump_path = fetch_youtube_dump(input).await?;

    let index_input = build_youtube_index_input(cfg, runtime, input, dump_path);
    let output = index_youtube_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_youtube_output(cfg, input, &output);
    Ok(())
}

fn build_youtube_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    dump_path: PathBuf,
) -> YoutubeSourceIndexInput {
    YoutubeSourceIndexInput {
        target: input.to_string(),
        youtube_dump_path: dump_path,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_youtube_source_with_job` creates the real job row
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

fn render_youtube_output(cfg: &Config, input: &str, output: &YoutubeSourceIndexOutput) {
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
                "removed_videos": output.removed_videos,
                "target": input,
                "collection": cfg.collection,
                "kind": "youtube",
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
            output.removed_videos,
        ))
    );
}
