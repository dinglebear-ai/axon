//! Post-init validation and profile-default application for `Config`.
//!
//! Split out of `into_config()` (bead axon_rust-2j9.6). Behavior is unchanged:
//!   * Validates the parent of `output_path` exists when explicitly set.
//!   * Restores default exclude-path prefixes when the user did not opt out.
//!   * Applies `performance::profile_settings()` to fields the user did not set.
//!   * Derives `output_dir` from the canonical Axon data directory when still at the clap default.

use super::super::super::cli::DEFAULT_OUTPUT_DIR;
use super::super::super::types::Config;
use super::super::excludes;
use super::super::performance;

/// Flags captured before `Config` was built that the post-init pass needs.
pub(super) struct PostInit {
    pub disable_default_excludes: bool,
    pub fetch_retries_was_set: bool,
    pub retry_backoff_was_set: bool,
    pub output_dir_was_explicit: bool,
}

pub(super) fn apply(cfg: &mut Config, ctx: PostInit) -> Result<(), String> {
    // Validate output path parent exists when explicitly set.
    if let Some(ref path) = cfg.output_path
        && let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        return Err(format!(
            "output directory '{}' does not exist",
            parent.display()
        ));
    }

    if cfg.exclude_path_prefix.is_empty() && !ctx.disable_default_excludes {
        cfg.exclude_path_prefix = excludes::default_exclude_prefixes_vec();
    }

    let ps = performance::profile_settings(cfg.performance_profile);

    if cfg.crawl_concurrency_limit.is_none() {
        cfg.crawl_concurrency_limit = Some(ps.crawl_concurrency);
    }
    if cfg.backfill_concurrency_limit.is_none() {
        cfg.backfill_concurrency_limit = Some(ps.backfill_concurrency);
    }
    if cfg.request_timeout_ms.is_none() {
        cfg.request_timeout_ms = Some(ps.request_timeout_ms);
    }
    if !ctx.fetch_retries_was_set {
        cfg.fetch_retries = ps.fetch_retries;
    }
    if !ctx.retry_backoff_was_set {
        cfg.retry_backoff_ms = ps.retry_backoff_ms;
    }
    cfg.crawl_broadcast_buffer_min = ps.broadcast_buffer_min;
    cfg.crawl_broadcast_buffer_max = ps.broadcast_buffer_max;

    // Derive output_dir from the canonical data directory when still at the clap default.
    if !ctx.output_dir_was_explicit && cfg.output_dir == std::path::Path::new(DEFAULT_OUTPUT_DIR) {
        cfg.output_dir = crate::core::paths::axon_data_base_dir().join("output");
    }
    Ok(())
}
