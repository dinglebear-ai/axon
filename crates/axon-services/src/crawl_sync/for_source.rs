//! Crawl acquisition for `axon source <web-url>`.
//!
//! Runs a synchronous crawl to completion (the same engine `axon crawl --wait`
//! uses) and returns the prepared crawl-output directory so the web source
//! bridge can read its `manifest.jsonl` + `markdown/` tree. Embedding is
//! disabled for this path — vectorization is owned by the web bridge, not the
//! crawl's own embed pass.

use std::error::Error;
use std::path::{Path, PathBuf};

use axon_core::config::Config;
use axon_core::content::url_to_domain;

use super::crawl_sync;

/// Prepared crawl output for one web source acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlForSourceResult {
    /// Root directory the crawl wrote (`.../domains/<domain>/sync`).
    pub output_dir: PathBuf,
    /// `<output_dir>/manifest.jsonl` — the web adapter's discovery input.
    pub manifest_path: PathBuf,
    /// `<output_dir>/markdown` — the web adapter's markdown root.
    pub markdown_root: PathBuf,
    /// Pages the crawl saw.
    pub pages_seen: u32,
    /// Markdown files written.
    pub markdown_files: u32,
}

/// Predict the directory a synchronous crawl writes for `start_url`.
///
/// Mirrors [`crawl_sync::crawl_sync_effective_config`]'s re-rooting
/// (`<output_dir>/domains/<domain>/sync`). Pure — no I/O — so it is unit-testable
/// and callers can locate the crawl output without re-running the crawl.
pub fn crawl_sync_output_dir(base_output_dir: &Path, start_url: &str) -> PathBuf {
    base_output_dir
        .join("domains")
        .join(url_to_domain(start_url))
        .join("sync")
}

/// Map a crawl-sync output dir to its `manifest.jsonl` + `markdown/` paths.
///
/// Pure helper so the path contract stays in one place and can be unit-tested
/// without a live crawl.
pub fn crawl_output_manifest_and_markdown(output_dir: &Path) -> (PathBuf, PathBuf) {
    (
        output_dir.join("manifest.jsonl"),
        output_dir.join("markdown"),
    )
}

/// Crawl `start_url` to completion (embed disabled) and return the prepared
/// crawl-output paths for the web source bridge.
pub async fn crawl_for_source(
    cfg: &Config,
    start_url: &str,
) -> Result<CrawlForSourceResult, Box<dyn Error>> {
    // The web bridge owns vectorization; disable the crawl's own embed pass so
    // pages are not embedded twice (and not embedded as raw crawl payloads that
    // bypass the source ledger).
    let mut crawl_cfg = cfg.clone();
    crawl_cfg.embed = false;

    let summary = crawl_sync(&crawl_cfg, start_url).await?;

    let output_dir = crawl_sync_output_dir(&crawl_cfg.output_dir, start_url);
    let (manifest_path, markdown_root) = crawl_output_manifest_and_markdown(&output_dir);
    Ok(CrawlForSourceResult {
        output_dir,
        manifest_path,
        markdown_root,
        pages_seen: summary.pages_seen,
        markdown_files: summary.markdown_files,
    })
}

#[cfg(test)]
#[path = "for_source_tests.rs"]
mod tests;
