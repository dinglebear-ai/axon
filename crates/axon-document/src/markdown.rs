//! Markdown and HTML chunk builders.
//!
//! Markdown sectioning is fence-aware (never splits inside a ` ``` `/`~~~`
//! fenced code block), carries full heading-breadcrumb context (not just the
//! section's own heading), and extracts YAML frontmatter as its own chunk
//! before sectioning the body. Contract:
//! `docs/pipeline-unification/sources/chunking-contract.md` "Markdown and
//! Docs Chunking".

use crate::chunk::DocumentChunk;
use crate::text::{plain_text_windows, source_range};

/// One ATX heading line: byte offset of its `#` run, its level (1-6), and
/// its title text.
struct Heading {
    byte: usize,
    level: usize,
    title: String,
}

pub(crate) fn markdown_sections(text: &str) -> Vec<DocumentChunk> {
    let (frontmatter, body_start) = extract_frontmatter(text);
    let mut chunks = Vec::new();
    if frontmatter.is_some() {
        chunks.push(
            DocumentChunk::new(
                text[..body_start].trim().to_string(),
                source_range(text, 0, body_start),
            )
            .with_metadata("markdown_block_kind", "frontmatter".into()),
        );
    }

    let headings = fence_aware_headings(text, body_start);
    let mut starts: Vec<usize> = headings.iter().map(|heading| heading.byte).collect();
    if starts.first().copied() != Some(body_start) {
        starts.insert(0, body_start);
    }
    starts.push(text.len());

    // Breadcrumb stack of (level, title) ancestors, updated as headings are
    // encountered in document order.
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut heading_idx = 0usize;

    for pair in starts.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let content = text[start..end].trim();
        if content.is_empty() {
            continue;
        }

        if let Some(heading) = headings.get(heading_idx).filter(|h| h.byte == start) {
            while stack
                .last()
                .is_some_and(|(level, _)| *level >= heading.level)
            {
                stack.pop();
            }
            stack.push((heading.level, heading.title.clone()));
            heading_idx += 1;
        }

        let breadcrumb: Vec<String> = stack.iter().map(|(_, title)| title.clone()).collect();
        let mut chunk = DocumentChunk::new(content.to_string(), source_range(text, start, end))
            .with_metadata("markdown_block_kind", "section".into());
        if let Some((level, title)) = stack.last() {
            chunk = chunk
                .with_title(title.clone())
                .with_heading_path(breadcrumb)
                .with_metadata("section_level", (*level as u32).into());
        }
        if let Some(language) = first_fence_language(content) {
            chunk = chunk.with_metadata("code_fence_language", language.into());
        }
        chunks.push(chunk);
    }

    chunks
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

/// Extracts a leading `---`-delimited YAML frontmatter block, if present.
/// Returns whether frontmatter was found and the byte offset where the
/// document body starts.
fn extract_frontmatter(text: &str) -> (Option<()>, usize) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, 0);
    };
    let Some(close) = rest.find("\n---") else {
        return (None, 0);
    };
    let after_delim = close + "\n---".len();
    let tail = &rest[after_delim..];
    let consumed = tail
        .find('\n')
        .map(|nl| after_delim + nl + 1)
        .unwrap_or(rest.len());
    (Some(()), 4 + consumed)
}

/// Byte offsets/levels/titles of ATX headings (`#`..`######`) that are not
/// inside a fenced code block, starting the scan at `from`.
fn fence_aware_headings(text: &str, from: usize) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut offset = from;
    for line in text[from..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let stripped = trimmed.trim_start();
        if is_fence_delimiter(stripped) {
            in_fence = !in_fence;
        } else if !in_fence {
            if let Some(level) = atx_heading_level(stripped) {
                let title = stripped
                    .trim_start_matches('#')
                    .trim()
                    .trim_end_matches('#')
                    .trim()
                    .to_string();
                headings.push(Heading {
                    byte: offset,
                    level,
                    title,
                });
            }
        }
        offset += line.len();
    }
    headings
}

fn is_fence_delimiter(line: &str) -> bool {
    (line.starts_with("```") || line.starts_with("~~~")) && !line.trim().is_empty()
}

fn atx_heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')).then_some(hashes)
}

/// First fenced code block's language label within `content`, if any.
fn first_fence_language(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(lang) = trimmed
            .strip_prefix("```")
            .or_else(|| trimmed.strip_prefix("~~~"))
        {
            let lang = lang.trim();
            if !lang.is_empty() {
                return Some(lang.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
