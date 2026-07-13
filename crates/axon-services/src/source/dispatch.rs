//! Per-family acquisition + bridge dispatch for `index_source`.
//!
//! Each function owns one acquisition class: it runs the family's existing
//! acquire helper, builds the family's existing `*SourceIndexInput`, calls the
//! family's existing `index_*_source_with_job` bridge, and normalizes the output
//! into [`IndexCounts`] for [`super::result_map::to_source_result`]. The
//! acquire helpers and bridges are unchanged — this is the relocation of the
//! per-family orchestration that previously lived in the CLI.

mod index_inputs;
mod web_options;

use anyhow::Context as _;
use axon_api::source::{AuthScope, AuthSnapshot, JobId, MetadataMap, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_info;
use uuid::Uuid;
use web_options::web_crawl_options;

use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use crate::source::SourceExecutionContext;
use crate::web_source::{
    WebSourceJobExecution, index_web_source_with_execution, web_source_job_create_request,
};
use crate::{
    GitSourceIndexInput, LocalSourceIndexInput, LocalSourceSelectionPolicy, SessionSelector,
    WebSourceIndexInput, clone_git_repo, fetch_feed_to_file, fetch_reddit_dump,
    fetch_registry_dump, fetch_youtube_dump, index_feed_source_with_job, index_git_source_with_job,
    index_local_source_with_job, index_reddit_source_with_job, index_registry_source_with_job,
    index_sessions_source_with_job, index_youtube_source_with_job, parse_registry_target,
    parse_session_selector,
};

/// Placeholder job id — every `index_*_source_with_job` bridge creates the real
/// job row and overwrites this with the descriptor's job id.
fn placeholder_job_id() -> JobId {
    JobId::new(Uuid::nil())
}

/// Local-path source: dispatch straight to the local bridge (no acquisition).
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_local(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=local embed={embed}"
    ));
    let has_local_scope = auth_snapshot
        .map(|snapshot| super::authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    super::enforce_local_source_policy(input, has_local_scope)?;
    let index_input = LocalSourceIndexInput {
        root: std::path::PathBuf::from(input),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        route: Some(route.clone()),
    };
    let output = index_local_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("local source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_files,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Git-repository source: shallow-clone (acquisition) then dispatch to the git
/// bridge. The `TempDir` is kept alive for the whole indexing pass.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_git(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=git embed={embed}"
    ));
    let checkout = clone_git_repo(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("git clone failed")?;
    let index_input = GitSourceIndexInput {
        target_url: input.to_string(),
        repo_root: checkout.path().to_path_buf(),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        route: Some(route.clone()),
        enricher: runtime.enricher.clone(),
    };
    let output = index_git_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("git source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_files,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Feed source: fetch the raw feed to a deterministic cache path then dispatch
/// to the feed bridge.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_feed(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=feed embed={embed} max_items={max_items:?}"
    ));
    let feed_path = fetch_feed_to_file(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("feed fetch failed")?;
    let index_input = index_inputs::feed_index_input(
        runtime,
        feed_path,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
    );
    let output = index_feed_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("feed source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_entries,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Reddit source: OAuth + fetch to a prepared dump then dispatch to the reddit
/// bridge. Missing credentials fail in `fetch_reddit_dump`, before any request.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_reddit(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=reddit embed={embed} max_items={max_items:?}"
    ));
    let dump_path = fetch_reddit_dump(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("reddit fetch failed")?;
    let index_input = index_inputs::reddit_index_input(
        runtime,
        input,
        dump_path,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
    );
    let output = index_reddit_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("reddit source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_items,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// YouTube source: yt-dlp fetch to a prepared dump then dispatch to the youtube
/// bridge. A missing yt-dlp binary fails in `fetch_youtube_dump`.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_youtube(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=youtube embed={embed} max_items={max_items:?}"
    ));
    let dump_path = fetch_youtube_dump(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("youtube fetch failed")?;
    let index_input = index_inputs::youtube_index_input(
        runtime,
        input,
        dump_path,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
    );
    let output = index_youtube_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("youtube source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_videos,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Registry source: parse the `pkg:` selector, fetch package metadata to a
/// prepared dump, then dispatch to the registry bridge.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_registry(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=registry embed={embed} max_items={max_items:?}"
    ));
    let (registry, package) =
        parse_registry_target(input).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let dump_path = fetch_registry_dump(&registry, &package)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("registry fetch failed")?;
    let index_input = index_inputs::registry_index_input(
        runtime,
        dump_path,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
    );
    let output = index_registry_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("registry source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_versions,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Session source: parse the `session:` selector (no network acquisition) then
/// dispatch to the sessions bridge.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_session(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=session embed={embed} max_items={max_items:?}"
    ));
    let SessionSelector {
        sessions_root,
        provider,
        session_id,
    } = parse_session_selector(input).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let index_input = index_inputs::session_index_input(
        runtime,
        sessions_root,
        provider,
        session_id,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
    );
    let output = index_sessions_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("session source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_files,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
    })
}

/// Web source: drive the `WebSourceAdapter`'s discover→acquire→normalize
/// pipeline directly (issue #298 Wave 1b) — no `crawl_for_source`/
/// `crawl_for_source_page` acquisition pre-pass and no
/// `manifest.jsonl`/`markdown_root` disk handoff. `dispatch_web` translates
/// the ambient CLI-resolved `cfg: &Config` into the web adapter's
/// `validated_options` shape so existing `--render-mode`/`--max-depth`/
/// `--url-whitelist`/etc. flags keep working; `max_pages` (already this
/// function's own parameter) overrides `cfg.max_pages` when set, matching the
/// pre-Wave-1b behavior.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_web(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_pages: Option<u64>,
    source_execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=web scope={scope:?} embed={embed} max_pages={max_pages:?}"
    ));
    let index_input = WebSourceIndexInput {
        source: input.to_string(),
        scope,
        map_urls: Vec::new(),
        crawl_options: web_crawl_options(cfg, max_pages),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        fetch_provider: runtime.fetch_provider.clone(),
        render_provider: runtime.render_provider.clone(),
    };
    let output = if let Some(job_id) = source_execution.existing_job_id.clone() {
        let execution = WebSourceJobExecution {
            job_id,
            owns_status: false,
        };
        index_web_source_with_execution(
            index_input,
            execution,
            runtime.jobs.as_ref(),
            runtime.ledger.as_ref(),
            runtime.embedding_provider.as_ref(),
            runtime.vector_store.as_ref(),
        )
        .await
    } else {
        let descriptor = runtime
            .jobs
            .create(web_source_job_create_request(
                &index_input,
                source_execution.priority,
                source_execution.idempotency_key.clone(),
                MetadataMap::new(),
            ))
            .await?;
        let execution = WebSourceJobExecution {
            job_id: descriptor.job_id,
            owns_status: true,
        };
        index_web_source_with_execution(
            index_input,
            execution,
            runtime.jobs.as_ref(),
            runtime.ledger.as_ref(),
            runtime.embedding_provider.as_ref(),
            runtime.vector_store.as_ref(),
        )
        .await
    }
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("web source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_pages,
        graph_candidates: output.graph_candidates,
        warnings: output.warnings,
    })
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
