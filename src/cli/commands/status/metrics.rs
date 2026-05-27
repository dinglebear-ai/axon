mod format;

pub(crate) use format::{format_error, job_runtime_text};

use serde_json::Value;

/// Extract the `"collection"` string from a job's `config_json`, if present.
pub(crate) fn collection_from_config(config_json: &Value) -> Option<&str> {
    config_json.get("collection").and_then(|v| v.as_str())
}

/// Extract crawl job UUID from an embed input path.
/// Supports both legacy `.cache/axon-rust/output/jobs/<UUID>/markdown` and
/// current `.cache/axon-rust/output/domains/<domain>/<UUID>/markdown` layouts.
pub(super) fn crawl_uuid_from_embed_input(input: &str) -> Option<uuid::Uuid> {
    use std::path::{Component, Path};
    for component in Path::new(input).components() {
        if let Component::Normal(segment) = component
            && let Some(s) = segment.to_str()
            && let Ok(uid) = s.parse::<uuid::Uuid>()
        {
            return Some(uid);
        }
    }
    None
}

/// Resolve a human-readable label for an embed job's input_text.
/// Priority: crawl URL lookup → URL passthrough → pretty path.
pub(crate) fn display_embed_input<'a>(
    input: &'a str,
    crawl_url_map: &std::collections::HashMap<uuid::Uuid, &'a str>,
) -> std::borrow::Cow<'a, str> {
    if let Some(url) =
        crawl_uuid_from_embed_input(input).and_then(|uid| crawl_url_map.get(&uid).copied())
    {
        return std::borrow::Cow::Borrowed(url);
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        return std::borrow::Cow::Borrowed(input);
    }
    let path = std::path::Path::new(input);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(input);
    if name == "markdown" {
        return std::borrow::Cow::Owned(
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|parent| format!("{parent}/markdown"))
                .unwrap_or_else(|| "output/markdown".to_string()),
        );
    }
    std::borrow::Cow::Borrowed(path.to_str().unwrap_or(input))
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
