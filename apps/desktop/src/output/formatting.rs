use crate::actions::ACTIONS;

use super::{OUTPUT_LIMIT, TRUNCATED_MESSAGE};

const SUMMARY_LIMIT: usize = 10;

pub(super) fn palette_output_text(subcommand: &str, text: &str) -> String {
    match subcommand {
        "ask" => ask_answer(text),
        "crawl" => crawl_summary(text),
        "embed" | "extract" | "ingest" => job_summary(subcommand, text),
        "map" => map_url_listing(text),
        "scrape" => scrape_body(text),
        "search" => search_results(text),
        _ => drop_cli_scaffolding(text),
    }
}

pub(super) fn rest_output_text(subcommand: &str, text: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return palette_output_text(subcommand, text);
    };

    match subcommand {
        "ask" => string_field(&value, "answer").unwrap_or_else(|| compact_json(&value)),
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
        "crawl" | "embed" | "extract" | "ingest" => job_start_result(subcommand, &value),
        "sources" => sources_result(&value),
        "domains" => domains_result(&value),
        "stats" => stats_result(&value),
        "doctor" => doctor_result(&value),
        "status" => status_result(&value),
        _ => compact_json(&value),
    }
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

pub(super) fn drop_cli_scaffolding(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !is_cli_scaffolding_line(line))
        .collect();
    let cleaned = trim_blank_lines(&lines).join("\n");
    if cleaned.is_empty() {
        text.to_string()
    } else {
        cleaned
    }
}

fn is_cli_scaffolding_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed == "Options:"
        || trimmed == "Overrides"
        || trimmed == "Follow progress"
        || trimmed == "Jobs"
        || trimmed == "Conversation"
        || trimmed == "Assistant:"
        || trimmed == "Ask Explain"
        || trimmed.starts_with("As of:")
        || trimmed.starts_with("Showing ")
        || trimmed.starts_with("Found ")
        || trimmed.starts_with("Timing:")
        || trimmed.starts_with("Session:")
        || trimmed.starts_with("Trace:")
        || trimmed.starts_with("Hint:")
        || trimmed.starts_with("Strategy ")
        || trimmed.starts_with("Scope ")
        || trimmed.starts_with("Pipeline ")
        || trimmed.starts_with("Runtime ")
        || trimmed.starts_with("axon ")
        || trimmed.starts_with("◐ Mapping ")
        || trimmed.starts_with("◐ Scraping ")
}

fn trim_blank_lines<'a>(lines: &'a [&'a str]) -> &'a [&'a str] {
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let end = lines.iter().rposition(|line| !line.trim().is_empty());
    match (start, end) {
        (Some(start), Some(end)) => &lines[start..=end],
        _ => &[],
    }
}

pub(super) fn scrape_body(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut start = 0;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("Scrape Results for ") || trimmed == "As of: now" {
            start = idx + 1;
        }
    }
    let body = trim_blank_lines(&lines[start..]).join("\n");
    if body.is_empty() {
        drop_cli_scaffolding(text)
    } else {
        body
    }
}

pub(super) fn ask_answer(text: &str) -> String {
    let mut answer = Vec::new();
    let mut in_answer = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "Assistant:" || trimmed.ends_with(" Assistant:") {
            in_answer = true;
            continue;
        }
        if in_answer && (trimmed.starts_with("Timing:") || trimmed.starts_with("Session:")) {
            break;
        }
        if in_answer {
            answer.push(line);
        }
    }
    let answer = trim_blank_lines(&answer).join("\n");
    if answer.is_empty() {
        drop_cli_scaffolding(text)
    } else {
        answer
    }
}

pub(super) fn crawl_summary(text: &str) -> String {
    let mut lines = Vec::new();
    let mut saw_job = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Crawl queued") || trimmed.contains("Crawl completed") {
            lines.push(clean_status_symbol(trimmed).to_string());
        } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            lines.push(trimmed.to_string());
        } else if let Some(value) = compact_labeled_value(trimmed, "Job") {
            saw_job = true;
            lines.push(format!("Job {value}"));
        } else if let Some(value) = trimmed.strip_prefix("Job:") {
            saw_job = true;
            lines.push(format!("Job {}", value.trim()));
        } else if trimmed.starts_with("Job ID:") {
            saw_job = true;
            lines.push(trimmed.to_string());
        }
    }
    if lines.is_empty() {
        drop_cli_scaffolding(text)
    } else {
        if saw_job {
            lines.push("Next: axon status".to_string());
        }
        lines.join("\n")
    }
}

fn job_summary(subcommand: &str, text: &str) -> String {
    let mut lines = Vec::new();
    let mut saw_job = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("queued") || trimmed.contains("completed") {
            lines.push(clean_status_symbol(trimmed).to_string());
        } else if trimmed.starts_with("Input:")
            || trimmed.starts_with("Target:")
            || trimmed.starts_with("Source:")
            || trimmed.starts_with("Status:")
            || trimmed.starts_with("Collection:")
            || trimmed.starts_with("Job ID:")
        {
            if trimmed.starts_with("Job ID:") {
                saw_job = true;
            }
            lines.push(trimmed.to_string());
        } else if let Some(value) = compact_labeled_value(trimmed, "Job") {
            saw_job = true;
            lines.push(format!("Job {value}"));
        }
    }
    if lines.is_empty() {
        drop_cli_scaffolding(text)
    } else {
        if subcommand == "ingest" && saw_job {
            lines.push("Next: axon status".to_string());
        }
        lines.join("\n")
    }
}

fn search_results(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("Search Results for ") && !trimmed.starts_with("Found ")
        })
        .collect();
    let cleaned = trim_blank_lines(&lines).join("\n");
    if cleaned.is_empty() {
        drop_cli_scaffolding(text)
    } else {
        cleaned
    }
}

fn clean_status_symbol(text: &str) -> &str {
    text.trim_start_matches(|ch: char| {
        ch == '●' || ch == '✓' || ch == '✔' || ch == '◐' || ch.is_whitespace()
    })
}

fn compact_labeled_value<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let value = line.strip_prefix(label)?.trim_start();
    if value.is_empty() { None } else { Some(value) }
}

pub(super) fn map_url_listing(text: &str) -> String {
    let urls: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix('•')
                .or_else(|| trimmed.strip_prefix("- "))
                .map(str::trim)
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        })
        .collect();

    if urls.is_empty() {
        text.to_string()
    } else {
        urls.join("\n")
    }
}

#[cfg(test)]
pub(super) fn actionable_error_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with("Error:"))
    {
        return lines[index..].join("\n");
    }

    let non_log_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.contains(" WARN ")
                || trimmed.contains(" INFO ")
                || trimmed.contains(" DEBUG ")
                || trimmed.contains(" TRACE "))
        })
        .collect();

    if non_log_lines.is_empty() {
        text.to_string()
    } else {
        non_log_lines.join("\n")
    }
}

/// Strip ANSI / VT escape sequences. CSI, OSC, DCS, APC, PM, and SOS are
/// covered; malformed sequences are silently dropped.
#[cfg(test)]
pub(super) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        let Some(&next) = chars.peek() else {
            continue;
        };
        match next {
            '[' => {
                chars.next();
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        break;
                    }
                }
            }
            ']' => {
                chars.next();
                consume_until_string_terminator(&mut chars, true);
            }
            'P' | '_' | '^' | 'X' => {
                chars.next();
                consume_until_string_terminator(&mut chars, false);
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
fn consume_until_string_terminator(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    allow_bel: bool,
) {
    while let Some(ch) = chars.next() {
        if allow_bel && ch == '\x07' {
            return;
        }
        if ch == '\x1b' {
            if chars.peek() == Some(&'\\') {
                chars.next();
                return;
            }
            continue;
        }
    }
}

#[cfg(test)]
pub(super) fn format_exit_status(status: &std::process::ExitStatus) -> String {
    if status.success() {
        return "ok".to_string();
    }
    if let Some(code) = status.code() {
        return format!("exit {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            let name = signal_name(sig).unwrap_or("signal");
            return format!("killed by {name} ({sig})");
        }
    }
    format!("{status}")
}

#[cfg(unix)]
#[cfg(test)]
fn signal_name(sig: i32) -> Option<&'static str> {
    match sig {
        1 => Some("SIGHUP"),
        2 => Some("SIGINT"),
        3 => Some("SIGQUIT"),
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        13 => Some("SIGPIPE"),
        14 => Some("SIGALRM"),
        15 => Some("SIGTERM"),
        _ => None,
    }
}

pub(super) fn command_title(subcommand: &str) -> &'static str {
    ACTIONS
        .iter()
        .find(|action| action.subcommand == subcommand)
        .map(|action| action.label)
        .unwrap_or("Command")
}

pub(super) fn truncate_output(mut text: String) -> String {
    if text.len() <= OUTPUT_LIMIT {
        return text;
    }

    let boundary = text.floor_char_boundary(OUTPUT_LIMIT);
    text.truncate(boundary);
    text.push_str(TRUNCATED_MESSAGE);
    text
}
