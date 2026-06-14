mod batch;
mod clone;
mod prepare;

use batch::collect_and_embed_batched;
use clone::clone_repo;
use prepare::{build_file_embed_ctx, collect_indexable_files};

use anyhow::{Result, bail};

use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::ingest::progress::PhaseReporter;

use super::GitHubCommonFields;

const PHASE_CLONING: &str = "cloning";
const PHASE_ENUMERATING_FILES: &str = "enumerating_files";
const PHASE_EMBEDDED_FILES: &str = "embedded_files";

/// Clone the repo and embed all indexable files concurrently.
pub async fn embed_files(
    cfg: &Config,
    common: &GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
    reporter: &PhaseReporter,
) -> Result<usize> {
    reporter
        .report(serde_json::json!({
            "phase": PHASE_CLONING,
            "repo": common.repo_slug,
        }))
        .await;

    log_info(&format!(
        "github clone_start repo={} branch={}",
        common.repo_slug, common.default_branch
    ));
    let tmp = clone_repo(common, &common.default_branch, token).await?;
    let repo_root = tmp.path().to_path_buf();

    reporter
        .report(serde_json::json!({
            "phase": PHASE_ENUMERATING_FILES,
            "repo": common.repo_slug,
        }))
        .await;

    let file_items =
        collect_indexable_files(&repo_root, include_source, &cfg.ingest_exclude_paths).await?;
    let files_total = file_items.len();

    log_info(&format!(
        "github clone complete indexable={files_total} repo={}",
        common.repo_slug
    ));

    let ctx = build_file_embed_ctx(cfg, common, repo_root);
    let stats =
        collect_and_embed_batched(&ctx, file_items, files_total, include_source, reporter).await?;

    reporter
        .report(serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": stats.chunks_embedded,
            "file_read_failures": stats.failed_file_reads,
            "cleanup_blocking_skips": stats.cleanup_blocking_skips,
            "embed_batches_failed": stats.failed_batches,
            "embed_files_failed": stats.failed_files,
            "embed_docs_failed": stats.failed_docs,
            "embed_chunks_failed": stats.failed_chunks,
            "phase": PHASE_EMBEDDED_FILES,
        }))
        .await;

    log_info(&format!(
        "github files_embedded total={files_total} read_failed={} cleanup_blocking_skips={} batches_failed={} files_failed={} docs_failed={} chunks_failed={} chunks={}",
        stats.failed_file_reads,
        stats.cleanup_blocking_skips,
        stats.failed_batches,
        stats.failed_files,
        stats.failed_docs,
        stats.failed_chunks,
        stats.chunks_embedded
    ));
    if stats.has_failed_batches() {
        bail!(
            "github file embedding had embed-batch failures: read_failed={} batches_failed={} files_failed={} docs_failed={} chunks_failed={} chunks_embedded={}",
            stats.failed_file_reads,
            stats.failed_batches,
            stats.failed_files,
            stats.failed_docs,
            stats.failed_chunks,
            stats.chunks_embedded
        );
    }

    Ok(stats.chunks_embedded)
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
