//! Markdown and HTML chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::{plain_text_windows, source_range};

pub(crate) fn markdown_sections(text: &str) -> Vec<DocumentChunk> {
    let mut starts = Vec::new();
    for (byte, line) in text.match_indices('#') {
        let at_line_start = byte == 0 || text.as_bytes().get(byte - 1) == Some(&b'\n');
        if at_line_start && line.starts_with('#') {
            starts.push(byte);
        }
    }
    if starts.first().copied() != Some(0) {
        starts.insert(0, 0);
    }
    starts.push(text.len());

    starts
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let content = text[start..end].trim();
            if content.is_empty() {
                return None;
            }
            let heading = content
                .lines()
                .next()
                .filter(|line| line.trim_start().starts_with('#'))
                .map(|line| line.trim_start_matches('#').trim().to_string());
            let mut chunk = DocumentChunk::new(content.to_string(), source_range(text, start, end));
            if let Some(heading) = heading {
                chunk = chunk
                    .with_title(heading.clone())
                    .with_heading_path(vec![heading]);
            }
            Some(chunk)
        })
        .collect()
}

pub(crate) fn html_article(text: &str) -> Vec<DocumentChunk> {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                plain.push('\n');
            }
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    plain_text_windows(&plain)
}
