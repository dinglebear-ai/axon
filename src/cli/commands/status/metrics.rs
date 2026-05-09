mod format;
mod ingest;

pub(crate) use format::{format_error, job_runtime_text};

use crate::core::ui::{accent, metric, subtle, symbol_for_status};
use serde_json::Value;

#[allow(dead_code)]
pub(super) fn section_symbol(statuses: &[&str]) -> String {
    if statuses.iter().any(|s| matches!(*s, "failed" | "error")) {
        symbol_for_status("failed")
    } else if statuses
        .iter()
        .any(|s| matches!(*s, "pending" | "running" | "processing" | "scraping"))
    {
        symbol_for_status("pending")
    } else {
        symbol_for_status("completed")
    }
}

#[allow(dead_code)]
pub(super) fn extract_metrics_suffix(result_json: Option<&Value>, url_count: usize) -> String {
    let sep = subtle(" | ");
    let mut parts = vec![metric(url_count, "urls")];
    if let Some(total_items) = result_json
        .and_then(|r| r.get("total_items"))
        .and_then(|v| v.as_u64())
    {
        parts.push(metric(total_items, "items"));
    }
    if let Some(pages) = result_json
        .and_then(|r| r.get("pages_visited"))
        .and_then(|v| v.as_u64())
    {
        parts.push(metric(pages, "pages"));
    }
    format!("{sep}{}", parts.join(&sep))
}

pub(crate) fn embed_metrics_suffix(status: &str, result_json: Option<&Value>) -> String {
    let sep = subtle(" | ");
    if matches!(status, "pending" | "running" | "processing") {
        if let (Some(done), Some(total)) = (
            result_json
                .and_then(|r| r.get("docs_completed"))
                .and_then(|v| v.as_u64()),
            result_json
                .and_then(|r| r.get("docs_total"))
                .and_then(|v| v.as_u64()),
        ) {
            return format!(
                "{sep}{}{}{} {}",
                accent(&done.to_string()),
                subtle("/"),
                accent(&total.to_string()),
                accent("docs")
            );
        }
        return String::new();
    }
    let docs = result_json
        .and_then(|r| r.get("docs_embedded"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let chunks = result_json
        .and_then(|r| r.get("chunks_embedded"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if docs == 0 && chunks == 0 {
        return String::new();
    }
    format!(
        "{sep}{}{sep}{}",
        metric(docs, "docs"),
        metric(chunks, "chunks")
    )
}

/// Extract the `"collection"` string from a job's `config_json`, if present.
pub(crate) fn collection_from_config(config_json: &Value) -> Option<&str> {
    config_json.get("collection").and_then(|v| v.as_str())
}

#[allow(dead_code)]
pub(super) fn summarize_urls(urls_json: &Value) -> (String, usize) {
    let urls = urls_json
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let count = urls.len();
    if count == 0 {
        return ("(no targets)".to_string(), 0);
    }
    let first = urls[0].clone();
    let label = if count > 1 {
        format!("{first} (+{} more)", count - 1)
    } else {
        first
    };
    (label, count)
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
mod tests {
    use super::*;

    #[test]
    fn collection_from_config_extracts_collection() {
        let json = serde_json::json!({"collection": "cortex"});
        assert_eq!(collection_from_config(&json), Some("cortex"));
    }

    #[test]
    fn collection_from_config_returns_none_for_missing() {
        let json = serde_json::json!({});
        assert_eq!(collection_from_config(&json), None);
    }

    #[test]
    fn collection_from_config_returns_none_for_non_string() {
        let json = serde_json::json!({"collection": 42});
        assert_eq!(collection_from_config(&json), None);
    }

    #[test]
    fn collection_from_config_handles_null() {
        let json = serde_json::json!(null);
        assert_eq!(collection_from_config(&json), None);
    }

    #[test]
    fn display_embed_input_uses_crawl_url_for_domain_output_path() {
        let crawl_id = match uuid::Uuid::parse_str("2313c2c5-29b8-46a6-a98d-2338f6b09a9d") {
            Ok(id) => id,
            Err(err) => panic!("test UUID should parse: {err}"),
        };
        let mut crawl_url_map = std::collections::HashMap::new();
        crawl_url_map.insert(crawl_id, "https://mem0.ai/");

        let label = display_embed_input(
            ".cache/axon-rust/output/domains/mem0.ai/2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown",
            &crawl_url_map,
        );

        assert_eq!(label, "https://mem0.ai/");
    }

    #[test]
    fn display_embed_input_preserves_path_when_crawl_url_is_unknown() {
        let crawl_url_map = std::collections::HashMap::new();

        let label = display_embed_input(
            ".cache/axon-rust/output/domains/mem0.ai/2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown",
            &crawl_url_map,
        );

        assert_eq!(label, "2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown");
    }
}
