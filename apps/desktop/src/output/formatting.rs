use crate::actions::ACTIONS;

use super::{OUTPUT_LIMIT, TRUNCATED_MESSAGE};

#[path = "formatting_rest.rs"]
mod formatting_rest;
#[cfg(test)]
#[path = "formatting_test_helpers.rs"]
mod formatting_test_helpers;
#[cfg(test)]
pub(super) use formatting_test_helpers::{actionable_error_text, format_exit_status, strip_ansi};

pub(super) fn palette_output_text(subcommand: &str, text: &str) -> String {
    match subcommand {
        "ask" | "chat" => ask_answer(text),
        "crawl" => crawl_summary(text),
        "embed" | "extract" | "ingest" => job_summary(subcommand, text),
        "map" => map_url_listing(text),
        "scrape" => scrape_body(text),
        "search" => search_results(text),
        _ => drop_cli_scaffolding(text),
    }
}

pub(super) fn rest_output_text(subcommand: &str, text: &str) -> String {
    formatting_rest::rest_output_text(subcommand, text)
        .unwrap_or_else(|| palette_output_text(subcommand, text))
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
