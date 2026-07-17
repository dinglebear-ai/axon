//! Translate the ambient CLI-resolved [`Config`] into the web adapter's
//! `validated_options` map, so `dispatch_web` (issue #298 Wave 1b) keeps
//! honoring existing `--render-mode`/`--max-depth`/`--url-whitelist`/etc.
//! flags without a `crawl_for_source` acquisition pre-pass.

use axon_api::source::{AuthScope, AuthSnapshot, MetadataMap};
use axon_core::config::Config;
use axon_error::{ApiError, ErrorStage};

/// Build the web adapter's `validated_options` map
/// (`crates/axon-route/src/web_options.rs`) from the ambient CLI-resolved
/// `Config`. `max_pages_override`/`max_depth_override` (the caller's own
/// `SourceRequest.limits`) win over config when set — matching the
/// pre-Wave-1b behavior where `crawl_for_source`'s per-call bounds overrode
/// the crawl config the same way.
pub(crate) fn web_crawl_options(
    cfg: &Config,
    max_pages_override: Option<u64>,
    max_depth_override: Option<u32>,
) -> MetadataMap {
    let mut options = MetadataMap::new();
    let max_pages = max_pages_override.unwrap_or(cfg.max_pages as u64);
    let max_depth = max_depth_override.unwrap_or(cfg.max_depth as u32);
    options.insert("max_pages".to_string(), serde_json::json!(max_pages));
    options.insert("max_depth".to_string(), serde_json::json!(max_depth));
    options.insert(
        "include_subdomains".to_string(),
        serde_json::json!(cfg.include_subdomains),
    );
    options.insert(
        "respect_robots".to_string(),
        serde_json::json!(cfg.respect_robots),
    );
    options.insert(
        "discover_sitemaps".to_string(),
        serde_json::json!(cfg.discover_sitemaps),
    );
    options.insert(
        "discover_llms_txt".to_string(),
        serde_json::json!(cfg.discover_llms_txt),
    );
    options.insert(
        "max_llms_txt_urls".to_string(),
        serde_json::json!(cfg.max_llms_txt_urls as u64),
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
        "etag_conditional".to_string(),
        serde_json::json!(cfg.etag_conditional),
    );
    options.insert(
        "cache_policy".to_string(),
        serde_json::json!(if cfg.etag_conditional {
            "revalidate"
        } else {
            "bypass"
        }),
    );
    options.insert(
        "render_mode".to_string(),
        serde_json::json!(api_render_mode(cfg.render_mode)),
    );
    options.insert(
        "verticals_enabled".to_string(),
        serde_json::json!(cfg.enable_verticals),
    );
    options.insert(
        "vertical_cache_ttl_secs".to_string(),
        serde_json::json!(cfg.vertical_cache_ttl_secs),
    );
    if !cfg.auto_dispatch_skip.is_empty() {
        options.insert(
            "auto_dispatch_skip".to_string(),
            serde_json::json!(cfg.auto_dispatch_skip),
        );
    }
    if let Some(user_agent) = cfg.user_agent.as_deref().filter(|value| !value.is_empty()) {
        options.insert("user_agent".to_string(), serde_json::json!(user_agent));
    }
    if let Some(path) = cfg.warc_output.as_ref() {
        options.insert(
            "warc_path".to_string(),
            serde_json::json!(path.to_string_lossy()),
        );
    }
    if let Some(path) = cfg.automation_script.as_ref() {
        options.insert(
            "automation_script".to_string(),
            serde_json::json!(path.to_string_lossy()),
        );
    }
    if !cfg.custom_headers.is_empty() {
        options.insert(
            "headers".to_string(),
            serde_json::Value::Object(header_options(&cfg.custom_headers)),
        );
    }
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

fn header_options(headers: &[String]) -> serde_json::Map<String, serde_json::Value> {
    headers
        .iter()
        .filter_map(|raw| raw.split_once(':'))
        .map(|(name, value)| {
            (
                name.trim().to_string(),
                serde_json::Value::String(value.trim().to_string()),
            )
        })
        .collect()
}

/// Merge caller-provided web adapter options into trusted config-derived
/// options. Most options are ordinary crawl knobs, but `automation_script`
/// points at a local JSON program that Chrome may execute; remote callers need
/// explicit local+execute scope for that key.
pub(crate) fn merge_caller_web_options(
    options: &mut MetadataMap,
    caller_options: &MetadataMap,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    for (key, value) in caller_options.0.iter() {
        if key == "automation_script" && !caller_can_set_automation_script(auth_snapshot) {
            return Err(ApiError::new(
                "auth.scope_required",
                ErrorStage::Authorizing,
                "web option automation_script requires axon:local and axon:execute",
            )
            .with_context("option", "automation_script")
            .with_context("required_scope", "axon:local,axon:execute"));
        }
        options.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn caller_can_set_automation_script(auth_snapshot: Option<&AuthSnapshot>) -> bool {
    let Some(snapshot) = auth_snapshot else {
        return true;
    };
    crate::source::authorize::snapshot_allows_scope(snapshot, AuthScope::Local)
        && crate::source::authorize::snapshot_allows_scope(snapshot, AuthScope::Execute)
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
