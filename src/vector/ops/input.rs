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
const MARKDOWN_CHUNK_MIN: usize = 500;

fn env_usize_clamped(key: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}

fn markdown_chunk_max_chars() -> usize {
    env_usize_clamped(
        "AXON_MARKDOWN_CHUNK_MAX_CHARS",
        MARKDOWN_CHUNK_MAX,
        256,
        16_384,
    )
}

fn markdown_chunk_min_chars(max_chars: usize) -> usize {
    env_usize_clamped(
        "AXON_MARKDOWN_CHUNK_MIN_CHARS",
        MARKDOWN_CHUNK_MIN.min(max_chars),
        1,
        max_chars,
    )
}

fn chunk_overlap_chars(max_chars: usize) -> usize {
    env_usize_clamped(
        "AXON_CHUNK_OVERLAP_CHARS",
        CHUNK_OVERLAP.min(max_chars.saturating_sub(1)),
        0,
        max_chars.saturating_sub(1),
    )
}

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

    let chunk_max = markdown_chunk_max_chars();
    let overlap = chunk_overlap_chars(chunk_max);
    // Step size between consecutive chunk starts (non-overlapping advance).
    let step = chunk_max - overlap;

    // Ring buffer of byte positions — holds at most MARKDOWN_CHUNK_MAX+1 entries
    // (one entry per char in the current [start .. start+MARKDOWN_CHUNK_MAX] window).
    // This avoids materialising a full Vec<usize> of every char index in the text.
    let mut ring: VecDeque<usize> = VecDeque::with_capacity(chunk_max + 2);
    let mut char_iter = text.char_indices();

    // Pre-fill with up to MARKDOWN_CHUNK_MAX+1 byte positions.
    for _ in 0..=chunk_max {
        match char_iter.next() {
            Some((pos, _)) => ring.push_back(pos),
            None => break,
        }
    }

    // Fast-path: the whole text fits in a single chunk.
    if ring.len() <= chunk_max {
        return vec![(0, text.to_string())];
    }

    let mut out = Vec::new();

    loop {
        let byte_start = *ring.front().expect("ring non-empty");
        let byte_end = if ring.len() > chunk_max {
            // Full window available: byte_end is the start of char at position
            // MARKDOWN_CHUNK_MAX (exclusive slice end).
            ring[chunk_max]
        } else {
            // Partial tail chunk: include through end of string.
            text.len()
        };

        out.push((byte_start, text[byte_start..byte_end].to_string()));

        if ring.len() <= chunk_max {
            break; // just emitted the last chunk
        }

        // Advance by STEP chars: drain the leading STEP positions, then refill.
        for _ in 0..step {
            ring.pop_front();
        }
        while ring.len() <= chunk_max {
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
    let chunk_max = markdown_chunk_max_chars();
    let config = ChunkConfig::new(markdown_chunk_min_chars(chunk_max)..chunk_max)
        .with_overlap(chunk_overlap_chars(chunk_max))
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
    let normalized_chunk = normalize_scraped_markdown_artifacts(chunk);
    if headings.is_empty() {
        return normalized_chunk;
    }
    let trimmed_chunk = normalized_chunk.trim_start();
    let headings = heading_context_to_add(trimmed_chunk, headings);
    if headings.is_empty() {
        return normalized_chunk;
    }
    let breadcrumb = headings.join("\n");
    if trimmed_chunk.starts_with(&breadcrumb) {
        return normalized_chunk;
    }
    let chunk_max = markdown_chunk_max_chars();
    let chunk_chars = trimmed_chunk.chars().count();
    if chunk_chars >= chunk_max {
        return take_chars(trimmed_chunk, chunk_max);
    }
    let breadcrumb_budget = chunk_max - chunk_chars;
    let breadcrumb = take_chars(&format!("{breadcrumb}\n\n"), breadcrumb_budget);
    format!("{breadcrumb}{trimmed_chunk}")
}

fn heading_context_to_add<'a>(chunk: &str, headings: Vec<&'a str>) -> Vec<&'a str> {
    let leading = leading_atx_headings(chunk);
    if leading.is_empty() {
        return headings;
    }

    let duplicate_keys = leading
        .iter()
        .map(|(_, key)| key.as_str())
        .collect::<HashSet<_>>();
    let first_chunk_heading_level = leading[0].0;

    headings
        .into_iter()
        .filter(|heading| {
            let Some((level, key)) = atx_heading_key(heading) else {
                return true;
            };
            level < first_chunk_heading_level && !duplicate_keys.contains(key.as_str())
        })
        .collect()
}

fn leading_atx_headings(chunk: &str) -> Vec<(usize, String)> {
    let mut headings = Vec::new();
    for line in chunk.trim_start().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(heading) = atx_heading_key(line) else {
            break;
        };
        headings.push(heading);
    }
    headings
}

fn atx_heading_key(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = trimmed[level..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let title = rest.trim_end_matches('#').trim();
    if title.is_empty() {
        return None;
    }
    Some((level, title.to_ascii_lowercase()))
}

fn normalize_scraped_markdown_artifacts(chunk: &str) -> String {
    let chunk = normalize_empty_anchor_headings(chunk);
    normalize_scraped_code_fence_artifacts(&chunk)
}

fn normalize_empty_anchor_headings(chunk: &str) -> String {
    let lines = chunk.lines().map(str::to_string).collect::<Vec<_>>();
    let mut out = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let marker = lines[i].trim();
        let level = marker.chars().take_while(|c| *c == '#').count();
        let is_empty_heading = (1..=6).contains(&level) && marker[level..].trim().is_empty();
        if !is_empty_heading {
            out.push(lines[i].clone());
            i += 1;
            continue;
        }

        if let Some((skip, title)) = empty_anchor_heading_title(&lines, i) {
            out.push(format!("{} {}", "#".repeat(level), title));
            changed = true;
            i += skip;
        } else {
            out.push(lines[i].clone());
            i += 1;
        }
    }

    if changed {
        let mut rendered = out.join("\n");
        if chunk.ends_with('\n') {
            rendered.push('\n');
        }
        rendered
    } else {
        chunk.to_string()
    }
}

fn empty_anchor_heading_title(lines: &[String], heading_idx: usize) -> Option<(usize, String)> {
    let anchor = lines.get(heading_idx + 1)?.trim();
    if anchor.starts_with("[\u{200b}](") && anchor.ends_with(')') {
        let title = lines.get(heading_idx + 2)?.trim();
        return valid_anchor_heading_title(title).map(|title| (3, title));
    }

    if anchor.starts_with("[\u{200b}") {
        let anchor_close = lines.get(heading_idx + 2)?.trim();
        if anchor_close.starts_with("](") && anchor_close.ends_with(')') {
            let title = lines.get(heading_idx + 3)?.trim();
            return valid_anchor_heading_title(title).map(|title| (4, title));
        }
    }

    None
}

fn valid_anchor_heading_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() || title.starts_with('#') || title.starts_with('[') {
        None
    } else {
        Some(title.to_string())
    }
}

fn normalize_scraped_code_fence_artifacts(chunk: &str) -> String {
    let mut lines = chunk.lines().map(str::to_string).collect::<Vec<_>>();
    let mut changed = false;
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i].trim() != "```" {
            i += 1;
            continue;
        }

        if unwrap_single_backtick_code_block(&mut lines, i) {
            changed = true;
        }

        let inner_open = lines[i + 1].trim();
        if !inner_open.starts_with("```") || inner_open == "```" {
            i += 1;
            continue;
        }

        let Some(inner_close) = lines
            .iter()
            .enumerate()
            .skip(i + 2)
            .find_map(|(idx, line)| (line.trim() == "```").then_some(idx))
        else {
            i += 1;
            continue;
        };

        if inner_close + 1 < lines.len() && lines[inner_close + 1].trim() == "```" {
            lines[i] = "````".to_string();
            lines[inner_close + 1] = "````".to_string();
            changed = true;
            i = inner_close + 2;
        } else {
            i += 1;
        }
    }

    if changed {
        let mut out = lines.join("\n");
        if chunk.ends_with('\n') {
            out.push('\n');
        }
        out
    } else {
        chunk.to_string()
    }
}

fn unwrap_single_backtick_code_block(lines: &mut Vec<String>, fence_idx: usize) -> bool {
    let Some(first_code) = lines.get(fence_idx + 1).map(|line| line.trim_start()) else {
        return false;
    };
    if !first_code.starts_with('`') || first_code.starts_with("```") {
        return false;
    }

    let Some(close_idx) = lines
        .iter()
        .enumerate()
        .skip(fence_idx + 2)
        .find_map(|(idx, line)| (line.trim() == "```").then_some(idx))
    else {
        return false;
    };
    if close_idx <= fence_idx + 2 || lines[close_idx - 1].trim() != "`" {
        return false;
    }

    if let Some(line) = lines.get_mut(fence_idx + 1)
        && let Some(pos) = line.find('`')
    {
        line.remove(pos);
    }
    lines.remove(close_idx - 1);
    true
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
