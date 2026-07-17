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
    ArtifactKind, ArtifactMode, ContentRef, LifecycleStatus, ResponseMode, SourceIntent,
    SourceLimits, SourceRequest, SourceResult, SourceScope,
};
use axon_core::config::{CommandKind, Config};
use axon_core::ui::{accent, muted, primary};
use axon_services::context::ServiceContext;
use axon_services::index_source;
use std::error::Error;

pub async fn run_source(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let input = resolve_source_input(cfg)?;

    let request = build_source_request(cfg, input)?;

    run_source_request(cfg, service_context, request).await
}

pub(crate) async fn run_source_request(
    cfg: &Config,
    service_context: &ServiceContext,
    request: SourceRequest,
) -> Result<(), Box<dyn Error>> {
    let result = index_source(request, service_context)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;

    render_source_result(cfg, &result);
    write_scrape_output_if_requested(cfg, service_context, &result).await?;

    // A degraded/failed result (unsupported input, no data plane, …) carries a
    // warning but no error — surface it as a nonzero exit for CLI callers.
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

/// Read the positional argument as the source input to index.
fn resolve_source_input(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .first()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "axon source requires a local path, git repository URL, feed URL, youtube target, \
             reddit target, web URL, session selector, or registry target argument"
                .into()
        })
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

pub(crate) fn render_source_result(cfg: &Config, result: &SourceResult) {
    if cfg.json_output {
        println!("{}", source_result_json(cfg, result));
        return;
    }

    if cfg.scrape_inline && render_inline_source_content(result) {
        return;
    }

    println!(
        "  {} {}",
        primary("Source Indexed"),
        accent(&result.source_id.0)
    );
    println!("  {}", muted(&format!("Input: {}", result.canonical_uri)));
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
    for warning in &result.warnings {
        println!("  {}", muted(&format!("Warning: {}", warning.message)));
    }
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
