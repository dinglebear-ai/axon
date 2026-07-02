//! Web-URL branch of `axon source <input>`.
//!
//! Acquisition (synchronous crawl to completion — like `axon crawl --wait`)
//! followed by dispatch to the web bridge
//! ([`axon_services::index_web_source_with_job`]). The crawl writes a
//! `manifest.jsonl` + `markdown/` tree under the domain's sync output dir; the
//! web bridge reads those prepared paths to discover, diff, embed, and publish.
//! This is the canonical replacement for `axon crawl <url>`.

use axon_api::source::{JobId, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::crawl_sync::crawl_for_source;
use axon_services::{WebSourceIndexInput, WebSourceIndexOutput, index_web_source_with_job};
use std::error::Error;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Crawl `input` (a validated http/https URL) then index the crawl output
/// through the web bridge.
pub async fn run_web_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=web",
        cfg.collection
    ));

    // Acquire: run the crawl to completion. The web bridge (not the crawl's own
    // embed pass) owns vectorization, so `crawl_for_source` disables embedding.
    let crawl = crawl_for_source(cfg, input).await?;
    log_info(&format!(
        "command=source kind=web crawl_pages={} crawl_markdown={} output_dir={}",
        crawl.pages_seen,
        crawl.markdown_files,
        crawl.output_dir.display()
    ));

    let index_input = build_web_index_input(cfg, runtime, input, &crawl);
    let output = index_web_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_web_output(cfg, input, &output);
    Ok(())
}

fn build_web_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    crawl: &axon_services::crawl_sync::CrawlForSourceResult,
) -> WebSourceIndexInput {
    WebSourceIndexInput {
        source: input.to_string(),
        // Site is the default web acquisition scope: a crawled subtree, not a
        // single page or a bare URL map.
        scope: SourceScope::Site,
        manifest_path: Some(crawl.manifest_path.clone()),
        markdown_root: Some(crawl.markdown_root.clone()),
        map_urls: Vec::new(),
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_web_source_with_job` creates the real job row and
        // overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
    }
}

fn render_web_output(cfg: &Config, input: &str, output: &WebSourceIndexOutput) {
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
                "removed_pages": output.removed_pages,
                "target": input,
                "collection": cfg.collection,
                "kind": "web",
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
            output.removed_pages,
        ))
    );
}
