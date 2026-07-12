//! Parsing + validation for the web adapter's real (non-legacy) option set.
//!
//! Mirrors the "Web Adapter" required-options table in
//! `docs/pipeline-unification/sources/adapter-scopes.md` (lines 188-210):
//! `max_pages`, `max_depth`, `include_subdomains`, `render_mode`,
//! `discover_sitemaps`, `max_sitemaps`, `sitemap_since_days`,
//! `url_whitelist`, `url_blacklist`, `etag_conditional`,
//! `min_markdown_chars`, `drop_thin_markdown`, `warc_path`,
//! `automation_script`, `verticals_enabled`.
//!
//! This is a **parsing/validation** pass only (#298 Wave 0 prerequisite): it
//! rejects malformed option values before a `RoutePlan` is constructed, so a
//! validated value can flow into `RoutePlan.validated_options` /
//! `SourcePlan.route.validated_options`. It does not resolve the doc's
//! `default: config` entries (that lives in `axon-core::Config`, which
//! `axon-route` deliberately does not depend on — see the crate's
//! `Cargo.toml`), and it is not yet consumed by `WebSourceAdapter` — wiring
//! real acquisition off these options is a later wave.
//!
//! The web `AdapterDefinition` in `capability.rs` also allows the legacy
//! `manifest_path`/`markdown_root`/`map_urls` keys used by the current
//! disk-handoff bridge (`axon-services::web_source::run::resolve_web_run`).
//! Those are intentionally left unvalidated by this module — they are being
//! replaced, not hardened, by this pass; [`validate`] passes them through
//! untouched via its catch-all arm.

use axon_api::{MetadataMap, RenderMode};
use axon_error::{ApiError, ErrorStage};
use serde_json::Value;

/// Validate every web-adapter option value present in `values`.
///
/// Keys not recognized as one of the real web options (including the legacy
/// `manifest_path`/`markdown_root`/`map_urls` trio) are left untouched here;
/// the router's generic `allowed_option_keys` membership check is what
/// rejects truly unknown keys before this ever runs.
pub(crate) fn validate(values: &MetadataMap) -> Result<(), ApiError> {
    for (key, value) in values.iter() {
        validate_option(key, value)?;
    }
    Ok(())
}

fn validate_option(key: &str, value: &Value) -> Result<(), ApiError> {
    match key {
        "max_pages" | "max_depth" | "max_sitemaps" | "sitemap_since_days"
        | "min_markdown_chars" => expect_non_negative_integer(key, value),
        "include_subdomains" | "discover_sitemaps" | "etag_conditional" | "drop_thin_markdown"
        | "verticals_enabled" => expect_bool(key, value),
        "render_mode" => expect_render_mode(value),
        "url_whitelist" | "url_blacklist" => expect_string_array(key, value),
        "warc_path" | "automation_script" => expect_nonempty_string(key, value),
        // Legacy disk-handoff keys (`manifest_path`, `markdown_root`,
        // `map_urls`) and anything else the router already allowed: nothing
        // further to validate here.
        _ => Ok(()),
    }
}

fn expect_bool(key: &str, value: &Value) -> Result<(), ApiError> {
    if value.is_boolean() {
        Ok(())
    } else {
        Err(invalid_option(key, "expected a boolean"))
    }
}

/// `max_pages`/`max_depth`/`max_sitemaps`/`sitemap_since_days`/
/// `min_markdown_chars` are all documented as plain (non-negative) integers
/// with no fixed upper bound of their own — the doc calls them "hard caps",
/// not range-limited fields — so this only rejects negative numbers,
/// non-integers, and non-numbers.
fn expect_non_negative_integer(key: &str, value: &Value) -> Result<(), ApiError> {
    if value.as_u64().is_some() {
        Ok(())
    } else {
        Err(invalid_option(key, "expected a non-negative integer"))
    }
}

/// Delegates to `axon_api::RenderMode`'s own `Deserialize` impl so the set of
/// valid enum values (`http`, `chrome`, `auto_switch`) has exactly one source
/// of truth.
fn expect_render_mode(value: &Value) -> Result<(), ApiError> {
    serde_json::from_value::<RenderMode>(value.clone())
        .map(|_| ())
        .map_err(|_| {
            invalid_option(
                "render_mode",
                "expected \"http\", \"chrome\", or \"auto_switch\"",
            )
        })
}

fn expect_string_array(key: &str, value: &Value) -> Result<(), ApiError> {
    let Some(array) = value.as_array() else {
        return Err(invalid_option(key, "expected an array of strings"));
    };
    for (index, entry) in array.iter().enumerate() {
        match entry.as_str() {
            Some(text) if !text.trim().is_empty() => {}
            Some(_) => {
                return Err(invalid_option(
                    key,
                    &format!("entry {index} must not be empty"),
                ));
            }
            None => {
                return Err(invalid_option(
                    key,
                    &format!("entry {index} must be a string"),
                ));
            }
        }
    }
    Ok(())
}

fn expect_nonempty_string(key: &str, value: &Value) -> Result<(), ApiError> {
    match value.as_str() {
        Some(text) if !text.trim().is_empty() => Ok(()),
        Some(_) => Err(invalid_option(key, "must not be empty")),
        None => Err(invalid_option(key, "expected a non-empty string")),
    }
}

fn invalid_option(key: &str, reason: &str) -> ApiError {
    ApiError::new(
        "route.options.invalid",
        ErrorStage::Routing,
        format!("invalid value for web adapter option `{key}`: {reason}"),
    )
    .with_context("adapter", "web")
    .with_context("option", key.to_string())
    .with_context("reason", reason.to_string())
}

#[cfg(test)]
#[path = "web_options_tests.rs"]
mod tests;
