//! `axon <source>` / `axon source <input>` — index a source through the
//! unified pipeline.
//!
//! This is now a **thin CLI shim**: it builds an [`axon_api::source::SourceRequest`]
//! from the resolved positional input + `--collection`, calls the
//! transport-neutral orchestrator [`axon_services::index_source`], and renders
//! the returned [`axon_api::source::SourceResult`]. All classification,
//! acquisition, and per-family bridge dispatch live in `axon-services` so CLI,
//! MCP, and REST share one entrypoint.

use axon_api::source::{
    ArtifactKind, ArtifactMode, ContentRef, LifecycleStatus, ResponseMode, Severity, SourceIntent,
    SourceLimits, SourceRequest, SourceResult, SourceScope, SourceWarning,
};
use axon_core::config::{CommandKind, Config};
use axon_core::ui::{accent, muted, primary};
use axon_services::context::ServiceContext;
use axon_services::index_source;
use futures_util::stream::{self, StreamExt};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::error::Error;

pub(crate) mod detach;

pub async fn run_source(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let inputs = resolve_source_inputs(cfg)?;
    if inputs.len() == 1 {
        let request = build_source_request(cfg, inputs.into_iter().next().expect("one input"))?;
        return run_source_request(cfg, service_context, request).await;
    }
    if cfg.command != CommandKind::Source {
        return Err("--urls and --url-glob accept multiple inputs only with `axon source`".into());
    }
    if cfg.output_path.is_some() {
        return Err("--output accepts only one source input".into());
    }

    let concurrency = cfg.batch_concurrency.clamp(1, 512);
    let outcomes = stream::iter(inputs.into_iter().enumerate())
        .map(|(index, input)| async move {
            let reported_input = input.clone();
            let result = match build_source_request(cfg, input) {
                Ok(request) => execute_source_request(cfg, service_context, request).await,
                Err(error) => Err(error),
            };
            result
                .map(|result| (index, result))
                .map_err(|error| (index, reported_input, error.to_string()))
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<Result<(usize, SourceResult), (usize, String, String)>>>()
        .await;
    if should_detach(cfg)
        && outcomes.iter().any(|outcome| {
            outcome
                .as_ref()
                .is_ok_and(|(_, result)| result.job.is_some())
        })
    {
        // A sibling input can fail after other jobs were durably enqueued.
        // Start the worker before propagating that error so successful jobs do
        // not remain queued indefinitely.
        detach::ensure_worker_process(cfg).await;
    }
    let mut indexed_results = Vec::new();
    let mut batch_errors = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(result) => indexed_results.push(result),
            Err(error) => batch_errors.push(error),
        }
    }
    indexed_results.sort_by_key(|(index, _)| *index);
    batch_errors.sort_by_key(|(index, _, _)| *index);
    let semantic_failures = indexed_results
        .iter()
        .filter(|(_, result)| result.status == LifecycleStatus::Failed)
        .count();
    let failed = batch_errors.len() + semantic_failures;
    let succeeded = indexed_results.len() - semantic_failures;

    if cfg.json_output {
        let mut rendered = indexed_results
            .iter()
            .map(|(index, result)| {
                if is_queued_descriptor(result) {
                    (*index, queued_descriptor_json(result))
                } else {
                    (*index, source_result_json(cfg, result))
                }
            })
            .collect::<Vec<_>>();
        rendered.extend(batch_errors.iter().map(|(index, input, error)| {
            (
                *index,
                serde_json::json!({
                    "input": input,
                    "status": "failed",
                    "error": error,
                }),
            )
        }));
        rendered.sort_by_key(|(index, _)| *index);
        let rendered = rendered
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "count": rendered.len(),
                "succeeded": succeeded,
                "failed": failed,
                "results": rendered,
            })
        );
    } else {
        for (_, result) in &indexed_results {
            render_source_result(cfg, result);
        }
        for (_, input, error) in &batch_errors {
            eprintln!("{} {}: {}", muted("failed"), input, error);
        }
    }
    if failed > 0 {
        return Err(format!(
            "{} of {} source inputs failed",
            failed,
            indexed_results.len() + batch_errors.len()
        )
        .into());
    }
    Ok(())
}

/// Per the command contract, `axon <source>` is detached by default: it
/// enqueues a durable job and returns a job descriptor. `--wait true` opts
/// into blocking foreground execution. Retained `scrape` stays foreground —
/// its whole purpose is returning the one page inline.
pub(crate) fn should_detach(cfg: &Config) -> bool {
    cfg.command == CommandKind::Source && !cfg.wait
}

pub(crate) async fn run_source_request(
    cfg: &Config,
    service_context: &ServiceContext,
    request: SourceRequest,
) -> Result<(), Box<dyn Error>> {
    let result = execute_source_request(cfg, service_context, request).await?;
    render_source_result(cfg, &result);
    if should_detach(cfg) && result.job.is_some() {
        detach::ensure_worker_process(cfg).await;
    }
    write_scrape_output_if_requested(cfg, service_context, &result).await?;
    if result.status == LifecycleStatus::Failed {
        let msg = result
            .warnings
            .first()
            .map(|w| w.message.clone())
            .unwrap_or_else(|| "source indexing failed".to_string());
        return Err(msg.into());
    }
    Ok(())
}

async fn execute_source_request(
    cfg: &Config,
    service_context: &ServiceContext,
    request: SourceRequest,
) -> Result<SourceResult, Box<dyn Error>> {
    let detached = should_detach(cfg);
    Ok(if detached {
        detach::enqueue_source_detached(service_context, request).await?
    } else {
        index_source(request, service_context)
            .await
            // Preserve the whole error chain: `anyhow::Error` erases to a
            // `Box<dyn Error>` whose `.source()` walk `main` renders, so the
            // actionable cause (e.g. the git adapter's clone failure) survives
            // instead of being flattened to only the outermost context.
            .map_err(|e| -> Box<dyn Error> { e.into() })?
    })
}

/// Parse a `--scope` string (e.g. `page`, `site`) into a [`SourceScope`].
///
/// `SourceScope` is `snake_case` in serde, so the raw flag value is
/// deserialized directly; an unknown scope returns a clear error listing the
/// offending value.
fn parse_scope(scope: &str) -> Result<SourceScope, Box<dyn Error>> {
    serde_json::from_value::<SourceScope>(serde_json::Value::String(scope.to_string()))
        .map_err(|_| format!("unknown --scope value: {scope}").into())
}

pub(crate) fn build_source_request(
    cfg: &Config,
    input: String,
) -> Result<SourceRequest, Box<dyn Error>> {
    let mut request = SourceRequest::new(input);
    request.collection = Some(cfg.collection.clone());
    request.embed = cfg.embed;
    if cfg.scrape_inline {
        request.output.response_mode = ResponseMode::Inline;
    }
    if cfg.command == CommandKind::Scrape {
        request.intent = SourceIntent::Acquire;
        request.scope = Some(SourceScope::Page);
        request.limits = SourceLimits {
            max_items: Some(1),
            max_pages: Some(1),
            max_depth: Some(0),
            ..SourceLimits::default()
        };
        if cfg.output_path.is_some() {
            request.output.artifact_mode = ArtifactMode::Always;
        }
    } else if let Some(scope) = cfg.source_scope.as_deref() {
        request.scope = Some(parse_scope(scope)?);
    }
    Ok(request)
}

/// Resolve positional, `--urls`, or `--url-glob` source inputs.
fn resolve_source_inputs(cfg: &Config) -> Result<Vec<String>, Box<dyn Error>> {
    let inputs = if cfg.urls_csv.is_some() || !cfg.url_glob.is_empty() {
        super::common_urls::parse_urls(cfg)
    } else {
        cfg.positional
            .iter()
            .filter(|input| !input.trim().is_empty())
            .cloned()
            .collect()
    };
    if inputs.is_empty() {
        Err({
            "axon source requires a local path, git repository URL, feed URL, youtube target, \
             reddit target, web URL, session selector, registry target, --urls, or --url-glob"
                .into()
        })
    } else {
        Ok(inputs)
    }
}

pub(crate) fn source_result_json(cfg: &Config, result: &SourceResult) -> serde_json::Value {
    serde_json::json!({
        "job_id": result.job_id.0.to_string(),
        "source_id": result.source_id.0,
        "canonical_uri": result.canonical_uri,
        "source_kind": result.source_kind,
        "adapter": result.adapter,
        "scope": result.scope,
        "status": result.status,
        "generation": result.ledger.generation.0,
        "documents_prepared": result.counts.documents_total,
        "chunks_prepared": result.counts.chunks_total,
        "vector_points_written": result.counts.vector_points_total,
        "collection": cfg.collection,
        "graph": result.graph,
        "warnings": result.warnings,
        "inline": &result.inline,
        "job": &result.job,
    })
}

/// True when the result represents a still-running detached job (a job
/// descriptor is present and the status is non-terminal). Such a result carries
/// no real counts yet, so it must render as a queued descriptor rather than the
/// terminal "Source Indexed" summary (`axon_rust-x4gxr.7` / `.10`).
fn is_queued_descriptor(result: &SourceResult) -> bool {
    result.job.is_some()
        && matches!(
            result.status,
            LifecycleStatus::Queued
                | LifecycleStatus::Pending
                | LifecycleStatus::Running
                | LifecycleStatus::Waiting
                | LifecycleStatus::Blocked
                | LifecycleStatus::Canceling
        )
}

/// Lean job-descriptor JSON for a detached, not-yet-run source job — the
/// contract's queued-descriptor shape, not the zero-filled full `SourceResult`.
/// Poll/stream hints are CLI commands so `--json` callers get actionable
/// next-steps too (`axon_rust-x4gxr.10`).
fn queued_descriptor_json(result: &SourceResult) -> serde_json::Value {
    let job_id = result.job_id.0.to_string();
    serde_json::json!({
        "job_id": job_id,
        "kind": "source",
        "status": result.status,
        "canonical_uri": result.canonical_uri,
        "poll": { "command": format!("axon jobs get {job_id}") },
        "events": { "command": format!("axon jobs events {job_id}") },
        "warnings": result.warnings,
    })
}

pub(crate) fn render_source_result(cfg: &Config, result: &SourceResult) {
    if cfg.json_output {
        if is_queued_descriptor(result) {
            println!("{}", queued_descriptor_json(result));
        } else {
            println!("{}", source_result_json(cfg, result));
        }
        return;
    }

    if cfg.scrape_inline && render_inline_source_content(result) {
        return;
    }

    if render_queued_source_descriptor(result) {
        return;
    }

    if render_failed_source(result) {
        return;
    }

    println!(
        "  {} {}",
        primary("Source Indexed"),
        accent(&result.source_id.0)
    );
    print_input_line(result);
    println!(
        "  {}",
        muted(&format!("Generation: {}", result.ledger.generation.0))
    );
    println!(
        "  {}",
        muted(&format!(
            "Documents: {}  Chunks: {}  Vector points: {}",
            result.counts.documents_total,
            result.counts.chunks_total,
            result.counts.vector_points_total,
        ))
    );
    println!(
        "  {}",
        muted(&format!(
            "Graph: {} nodes  {} edges  {} evidence",
            result.graph.nodes_upserted, result.graph.edges_upserted, result.graph.evidence_records,
        ))
    );
    print_warnings(result);
}

fn print_input_line(result: &SourceResult) {
    println!("  {}", muted(&format!("Input: {}", result.canonical_uri)));
}

fn print_warnings(result: &SourceResult) {
    for (label, message, count) in grouped_warnings(&result.warnings) {
        let line = if count > 1 {
            format!("{label} {message} (x{count})")
        } else {
            format!("{label} {message}")
        };
        println!("  {}", muted(&sanitize_terminal_text(&line)));
    }
}

/// Collapse identical `(severity label, message)` pairs into one entry with a
/// count, preserving first-seen order. Per-document warnings repeat once per
/// document, so a site-scope run can otherwise print hundreds of identical
/// lines; JSON output keeps the full per-item list. The `HashMap` index keeps
/// grouping linear even when a large crawl emits mostly-unique messages.
fn grouped_warnings(warnings: &[SourceWarning]) -> Vec<(&'static str, &str, usize)> {
    let mut grouped: Vec<(&'static str, &str, usize)> = Vec::new();
    let mut index: HashMap<(&'static str, &str), usize> = HashMap::new();
    for warning in warnings {
        let label = severity_label(warning);
        match index.entry((label, warning.message.as_str())) {
            Entry::Occupied(slot) => grouped[*slot.get()].2 += 1,
            Entry::Vacant(slot) => {
                slot.insert(grouped.len());
                grouped.push((label, warning.message.as_str(), 1));
            }
        }
    }
    grouped
}

/// Informational diagnostics (e.g. `parse.parser_hint_unregistered`) must not
/// masquerade as degradations in human output.
fn severity_label(warning: &SourceWarning) -> &'static str {
    if matches!(warning.severity, Severity::Debug | Severity::Info) {
        "Note:"
    } else {
        "Warning:"
    }
}

/// Warning text can embed upstream-derived strings; strip control characters
/// (including ANSI escape introducers) before the text joins an ANSI-styled
/// terminal line.
fn sanitize_terminal_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Render the job-descriptor shape for a detached (still non-terminal) source
/// result. Returns false for terminal results so the full indexed / failed
/// rendering runs instead.
fn render_queued_source_descriptor(result: &SourceResult) -> bool {
    if !is_queued_descriptor(result) {
        return false;
    }

    let job_id = result.job_id.0.to_string();
    println!("  {} {}", primary("Source Queued"), accent(&job_id));
    print_input_line(result);
    println!(
        "  {}",
        muted(&format!(
            "Poll: axon jobs get {job_id}  ·  Stream: axon jobs events {job_id}"
        ))
    );
    println!("  {}", muted("Foreground instead: re-run with --wait true"));
    print_warnings(result);
    true
}

/// Render a failed source result instead of the misleading zero-count "Source
/// Indexed" banner. `run_source_request` still returns a non-zero exit, so this
/// is the human context line for that failure (`axon_rust-x4gxr.7`).
fn render_failed_source(result: &SourceResult) -> bool {
    if result.status != LifecycleStatus::Failed {
        return false;
    }
    println!(
        "  {} {}",
        primary("Source Failed"),
        accent(&result.canonical_uri)
    );
    print_warnings(result);
    true
}
fn render_inline_source_content(result: &SourceResult) -> bool {
    let Some(inline) = &result.inline else {
        return false;
    };
    match inline.content.as_ref() {
        Some(ContentRef::InlineText { text }) => {
            println!("{text}");
            true
        }
        Some(ContentRef::InlineBytes { bytes_base64, .. }) => {
            println!("{bytes_base64}");
            true
        }
        _ => false,
    }
}

async fn write_scrape_output_if_requested(
    cfg: &Config,
    service_context: &ServiceContext,
    result: &SourceResult,
) -> Result<(), Box<dyn Error>> {
    if cfg.command != CommandKind::Scrape {
        return Ok(());
    }
    let Some(path) = cfg.output_path.as_ref() else {
        return Ok(());
    };

    if let Some(ContentRef::InlineText { text }) = result
        .inline
        .as_ref()
        .and_then(|inline| inline.content.as_ref())
    {
        axon_core::artifacts::atomic_write_explicit(path, text.as_bytes())
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
        return Ok(());
    }

    let artifact = result
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_kind == ArtifactKind::NormalizedContent)
        .ok_or_else(|| "scrape completed without cleaned content to write".to_string())?;
    let content =
        axon_services::artifacts::artifact_content(service_context, artifact.artifact_id.clone())
            .await
            .map_err(|err| -> Box<dyn Error> {
                format!(
                    "failed to read scrape cleaned content artifact {}: {err}",
                    artifact.artifact_id.0
                )
                .into()
            })?;
    let bytes = tokio::fs::read(&content.path)
        .await
        .map_err(|err| -> Box<dyn Error> {
            format!(
                "failed to read scrape cleaned content artifact {}: {err}",
                artifact.artifact_id.0
            )
            .into()
        })?;
    axon_core::artifacts::atomic_write_explicit(path, &bytes)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    Ok(())
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
