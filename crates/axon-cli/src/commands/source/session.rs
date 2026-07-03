//! Session branch of `axon source <input>`.
//!
//! Unlike reddit/youtube/registry, a session source has **no network
//! acquisition** — the `session:<provider>:<path>` selector already points at
//! an on-disk session export. So this branch parses the selector into
//! `(sessions_root, provider, session_id)`
//! ([`axon_services::parse_session_selector`]) and dispatches straight to the
//! sessions bridge ([`axon_services::index_sessions_source_with_job`]), which
//! reads the export file(s) under `sessions_root` (claude/codex = `.jsonl`,
//! gemini = a single `.json`).
//!
//! This is the single-session slice. Indexing a whole directory of sessions
//! (bulk) is a P10 follow-up.

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::TargetLocalSourceRuntime;
use axon_services::{
    SessionSelector, SessionsSourceIndexInput, SessionsSourceIndexOutput,
    index_sessions_source_with_job, parse_session_selector,
};
use std::error::Error;
use uuid::Uuid;

use super::CLI_OWNER_ID;

/// Resolve `input` (a `session:<provider>:<path>` selector) to an on-disk
/// session export and index it through the sessions bridge.
pub async fn run_session_source(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=source collection={} kind=session",
        cfg.collection
    ));

    // Resolve: pure selector parse to (sessions_root, provider, session_id). An
    // invalid selector fails here, before any indexing work.
    let selector = parse_session_selector(input)?;

    let index_input = build_session_index_input(cfg, runtime, selector);
    let output = index_sessions_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_session_output(cfg, input, &output);
    Ok(())
}

fn build_session_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    selector: SessionSelector,
) -> SessionsSourceIndexInput {
    SessionsSourceIndexInput {
        sessions_root: selector.sessions_root,
        provider: selector.provider,
        session_id: selector.session_id,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_sessions_source_with_job` creates the real job
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

fn render_session_output(cfg: &Config, input: &str, output: &SessionsSourceIndexOutput) {
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
                "kind": "session",
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
