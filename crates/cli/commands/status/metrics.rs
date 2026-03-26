use crate::crates::core::ui::{accent, metric, muted, subtle, symbol_for_status};
use chrono::{DateTime, Utc};
use serde_json::Value;

fn format_duration(mut secs: u64) -> String {
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3_600;
    secs %= 3_600;
    let minutes = secs / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// Human-readable relative age: "3s ago", "12m ago", "2h ago", "4d ago".
pub(super) fn format_age(ts: &DateTime<Utc>) -> String {
    let secs = (Utc::now() - *ts).num_seconds().max(0) as u64;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Human-readable run duration.
///
/// Terminal jobs show total runtime from `started_at` to `finished_at`.
/// Active jobs show elapsed runtime from `started_at` to now.
/// If runtime anchors are missing, falls back to relative age.
pub(crate) fn job_runtime_text(
    status: &str,
    started_at: Option<&DateTime<Utc>>,
    finished_at: Option<&DateTime<Utc>>,
    updated_at: &DateTime<Utc>,
) -> String {
    match status {
        "completed" | "failed" | "canceled" => {
            if let (Some(started), Some(finished)) = (started_at, finished_at) {
                let secs = (*finished - *started).num_seconds().max(0) as u64;
                format_duration(secs)
            } else {
                format_age(finished_at.unwrap_or(updated_at))
            }
        }
        "running" | "processing" | "scraping" => {
            if let Some(started) = started_at {
                let secs = (Utc::now() - *started).num_seconds().max(0) as u64;
                format_duration(secs)
            } else {
                format_age(updated_at)
            }
        }
        _ => format_age(updated_at),
    }
}

/// First line of error_text, truncated to 60 chars.
pub(crate) fn format_error(error_text: Option<&str>) -> Option<String> {
    let text = error_text?.trim();
    if text.is_empty() {
        return None;
    }
    let first_line = text.lines().next().unwrap_or(text);
    if first_line.chars().count() > 60 {
        Some(format!(
            "{}…",
            crate::crates::cli::commands::common::truncate_chars(first_line, 60)
        ))
    } else {
        Some(first_line.to_string())
    }
}

/// Section header symbol: ✗ if any failed, ◐ if any active, ✓ if all terminal.
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

#[allow(dead_code)]
pub(super) fn ingest_metrics_suffix(status: &str, result_json: Option<&Value>) -> String {
    let sep = subtle(" | ");
    if matches!(status, "pending" | "running" | "processing") {
        return ingest_active_metrics_suffix(result_json, &sep);
    }
    ingest_completed_metrics_suffix(result_json, &sep)
}

#[allow(dead_code)]
fn ingest_active_metrics_suffix(result_json: Option<&Value>, sep: &str) -> String {
    let Some(r) = result_json else {
        return String::new();
    };
    let chunks = r
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let tasks_done = r.get("tasks_done").and_then(|v| v.as_u64());
    let tasks_total = r.get("tasks_total").and_then(|v| v.as_u64());
    let phase = r.get("phase").and_then(|v| v.as_str());

    // When per-task tracking fields are present, build a rich multi-part suffix.
    if tasks_done.is_some() || tasks_total.is_some() || phase.is_some() {
        return build_rich_active_suffix(r, chunks, tasks_done, tasks_total, phase, sep);
    }

    // Legacy path: no per-task fields — use the original progress helpers.
    if let Some(line) =
        progress_with_chunks(r, "videos_done", "videos_total", "videos", chunks, sep)
    {
        return line;
    }
    if let Some(line) = progress_with_chunks(r, "files_done", "files_total", "files", chunks, sep) {
        return line;
    }

    let enumerating = r
        .get("enumerating")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match () {
        _ if enumerating => format!("{sep}{}", muted("enumerating…")),
        _ if chunks > 0 => format!("{sep}{}", metric(chunks, "chunks")),
        _ => String::new(),
    }
}

/// Build a multi-part status string when per-task tracking fields are available.
///
/// Parts (all optional, shown when present):
/// - File progress: `"150/155 files"`
/// - Chunk count: `"1700 chunks"`
/// - Task progress: `"3/5 tasks"`
/// - Current phase label
/// - Phase-specific detail (e.g., `"42 issues, page 2"` for `fetching_issues`)
#[allow(dead_code)]
fn build_rich_active_suffix(
    r: &Value,
    chunks: u64,
    tasks_done: Option<u64>,
    tasks_total: Option<u64>,
    phase: Option<&str>,
    sep: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // File progress (videos take priority over files)
    if let Some(frag) = format_fraction(r, "videos_done", "videos_total", "videos") {
        parts.push(frag);
    } else if let Some(frag) = format_fraction(r, "files_done", "files_total", "files") {
        parts.push(frag);
    }

    // Chunk count
    if chunks > 0 {
        parts.push(metric(chunks, "chunks"));
    }

    // Task progress
    if let (Some(done), Some(total)) = (tasks_done, tasks_total) {
        parts.push(format!(
            "{}{}{} {}",
            accent(&done.to_string()),
            subtle("/"),
            accent(&total.to_string()),
            accent("tasks"),
        ));
    }

    // Phase label
    if let Some(p) = phase {
        parts.push(muted(p));
    }

    // Phase-specific detail
    if let Some(detail) = phase_detail(r, phase) {
        parts.push(detail);
    }

    if parts.is_empty() {
        return String::new();
    }
    format!("{sep}{}", parts.join(sep))
}

/// Format `"done/total label"` from two JSON keys, if both are present.
#[allow(dead_code)]
fn format_fraction(r: &Value, done_key: &str, total_key: &str, label: &str) -> Option<String> {
    let done = r.get(done_key).and_then(|v| v.as_u64())?;
    let total = r.get(total_key).and_then(|v| v.as_u64())?;
    Some(format!(
        "{}{}{} {}",
        accent(&done.to_string()),
        subtle("/"),
        accent(&total.to_string()),
        accent(label),
    ))
}

/// Contextual detail string for the current phase.
#[allow(dead_code)]
fn phase_detail(r: &Value, phase: Option<&str>) -> Option<String> {
    match phase? {
        "fetching_issues" => fetch_detail(r, "issues_fetched", "issues_page", "issues"),
        "fetching_prs" => fetch_detail(r, "prs_fetched", "issues_page", "PRs"),
        "embedding_issues" | "embedding_prs" | "embedding_wiki" => {
            let total = r
                .get("issues_total")
                .or_else(|| r.get("prs_total"))
                .or_else(|| r.get("wiki_pages"))
                .and_then(|v| v.as_u64());
            total.map(|n| format!("{} items", accent(&n.to_string())))
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn fetch_detail(r: &Value, count_key: &str, page_key: &str, label: &str) -> Option<String> {
    let fetched = r.get(count_key).and_then(|v| v.as_u64());
    let page = r.get(page_key).and_then(|v| v.as_u64());
    match (fetched, page) {
        (Some(n), Some(p)) => Some(format!(
            "{} {label}, page {}",
            accent(&n.to_string()),
            accent(&p.to_string()),
        )),
        (Some(n), None) => Some(format!("{} {label}", accent(&n.to_string()))),
        (None, Some(p)) => Some(format!("page {}", accent(&p.to_string()))),
        (None, None) => None,
    }
}

#[allow(dead_code)]
fn ingest_completed_metrics_suffix(result_json: Option<&Value>, sep: &str) -> String {
    let Some(r) = result_json else {
        return String::new();
    };
    let chunks = r
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if chunks == 0 {
        return String::new();
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "videos_done", "videos_total", "videos", chunks, sep)
    {
        return line;
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "files_done", "files_total", "files", chunks, sep)
    {
        return line;
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "tasks_done", "tasks_total", "tasks", chunks, sep)
    {
        return line;
    }
    format!("{sep}{}", metric(chunks, "chunks"))
}

#[allow(dead_code)]
fn progress_with_chunks(
    payload: &Value,
    done_key: &str,
    total_key: &str,
    label: &str,
    chunks: u64,
    sep: &str,
) -> Option<String> {
    let done = payload.get(done_key).and_then(|v| v.as_u64())?;
    let total = payload.get(total_key).and_then(|v| v.as_u64())?;
    Some(format_progress_with_chunks(done, total, label, chunks, sep))
}

#[allow(dead_code)]
fn completed_progress_with_chunks(
    payload: &Value,
    done_key: &str,
    total_key: &str,
    label: &str,
    chunks: u64,
    sep: &str,
) -> Option<String> {
    let total = payload.get(total_key).and_then(|v| v.as_u64())?;
    let done = payload
        .get(done_key)
        .and_then(|v| v.as_u64())
        .unwrap_or(total);
    Some(format_progress_with_chunks(done, total, label, chunks, sep))
}

#[allow(dead_code)]
fn format_progress_with_chunks(
    done: u64,
    total: u64,
    label: &str,
    chunks: u64,
    sep: &str,
) -> String {
    format!(
        "{sep}{}{}{} {label}{sep}{}",
        accent(&done.to_string()),
        subtle("/"),
        accent(&total.to_string()),
        metric(chunks, "chunks"),
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
///
/// Uses `std::path::Path::components()` for portable path segment iteration
/// instead of splitting on `/`.
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
    use chrono::Duration;

    /// Strip ANSI escape sequences so test assertions compare plain text.
    fn strip_ansi(s: &str) -> String {
        console::strip_ansi_codes(s).into_owned()
    }

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
    fn job_runtime_text_reports_running_elapsed_from_started_at() {
        let started = Utc::now() - Duration::seconds(125);
        let updated = Utc::now();
        let value = job_runtime_text("running", Some(&started), None, &updated);
        // Allow 1s drift: Utc::now() inside job_runtime_text may differ from the one above.
        assert!(
            value == "2m5s" || value == "2m6s",
            "expected '2m5s' or '2m6s', got '{value}'"
        );
    }

    #[test]
    fn job_runtime_text_reports_completed_duration_from_start_finish() {
        let started = Utc::now() - Duration::seconds(3700);
        let finished = Utc::now();
        let updated = finished;
        let value = job_runtime_text("completed", Some(&started), Some(&finished), &updated);
        assert_eq!(value, "1h1m");
    }

    #[test]
    fn ingest_suffix_shows_phase_and_tasks_done() {
        let result = serde_json::json!({
            "files_done": 150,
            "files_total": 155,
            "chunks_embedded": 1700,
            "tasks_done": 3,
            "tasks_total": 5,
            "phase": "fetching_issues",
            "issues_fetched": 42,
            "issues_page": 2,
        });
        let raw = ingest_metrics_suffix("running", Some(&result));
        let suffix = strip_ansi(&raw);
        assert!(
            suffix.contains("fetching_issues"),
            "should show current phase: {suffix}"
        );
        assert!(
            suffix.contains("3/5"),
            "should show task progress: {suffix}"
        );
    }

    #[test]
    fn ingest_suffix_shows_embedding_issues_phase() {
        let result = serde_json::json!({
            "tasks_done": 2,
            "tasks_total": 5,
            "phase": "embedding_issues",
            "issues_total": 100,
            "chunks_embedded": 2400,
        });
        let raw = ingest_metrics_suffix("running", Some(&result));
        let suffix = strip_ansi(&raw);
        assert!(
            suffix.contains("embedding_issues"),
            "should show phase: {suffix}"
        );
    }

    #[test]
    fn ingest_suffix_backward_compatible_files_only() {
        // Old-style result_json with just files and chunks — no phase, no tasks
        let result = serde_json::json!({
            "files_done": 10,
            "files_total": 20,
            "chunks_embedded": 500,
        });
        let raw = ingest_metrics_suffix("running", Some(&result));
        let suffix = strip_ansi(&raw);
        assert!(
            suffix.contains("10/20"),
            "should show file progress: {suffix}"
        );
        assert!(suffix.contains("500"), "should show chunk count: {suffix}");
    }

    #[test]
    fn ingest_suffix_phase_specific_detail_fetching_issues() {
        let result = serde_json::json!({
            "tasks_done": 1,
            "tasks_total": 4,
            "phase": "fetching_issues",
            "issues_fetched": 42,
            "issues_page": 2,
        });
        let raw = ingest_metrics_suffix("running", Some(&result));
        let suffix = strip_ansi(&raw);
        assert!(
            suffix.contains("42"),
            "should show issues fetched count: {suffix}"
        );
        assert!(
            suffix.contains("page 2"),
            "should show page number: {suffix}"
        );
    }
}
