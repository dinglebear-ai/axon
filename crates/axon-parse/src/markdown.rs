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

    let mut fence: Option<(u8, usize)> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let trimmed = &line[indent..];
        if let Some((marker, length)) = fence {
            let run = trimmed.bytes().take_while(|byte| *byte == marker).count();
            if indent <= 3 && run >= length && trimmed[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if indent <= 3 {
            let marker = trimmed.as_bytes().first().copied().unwrap_or_default();
            let run = trimmed.bytes().take_while(|byte| *byte == marker).count();
            if matches!(marker, b'`' | b'~')
                && run >= 3
                && (marker != b'`' || !trimmed[run..].contains('`'))
            {
                fence = Some((marker, run));
                continue;
            }
        }
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
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let trimmed = &line[indent..];
    let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_matches([' ', '\t']);
    // Closing hashes are decoration only when preceded by whitespace.
    let without_hashes = rest.trim_end_matches('#');
    let heading = if without_hashes.is_empty() || without_hashes.ends_with([' ', '\t']) {
        without_hashes.trim_end_matches([' ', '\t'])
    } else {
        rest
    };
    // Empty headings have no meaningful entity to put in the graph.
    (!heading.is_empty()).then_some((level, heading))
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
