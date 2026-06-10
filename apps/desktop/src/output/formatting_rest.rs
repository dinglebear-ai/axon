#[cfg(test)]
#[path = "formatting_rest_tests.rs"]
mod tests;

const SUMMARY_LIMIT: usize = 10;

pub(super) fn rest_output_text(subcommand: &str, text: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return None;
    };

    Some(match subcommand {
        "ask" | "chat" => string_field(&value, "answer").unwrap_or_else(|| compact_json(&value)),
        "scrape" => string_field(&value, "markdown")
            .or_else(|| string_field(&value, "output"))
            .unwrap_or_else(|| compact_json(&value)),
        "retrieve" => string_field(&value, "content").unwrap_or_else(|| compact_json(&value)),
        "summarize" => summarize_result(&value),
        "research" => research_result(&value),
        "query" => query_result(&value),
        "search" => search_result(&value),
        "map" => map_result(&value),
        "suggest" => suggestions_result(&value),
        "evaluate" => evaluate_result(&value),
        "screenshot" => screenshot_result(&value),
        "crawl" | "embed" | "extract" | "ingest" => job_start_result(subcommand, &value),
        "sources" => sources_result(&value),
        "domains" => domains_result(&value),
        "stats" => stats_result(&value),
        "doctor" => doctor_result(&value),
        "status" => status_result(&value),
        _ => compact_json(&value),
    })
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToString::to_string)
}

fn array_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a Vec<serde_json::Value>> {
    value.get(key)?.as_array()
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn summarize_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(summary) = string_field(value, "summary") {
        lines.push(summary);
    }
    if let Some(documents) = array_field(value, "documents").filter(|docs| !docs.is_empty()) {
        lines.push(String::new());
        lines.push("Sources".to_string());
        for doc in documents.iter().take(SUMMARY_LIMIT) {
            if let Some(url) = doc.get("url").and_then(|v| v.as_str()) {
                let chars = doc
                    .get("content_chars")
                    .and_then(|v| v.as_u64())
                    .map(|count| format!(" ({count} chars)"))
                    .unwrap_or_default();
                lines.push(format!("{url}{chars}"));
            }
        }
    }
    non_empty_or_compact(lines, value)
}

fn research_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(summary) = string_field(value, "summary") {
        lines.push(summary);
    }
    if let Some(results) = array_field(value, "search_results").filter(|rows| !rows.is_empty()) {
        lines.push(String::new());
        lines.push("Results".to_string());
        for result in results.iter().take(SUMMARY_LIMIT) {
            let title = result
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                lines.push(title.to_string());
            } else {
                lines.push(format!("{title}\n{url}"));
            }
        }
    }
    non_empty_or_compact(lines, value)
}

fn query_result(value: &serde_json::Value) -> String {
    let Some(results) = array_field(value, "results") else {
        return compact_json(value);
    };
    if results.is_empty() {
        return "No query results.".to_string();
    }
    results
        .iter()
        .take(SUMMARY_LIMIT)
        .map(|hit| {
            let rank = hit.get("rank").and_then(|v| v.as_u64()).unwrap_or(0);
            let score = hit
                .get("score")
                .and_then(|v| v.as_f64())
                .map(|score| format!("{score:.3}"))
                .unwrap_or_else(|| "?".to_string());
            let url = hit.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = hit.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            format!("{rank}. score {score}\n{url}\n{}", snippet.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn search_result(value: &serde_json::Value) -> String {
    let Some(results) = array_field(value, "results") else {
        return compact_json(value);
    };
    if results.is_empty() {
        return "No search results.".to_string();
    }
    results
        .iter()
        .take(SUMMARY_LIMIT)
        .enumerate()
        .map(|(idx, result)| {
            let title = result
                .get("title")
                .or_else(|| result.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = result
                .get("snippet")
                .or_else(|| result.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{}. {title}\n{url}\n{}", idx + 1, snippet.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn map_result(value: &serde_json::Value) -> String {
    let Some(urls) = array_field(value, "urls") else {
        return compact_json(value);
    };
    if urls.is_empty() {
        return "No URLs discovered.".to_string();
    }
    urls.iter()
        .take(100)
        .filter_map(|url| url.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn suggestions_result(value: &serde_json::Value) -> String {
    let Some(suggestions) = array_field(value, "suggestions") else {
        return compact_json(value);
    };
    if suggestions.is_empty() {
        return "No crawl suggestions.".to_string();
    }
    suggestions
        .iter()
        .take(SUMMARY_LIMIT)
        .map(|suggestion| {
            let url = suggestion.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let reason = suggestion
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{url}\n{reason}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn evaluate_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(query) = string_field(value, "query") {
        lines.push(format!("Question\n{query}"));
    }
    if let Some(analysis) = string_field(value, "analysis_answer") {
        lines.push(format!("Judge\n{}", analysis.trim()));
    }
    if let Some(answer) = string_field(value, "rag_answer") {
        lines.push(format!("RAG\n{}", answer.trim()));
    }
    if let Some(answer) = string_field(value, "baseline_answer") {
        lines.push(format!("Baseline\n{}", answer.trim()));
    }
    if let Some(urls) = array_field(value, "source_urls").filter(|urls| !urls.is_empty()) {
        let rendered = urls
            .iter()
            .take(SUMMARY_LIMIT)
            .filter_map(|url| url.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !rendered.is_empty() {
            lines.push(format!("Sources\n{rendered}"));
        }
    }
    non_empty_or_compact(lines, value)
}

fn job_start_result(subcommand: &str, value: &serde_json::Value) -> String {
    // Terminal job state from polling: { "job": { "status": "completed", ... } }
    if let Some(job) = value.get("job") {
        return job_terminal_result(subcommand, job);
    }
    // Accepted-job response: show job ID so the user can still track it manually
    let result = value.get("result").unwrap_or(value);
    let mut lines = Vec::new();
    if let Some(disposition) = string_field(value, "disposition") {
        lines.push(format!("{subcommand} {disposition}"));
    }
    if let Some(mode) = string_field(value, "execution_mode") {
        lines.push(format!("mode: {mode}"));
    }
    if let Some(job_id) = string_field(result, "job_id") {
        lines.push(format!("job: {job_id}"));
    }
    if let Some(job_ids) = array_field(result, "job_ids") {
        for job_id in job_ids.iter().filter_map(|id| id.as_str()) {
            lines.push(format!("job: {job_id}"));
        }
    }
    if let Some(jobs) = array_field(result, "jobs") {
        for job in jobs.iter().take(SUMMARY_LIMIT) {
            let job_id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
            let url = job.get("url").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!("job: {job_id}\n{url}"));
        }
    }
    if lines.iter().any(|line| line.starts_with("job: ")) {
        lines.push("Next: status".to_string());
    }
    non_empty_or_compact(lines, value)
}

/// Format a terminal job state returned by the status-polling path.
/// Shows status, key metrics from `result_json`, and the job's target URL.
fn job_terminal_result(subcommand: &str, job: &serde_json::Value) -> String {
    let status = string_field(job, "status").unwrap_or_else(|| "unknown".to_string());
    let mut lines = vec![format!("{subcommand} {status}")];
    if let Some(err) = string_field(job, "error_text") {
        lines.push(format!("error: {err}"));
    }
    if let Some(result) = job.get("result_json") {
        for key in &[
            "pages_crawled",
            "docs_embedded",
            "chunks_embedded",
            "md_created",
        ] {
            if let Some(v) = result.get(*key).and_then(|v| v.as_u64()).filter(|&n| n > 0) {
                lines.push(format!("{key}: {v}"));
            }
        }
        if let Some(ms) = result
            .get("elapsed_ms")
            .and_then(|v| v.as_u64())
            .filter(|&n| n >= 1000)
        {
            lines.push(format!("elapsed: {:.1}s", ms as f64 / 1000.0));
        }
    }
    if let Some(target) = string_field(job, "url").or_else(|| string_field(job, "target")) {
        lines.push(target);
    }
    lines.join("\n")
}

fn sources_result(value: &serde_json::Value) -> String {
    let count = value.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    let urls = array_field(value, "urls").cloned().unwrap_or_default();
    let mut lines = vec![format!("{count} indexed sources")];
    lines.extend(
        urls.iter()
            .take(SUMMARY_LIMIT)
            .filter_map(|url| url.as_str())
            .map(ToString::to_string),
    );
    lines.join("\n")
}

fn domains_result(value: &serde_json::Value) -> String {
    let Some(domains) = array_field(value, "domains") else {
        return compact_json(value);
    };
    if domains.is_empty() {
        return "No indexed domains.".to_string();
    }
    domains
        .iter()
        .take(SUMMARY_LIMIT)
        .map(|domain| {
            let name = domain
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let vectors = domain.get("vectors").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("{name}: {vectors} vectors")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn stats_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    collect_key_values(
        value,
        &mut lines,
        &["points_count", "vectors_count", "status"],
    );
    non_empty_or_compact(lines, value)
}

fn doctor_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    collect_key_values(
        value,
        &mut lines,
        &["status", "ok", "name", "message", "url"],
    );
    non_empty_or_compact(lines, value)
}

fn status_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(totals) = value.get("totals") {
        for key in ["crawl", "extract", "embed", "ingest"] {
            if let Some(count) = totals.get(key).and_then(|v| v.as_i64()) {
                lines.push(format!("{key}: {count}"));
            }
        }
    }
    collect_key_values(
        value,
        &mut lines,
        &["status", "id", "target", "url", "error_text"],
    );
    non_empty_or_compact(lines, value)
}

fn collect_key_values(value: &serde_json::Value, lines: &mut Vec<String>, keys: &[&str]) {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key).and_then(scalar_text) {
                    lines.push(format!("{key}: {value}"));
                }
            }
            for nested in map.values() {
                if lines.len() >= SUMMARY_LIMIT {
                    return;
                }
                collect_key_values(nested, lines, keys);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                if lines.len() >= SUMMARY_LIMIT {
                    return;
                }
                collect_key_values(nested, lines, keys);
            }
        }
        _ => {}
    }
}

fn scalar_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn screenshot_result(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(url) = string_field(value, "url") {
        lines.push(url);
    }
    let handle = value.get("artifact_handle");
    let relative_path = handle
        .and_then(|h| h.get("relative_path"))
        .and_then(|v| v.as_str());
    let size_bytes = handle
        .and_then(|h| h.get("bytes"))
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("size_bytes")?.as_u64());
    let mut meta = Vec::new();
    if let Some(b) = size_bytes {
        meta.push(format_bytes(b));
    }
    if let Some(path) = relative_path {
        meta.push(format!("artifact: {path}"));
    }
    if !meta.is_empty() {
        lines.push(meta.join(" · "));
    }
    non_empty_or_compact(lines, value)
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn non_empty_or_compact(lines: Vec<String>, value: &serde_json::Value) -> String {
    let text = lines
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        compact_json(value)
    } else {
        text
    }
}
