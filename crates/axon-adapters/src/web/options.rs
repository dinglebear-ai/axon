//! Parsing helpers for the web adapter's `validated_options` (see
//! `crates/axon-route/src/web_options.rs` for the validation pass that runs
//! before these values ever reach the adapter) and the `Config` this crate's
//! (temporary, Wave 2 removes it) `axon-crawl` dependency needs for `Site`/
//! `Docs` discovery.

use std::path::PathBuf;

use axon_api::source::*;
use axon_core::config::Config;
use serde_json::Value;

use crate::providers::chrome_render::map_render_mode;

/// Mirrors `axon_core::config::Config::default().min_markdown_chars`. Kept as
/// a local constant (rather than depending on `Config::default()` from the
/// hot per-item acquire path) because `acquire` only needs this one scalar,
/// not a full `Config`.
const DEFAULT_MIN_MARKDOWN_CHARS: usize = 200;

pub(super) fn effective_render_mode(values: &MetadataMap) -> RenderMode {
    values
        .get("render_mode")
        .and_then(|value| serde_json::from_value::<RenderMode>(value.clone()).ok())
        .unwrap_or(RenderMode::AutoSwitch)
}

pub(super) fn min_markdown_chars(values: &MetadataMap) -> usize {
    usize_option(values, "min_markdown_chars").unwrap_or(DEFAULT_MIN_MARKDOWN_CHARS)
}

fn bool_option(values: &MetadataMap, key: &str) -> Option<bool> {
    values.get(key).and_then(Value::as_bool)
}

fn u32_option(values: &MetadataMap, key: &str) -> Option<u32> {
    values
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn usize_option(values: &MetadataMap, key: &str) -> Option<usize> {
    values.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

fn string_array_option(values: &MetadataMap, key: &str) -> Vec<String> {
    values
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Build the `axon-crawl` `Config` for one `Site`/`Docs` discovery crawl,
/// driven by `plan.route.validated_options` (falling back to `Config::default()`
/// for anything absent). `output_dir` is the caller's ephemeral scratch
/// directory — see `web/site_discovery.rs`.
///
/// `url_blacklist` maps onto `Config::exclude_path_prefix` — the closest
/// existing engine knob (`runtime::configure_website_with_crawl_id` folds it,
/// together with the SSRF blacklist, into Spider's `with_blacklist_url`).
pub(super) fn build_discovery_config(plan: &SourcePlan, output_dir: PathBuf) -> Config {
    let values = &plan.route.validated_options.values;
    let mut cfg = Config {
        output_dir,
        embed: false,
        // Defense-in-depth (issue #298 Wave 1b): this ephemeral, adapter-owned
        // discovery crawl must never opt into Spider's built-in crawl-result
        // caching or the whole-crawl disk-TTL shortcut that used to live in
        // `axon-services::crawl_sync` — `LedgerStore::diff_manifest` is now the
        // sole staleness authority. `Config::default()` already sets this to
        // `false`; forcing it here survives a future default flip.
        cache: false,
        render_mode: map_render_mode(effective_render_mode(values)),
        ..Config::default()
    };
    if let Some(value) = u32_option(values, "max_pages") {
        cfg.max_pages = value;
    }
    if let Some(value) = usize_option(values, "max_depth") {
        cfg.max_depth = value;
    }
    if let Some(value) = bool_option(values, "include_subdomains") {
        cfg.include_subdomains = value;
    }
    if let Some(value) = bool_option(values, "discover_sitemaps") {
        cfg.discover_sitemaps = value;
    }
    if let Some(value) = usize_option(values, "max_sitemaps") {
        cfg.max_sitemaps = value;
    }
    if let Some(value) = u32_option(values, "sitemap_since_days") {
        cfg.sitemap_since_days = value;
    }
    if let Some(value) = usize_option(values, "min_markdown_chars") {
        cfg.min_markdown_chars = value;
    }
    if let Some(value) = bool_option(values, "drop_thin_markdown") {
        cfg.drop_thin_markdown = value;
    }
    let whitelist = string_array_option(values, "url_whitelist");
    if !whitelist.is_empty() {
        cfg.url_whitelist = whitelist;
    }
    let blacklist = string_array_option(values, "url_blacklist");
    if !blacklist.is_empty() {
        cfg.exclude_path_prefix.extend(blacklist);
    }
    cfg
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
