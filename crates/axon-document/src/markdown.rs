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
    let normalized = text.to_ascii_lowercase();
    let mut cursor = 0usize;

    while let Some(relative_open) = normalized[cursor..].find('<') {
        let open = cursor + relative_open;
        plain.push_str(&text[cursor..open]);
        let Some(relative_close) = normalized[open + 1..].find('>') else {
            break;
        };
        let close = open + 1 + relative_close;
        let tag = normalized[open + 1..close].trim_start();
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|ch: char| ch.is_ascii_whitespace() || ch == '/')
            .next()
            .unwrap_or_default();

        if !closing && is_non_content_html_tag(name) && !tag.ends_with('/') {
            let closing_tag = format!("</{name}");
            let search_from = close + 1;
            let Some(relative_end_open) = normalized[search_from..].find(&closing_tag) else {
                cursor = text.len();
                break;
            };
            let end_open = search_from + relative_end_open;
            let Some(relative_end_close) = normalized[end_open + closing_tag.len()..].find('>')
            else {
                cursor = text.len();
                break;
            };
            cursor = end_open + closing_tag.len() + relative_end_close + 1;
        } else {
            cursor = close + 1;
        }
        plain.push('\n');
    }
    if cursor < text.len() {
        plain.push_str(&text[cursor..]);
    }
    let visible = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    plain_text_windows(&visible)
        .into_iter()
        .map(|mut chunk| {
            // The visible-text buffer is a lossy DOM projection, so its byte
            // offsets do not map back to raw HTML. Anchor each derived chunk
            // to the full source document instead of publishing false or
            // out-of-bounds offsets from the transformed buffer.
            chunk.range = source_range(text, 0, text.len());
            chunk
        })
        .collect()
}

fn is_non_content_html_tag(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "template" | "noscript" | "svg" | "canvas"
    )
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
        } else if !in_fence && let Some(level) = atx_heading_level(stripped) {
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
