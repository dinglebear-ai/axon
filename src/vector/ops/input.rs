pub mod classify;
pub mod code;

use crate::core::http::normalize_url;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use text_splitter::{ChunkConfig, MarkdownSplitter};

/// Overlap in characters shared between adjacent chunks.
pub const CHUNK_OVERLAP: usize = 200;
const MARKDOWN_CHUNK_MAX: usize = 2000;

pub fn chunk_text(text: &str) -> Vec<String> {
    // Fast-path: avoid the 800 KB Vec<usize> allocation for short documents.
    if text.chars().count() <= MARKDOWN_CHUNK_MAX {
        return vec![text.to_string()];
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_count = offsets.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < char_count {
        let end = (i + MARKDOWN_CHUNK_MAX).min(char_count);
        let byte_start = offsets[i];
        let byte_end = if end < char_count {
            offsets[end]
        } else {
            text.len()
        };
        out.push(text[byte_start..byte_end].to_string());
        if end == char_count {
            break;
        }
        i = end.saturating_sub(CHUNK_OVERLAP);
    }
    out
}

/// Split markdown content at structural boundaries (headers, paragraphs).
///
/// Uses `MarkdownSplitter` from the `text_splitter` crate. Chunks target
/// 500–2000 characters, splitting on `##`/`###` headers and `\n\n` paragraph
/// breaks before falling back to sentence or word boundaries. Empty and
/// whitespace-only chunks are filtered.
///
/// Use this for all markdown content (web crawl, GitHub READMEs, wikis).
/// Use `chunk_text()` for plain text (Reddit posts, YouTube transcripts).
pub fn chunk_markdown(text: &str) -> Vec<String> {
    let config = ChunkConfig::new(500..MARKDOWN_CHUNK_MAX)
        .with_overlap(CHUNK_OVERLAP)
        .expect("CHUNK_OVERLAP < max chunk size");
    let headings = markdown_heading_index(text);
    MarkdownSplitter::new(config)
        .chunk_indices(text)
        .map(|(offset, chunk)| {
            chunk_with_heading_context(chunk, active_headings(&headings, offset))
        })
        .filter(|c| !c.trim().is_empty())
        .collect()
}

#[derive(Debug)]
struct MarkdownHeading {
    offset: usize,
    level: usize,
    line: String,
}

fn markdown_heading_index(text: &str) -> Vec<MarkdownHeading> {
    let mut headings = Vec::new();
    let mut current: Option<(usize, usize, String)> = None;
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((range.start, heading_level(level), String::new()));
            }
            Event::Text(value) | Event::Code(value) => {
                if let Some((_, _, title)) = current.as_mut() {
                    title.push_str(&value);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((offset, level, title)) = current.take() {
                    let title = title.trim();
                    if !title.is_empty() {
                        headings.push(MarkdownHeading {
                            offset,
                            level,
                            line: format!("{} {}", "#".repeat(level), title),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    headings
}

fn heading_level(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn active_headings(headings: &[MarkdownHeading], offset: usize) -> Vec<&str> {
    let mut stack: Vec<&str> = Vec::new();
    for heading in headings
        .iter()
        .take_while(|heading| heading.offset <= offset)
    {
        let idx = heading.level.saturating_sub(1);
        stack.truncate(idx);
        stack.push(heading.line.as_str());
    }
    stack
}

fn chunk_with_heading_context(chunk: &str, headings: Vec<&str>) -> String {
    if headings.is_empty() {
        return chunk.to_string();
    }
    let breadcrumb = format!("{}\n\n", headings.join("\n"));
    if chunk.trim_start().starts_with(breadcrumb.trim_end()) {
        return chunk.to_string();
    }
    let body_budget = MARKDOWN_CHUNK_MAX.saturating_sub(breadcrumb.chars().count());
    if body_budget == 0 {
        return take_chars(&breadcrumb, MARKDOWN_CHUNK_MAX);
    }
    format!(
        "{breadcrumb}{}",
        take_chars(chunk.trim_start(), body_budget)
    )
}

fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub fn url_lookup_candidates(target: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let normalized = normalize_url(target);
    let variants = [
        target.to_string(),
        normalized.to_string(),
        normalized.trim_end_matches('/').to_string(),
        format!("{}/", normalized.trim_end_matches('/')),
    ];
    for variant in variants {
        if variant.is_empty() {
            continue;
        }
        if seen.insert(variant.clone()) {
            out.push(variant);
        }
    }
    out
}

#[cfg(test)]
#[path = "input_proptest_tests.rs"]
mod input_proptest;

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
