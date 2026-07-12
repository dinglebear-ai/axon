//! Translate the ambient CLI-resolved [`Config`] into the web adapter's
//! `validated_options` map, so `dispatch_web` (issue #298 Wave 1b) keeps
//! honoring existing `--render-mode`/`--max-depth`/`--url-whitelist`/etc.
//! flags without a `crawl_for_source` acquisition pre-pass.

use axon_api::source::MetadataMap;
use axon_core::config::Config;

/// Build the web adapter's `validated_options` map
/// (`crates/axon-route/src/web_options.rs`) from the ambient CLI-resolved
/// `Config`. `max_pages_override` (the caller's own `SourceRequest.limits.
/// max_pages`) wins over `cfg.max_pages` when set — matching the pre-Wave-1b
/// behavior where `crawl_for_source`'s `max_pages` parameter overrode the
/// crawl config the same way.
pub(super) fn web_crawl_options(cfg: &Config, max_pages_override: Option<u64>) -> MetadataMap {
    let mut options = MetadataMap::new();
    let max_pages = max_pages_override.unwrap_or(cfg.max_pages as u64);
    options.insert("max_pages".to_string(), serde_json::json!(max_pages));
    options.insert(
        "max_depth".to_string(),
        serde_json::json!(cfg.max_depth as u64),
    );
    options.insert(
        "include_subdomains".to_string(),
        serde_json::json!(cfg.include_subdomains),
    );
    options.insert(
        "discover_sitemaps".to_string(),
        serde_json::json!(cfg.discover_sitemaps),
    );
    options.insert(
        "max_sitemaps".to_string(),
        serde_json::json!(cfg.max_sitemaps as u64),
    );
    options.insert(
        "sitemap_since_days".to_string(),
        serde_json::json!(cfg.sitemap_since_days),
    );
    options.insert(
        "min_markdown_chars".to_string(),
        serde_json::json!(cfg.min_markdown_chars as u64),
    );
    options.insert(
        "drop_thin_markdown".to_string(),
        serde_json::json!(cfg.drop_thin_markdown),
    );
    options.insert(
        "render_mode".to_string(),
        serde_json::json!(api_render_mode(cfg.render_mode)),
    );
    if !cfg.url_whitelist.is_empty() {
        options.insert(
            "url_whitelist".to_string(),
            serde_json::json!(cfg.url_whitelist),
        );
    }
    if !cfg.exclude_path_prefix.is_empty() {
        options.insert(
            "url_blacklist".to_string(),
            serde_json::json!(cfg.exclude_path_prefix),
        );
    }
    options
}

/// `axon_core::config::RenderMode` (kebab-case CLI representation) ->
/// `axon_api::source::RenderMode` (snake_case wire representation the web
/// adapter's option validator expects). Both enums have identical variants;
/// only the serde string form differs, so this is a plain match rather than
/// a serde round-trip.
fn api_render_mode(mode: axon_core::config::RenderMode) -> axon_api::source::RenderMode {
    use axon_api::source::RenderMode as ApiRenderMode;
    use axon_core::config::RenderMode as CoreRenderMode;
    match mode {
        CoreRenderMode::Http => ApiRenderMode::Http,
        CoreRenderMode::Chrome => ApiRenderMode::Chrome,
        CoreRenderMode::AutoSwitch => ApiRenderMode::AutoSwitch,
    }
}

#[cfg(test)]
#[path = "web_options_tests.rs"]
mod tests;
