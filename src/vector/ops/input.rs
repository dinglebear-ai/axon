pub mod classify;
pub mod code;
pub mod select;

use crate::core::http::normalize_url;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use text_splitter::{ChunkConfig, MarkdownSplitter};

/// Overlap in characters shared between adjacent chunks.
pub const CHUNK_OVERLAP: usize = 200;
const MARKDOWN_CHUNK_MAX: usize = 2000;

#[must_use]
pub fn chunk_text(text: &str) -> Vec<String> {
    chunk_text_with_offsets(text)
        .into_iter()
        .map(|(_, chunk)| chunk)
        .collect()
}

/// Like [`chunk_text`], but each chunk carries its byte offset into `text`.
///
/// Callers that need source positions (line numbers, `#L` fragments) must use
/// the offsets returned here — re-discovering a chunk's position by substring
/// search locks onto the first occurrence and mislabels files with repeated
/// content.
#[must_use]
pub fn chunk_text_with_offsets(text: &str) -> Vec<(usize, String)> {
    use std::collections::VecDeque;

    // Step size between consecutive chunk starts (non-overlapping advance).
    const STEP: usize = MARKDOWN_CHUNK_MAX - CHUNK_OVERLAP;

    // Ring buffer of byte positions — holds at most MARKDOWN_CHUNK_MAX+1 entries
    // (one entry per char in the current [start .. start+MARKDOWN_CHUNK_MAX] window).
    // This avoids materialising a full Vec<usize> of every char index in the text.
    let mut ring: VecDeque<usize> = VecDeque::with_capacity(MARKDOWN_CHUNK_MAX + 2);
    let mut char_iter = text.char_indices();

    // Pre-fill with up to MARKDOWN_CHUNK_MAX+1 byte positions.
    for _ in 0..=MARKDOWN_CHUNK_MAX {
        match char_iter.next() {
            Some((pos, _)) => ring.push_back(pos),
            None => break,
        }
    }

    // Fast-path: the whole text fits in a single chunk.
    if ring.len() <= MARKDOWN_CHUNK_MAX {
        return vec![(0, text.to_string())];
    }

    let mut out = Vec::new();

    loop {
        let byte_start = *ring.front().expect("ring non-empty");
        let byte_end = if ring.len() > MARKDOWN_CHUNK_MAX {
            // Full window available: byte_end is the start of char at position
            // MARKDOWN_CHUNK_MAX (exclusive slice end).
            ring[MARKDOWN_CHUNK_MAX]
        } else {
            // Partial tail chunk: include through end of string.
            text.len()
        };

        out.push((byte_start, text[byte_start..byte_end].to_string()));

        if ring.len() <= MARKDOWN_CHUNK_MAX {
            break; // just emitted the last chunk
        }

        // Advance by STEP chars: drain the leading STEP positions, then refill.
        for _ in 0..STEP {
            ring.pop_front();
        }
        while ring.len() <= MARKDOWN_CHUNK_MAX {
            match char_iter.next() {
                Some((pos, _)) => ring.push_back(pos),
                None => break,
            }
        }
    }

    out
}

#[must_use]
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
    chunk_markdown_with_offsets(text)
        .into_iter()
        .map(|(_, _, chunk)| chunk)
        .collect()
}

/// Like [`chunk_markdown`], but each chunk carries the original source byte
/// range used to produce it. The rendered chunk may include repeated heading
/// context, so callers should use the returned byte range for line accounting.
#[must_use]
pub fn chunk_markdown_with_offsets(text: &str) -> Vec<(usize, usize, String)> {
    let config = ChunkConfig::new(500..MARKDOWN_CHUNK_MAX)
        .with_overlap(CHUNK_OVERLAP)
        .expect("CHUNK_OVERLAP < max chunk size");
    let headings = markdown_heading_index(text);
    MarkdownSplitter::new(config)
        .chunk_indices(text)
        .map(|(offset, chunk)| {
            let rendered = chunk_with_heading_context(chunk, active_headings(&headings, offset));
            (offset, offset + chunk.len(), rendered)
        })
        .filter(|(_, _, c)| !c.trim().is_empty())
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
    let trimmed_chunk = chunk.trim_start();
    let breadcrumb = headings.join("\n");
    if trimmed_chunk.starts_with(&breadcrumb) {
        return chunk.to_string();
    }
    let chunk_chars = trimmed_chunk.chars().count();
    if chunk_chars >= MARKDOWN_CHUNK_MAX {
        return take_chars(trimmed_chunk, MARKDOWN_CHUNK_MAX);
    }
    let breadcrumb_budget = MARKDOWN_CHUNK_MAX - chunk_chars;
    let breadcrumb = take_chars(&format!("{breadcrumb}\n\n"), breadcrumb_budget);
    format!("{breadcrumb}{trimmed_chunk}")
}

fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[must_use]
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
