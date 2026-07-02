//! `axon source <input>` — index a source through the unified pipeline.
//!
//! This is the first user-facing surface of the new source pipeline. It handles
//! two acquisition classes:
//!
//! * **Local paths** — dispatched to the target local-source runtime via
//!   [`axon_services::index_local_source_with_job`].
//! * **Git repository URLs** — shallow-cloned (acquisition) then dispatched to
//!   the git bridge via [`axon_services::index_git_source_with_job`].
//!
//! Everything else (web/feed/reddit/youtube acquisition) returns a clear "not
//! yet wired" error — a later P10 slice.

mod git;

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

/// Stable owner id used to lease sources indexed from the CLI.
pub(crate) const CLI_OWNER_ID: &str = "cli";

/// Acquisition class the input routes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceInputKind {
    /// An existing path on the local filesystem.
    Local,
    /// A parseable git repository URL (github/gitlab/gitea/`.git`/`git+https`).
    Git,
    /// Neither — unsupported for this slice.
    Unsupported,
}

pub async fn run_source(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let input = resolve_source_input(cfg)?;

    match classify_source_input(&input).await {
        SourceInputKind::Local => run_local_source(cfg, service_context, &input).await,
        SourceInputKind::Git => {
            let runtime = require_data_plane(service_context)?;
            git::run_git_source(cfg, runtime, &input).await
        }
        SourceInputKind::Unsupported => Err(unsupported_input_error(&input)),
    }
}

/// Classify the input into an acquisition class.
///
/// Local existence wins first (a directory literally named like a URL is still
/// treated as local), then git-URL parsing, then unsupported. Split out as a
/// pure-ish async fn (only fs metadata + string parsing) so routing is testable
/// without a data plane.
async fn classify_source_input(input: &str) -> SourceInputKind {
    if input_is_local_path(input).await {
        return SourceInputKind::Local;
    }
    if input_is_git_target(input) {
        return SourceInputKind::Git;
    }
    SourceInputKind::Unsupported
}

/// True when `input` resolves to an existing path on disk.
async fn input_is_local_path(input: &str) -> bool {
    tokio::fs::metadata(PathBuf::from(input)).await.is_ok()
}

/// True when `input` parses as a git repository target.
fn input_is_git_target(input: &str) -> bool {
    axon_services::is_git_target(input)
}

/// Read the positional argument, mirroring how `run_embed` resolves input.
fn resolve_source_input(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .first()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "axon source requires a local path or git repository URL argument".into())
}

/// Require the target source runtime (the data plane), or return the shared
/// "data plane required" guard error.
fn require_data_plane(
    service_context: &ServiceContext,
) -> Result<&TargetLocalSourceRuntime, Box<dyn Error>> {
    service_context
        .target_local_source_runtime()
        .ok_or_else(|| -> Box<dyn Error> {
            "source indexing requires a running data plane (set qdrant_url + tei_url; \
             available under serve/mcp/--wait)"
                .into()
        })
}

/// Clear error for inputs that are neither a local path nor a git URL.
fn unsupported_input_error(input: &str) -> Box<dyn Error> {
    format!(
        "axon source supports local paths and git repository URLs; {input} is neither \
         (web/feed/reddit/youtube acquisition is a P10 follow-up)"
    )
    .into()
}

async fn run_local_source(
    cfg: &Config,
    service_context: &ServiceContext,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    let runtime = require_data_plane(service_context)?;
    let root = PathBuf::from(input);
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

    render_source_output(cfg, input, &output);
    Ok(())
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
