use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "markdown";

pub fn heading_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let mut heading_path: Vec<String> = Vec::new();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let Some((level, heading)) = parse_atx_heading(line) else {
            continue;
        };
        heading_path.truncate(level.saturating_sub(1));
        heading_path.push(heading.to_string());
        let anchor = heading_anchor(heading);
        let line_no = idx as u32 + 1;

        facts.push(source_fact(
            input,
            "markdown_headings",
            "markdown_line_scan",
            "markdown_heading",
            heading,
            json!({
                "level": level,
                "anchor": anchor,
                "heading_path": heading_path,
            }),
            Some(line_no),
        ));
        candidates.push(graph_candidate(
            input,
            "markdown_headings",
            "markdown_heading",
            heading,
            Some(line_no),
            Some(line.trim().to_string()),
        ));
    }

    (facts, candidates)
}

fn parse_atx_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = trimmed[level..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((level, rest.trim_end_matches('#').trim()))
}

fn heading_anchor(heading: &str) -> String {
    let mut anchor = String::new();
    let mut last_dash = false;
    for ch in heading.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            anchor.push(ch);
            last_dash = false;
        } else if !last_dash && !anchor.is_empty() {
            anchor.push('-');
            last_dash = true;
        }
    }
    if last_dash {
        anchor.pop();
    }
    anchor
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
