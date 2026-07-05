//! Per-family acquisition + bridge dispatch for `index_source`.
//!
//! Each function owns one acquisition class: it runs the family's existing
//! acquire helper, builds the family's existing `*SourceIndexInput`, calls the
//! family's existing `index_*_source_with_job` bridge, and normalizes the output
//! into [`IndexCounts`] for [`super::result_map::to_source_result`]. The
//! acquire helpers and bridges are unchanged — this is the relocation of the
//! per-family orchestration that previously lived in the CLI.

use anyhow::Context as _;
use axon_api::source::{JobId, SourceScope};
use axon_core::config::Config;
use axon_core::logging::log_info;
use uuid::Uuid;

use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use crate::crawl_sync::crawl_for_source;
use crate::{
    FeedSourceIndexInput, GitSourceIndexInput, LocalSourceIndexInput, LocalSourceSelectionPolicy,
    RedditSourceIndexInput, RegistrySourceIndexInput, SessionSelector, SessionsSourceIndexInput,
    WebSourceIndexInput, YoutubeSourceIndexInput, clone_git_repo, fetch_feed_to_file,
    fetch_reddit_dump, fetch_registry_dump, fetch_youtube_dump, index_feed_source_with_job,
    index_git_source_with_job, index_local_source_with_job, index_reddit_source_with_job,
    index_registry_source_with_job, index_sessions_source_with_job, index_web_source_with_job,
    index_youtube_source_with_job, parse_registry_target, parse_session_selector,
};

/// Placeholder job id — every `index_*_source_with_job` bridge creates the real
/// job row and overwrites this with the descriptor's job id.
fn placeholder_job_id() -> JobId {
    JobId::new(Uuid::nil())
}

/// Local-path source: dispatch straight to the local bridge (no acquisition).
pub async fn dispatch_local(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=local"
    ));
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
    })
}

/// Git-repository source: shallow-clone (acquisition) then dispatch to the git
/// bridge. The `TempDir` is kept alive for the whole indexing pass.
pub async fn dispatch_git(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!("command=source collection={collection} kind=git"));
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
    })
}

/// Feed source: fetch the raw feed to a deterministic cache path then dispatch
/// to the feed bridge.
pub async fn dispatch_feed(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!("command=source collection={collection} kind=feed"));
    let feed_path = fetch_feed_to_file(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("feed fetch failed")?;
    let index_input = FeedSourceIndexInput {
        feed_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    };
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
    })
}

/// Reddit source: OAuth + fetch to a prepared dump then dispatch to the reddit
/// bridge. Missing credentials fail in `fetch_reddit_dump`, before any request.
pub async fn dispatch_reddit(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=reddit"
    ));
    let dump_path = fetch_reddit_dump(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("reddit fetch failed")?;
    let index_input = RedditSourceIndexInput {
        target: input.to_string(),
        dump_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    };
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
    })
}

/// YouTube source: yt-dlp fetch to a prepared dump then dispatch to the youtube
/// bridge. A missing yt-dlp binary fails in `fetch_youtube_dump`.
pub async fn dispatch_youtube(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=youtube"
    ));
    let dump_path = fetch_youtube_dump(input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("youtube fetch failed")?;
    let index_input = YoutubeSourceIndexInput {
        target: input.to_string(),
        youtube_dump_path: dump_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    };
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
    })
}

/// Registry source: parse the `pkg:` selector, fetch package metadata to a
/// prepared dump, then dispatch to the registry bridge.
pub async fn dispatch_registry(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=registry"
    ));
    let (registry, package) =
        parse_registry_target(input).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let dump_path = fetch_registry_dump(&registry, &package)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("registry fetch failed")?;
    let index_input = RegistrySourceIndexInput {
        registry_dump_path: dump_path,
        include_all_versions: false,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    };
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
    })
}

/// Session source: parse the `session:` selector (no network acquisition) then
/// dispatch to the sessions bridge.
pub async fn dispatch_session(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=session"
    ));
    let SessionSelector {
        sessions_root,
        provider,
        session_id,
    } = parse_session_selector(input).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let index_input = SessionsSourceIndexInput {
        sessions_root,
        provider,
        session_id,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    };
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
    })
}

/// Web source: crawl to completion (acquisition) then dispatch to the web
/// bridge. The web bridge owns vectorization, so `crawl_for_source` disables the
/// crawl's own embed pass.
pub async fn dispatch_web(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!("command=source collection={collection} kind=web"));
    let crawl = crawl_for_source(cfg, input)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("web crawl acquisition failed")?;
    log_info(&format!(
        "command=source kind=web crawl_pages={} crawl_markdown={} output_dir={}",
        crawl.pages_seen,
        crawl.markdown_files,
        crawl.output_dir.display()
    ));
    let index_input = WebSourceIndexInput {
        source: input.to_string(),
        scope,
        manifest_path: Some(crawl.manifest_path.clone()),
        markdown_root: Some(crawl.markdown_root.clone()),
        map_urls: Vec::new(),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
    };
    let output = index_web_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
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
    })
}
