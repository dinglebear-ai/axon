pub mod classify;
pub mod code;

use crate::core::http::normalize_url;
use std::collections::HashSet;
use text_splitter::{ChunkConfig, MarkdownSplitter};

/// Overlap in characters shared between adjacent chunks.
pub const CHUNK_OVERLAP: usize = 200;

pub fn chunk_text(text: &str) -> Vec<String> {
    const MAX: usize = 2000;

    // Fast-path: avoid the 800 KB Vec<usize> allocation for short documents.
    if text.chars().count() <= MAX {
        return vec![text.to_string()];
    }

    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let char_count = offsets.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < char_count {
        let end = (i + MAX).min(char_count);
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
    let config = ChunkConfig::new(500..2000)
        .with_overlap(CHUNK_OVERLAP)
        .expect("CHUNK_OVERLAP < max chunk size");
    MarkdownSplitter::new(config)
        .chunks(text)
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty())
        .collect()
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
#[path = "input_proptest.rs"]
mod input_proptest;

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
