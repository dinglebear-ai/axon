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

mod semantics;
mod windowing;
use windowing::{closes_fence, opens_fence, pack_small_sections, split_oversized_sections};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MarkdownChunkLimits {
    max_chars: usize,
    min_chars: usize,
    overlap_chars: usize,
}

#[cfg(test)]
const CURRENT_STRUCTURAL_DEFAULTS: MarkdownChunkLimits = MarkdownChunkLimits {
    max_chars: 2_000,
    min_chars: 500,
    overlap_chars: 200,
};

impl MarkdownChunkLimits {
    pub(crate) fn new(max_chars: usize, min_chars: usize, overlap_chars: usize) -> Self {
        let max_chars = max_chars.max(1);
        Self {
            max_chars,
            min_chars: min_chars.clamp(1, max_chars),
            overlap_chars: overlap_chars.min(max_chars.saturating_sub(1)),
        }
    }

    pub(crate) fn max_chars(self) -> usize {
        self.max_chars
    }
}

/// One ATX heading line: byte offset of its `#` run, its level (1-6), and
/// its title text.
struct Heading {
    byte: usize,
    level: usize,
    title: String,
}

#[cfg(test)]
pub(crate) fn markdown_sections(text: &str) -> Vec<DocumentChunk> {
    markdown_sections_with_limits(text, CURRENT_STRUCTURAL_DEFAULTS)
}

pub(crate) fn markdown_sections_with_limits(
    text: &str,
    limits: MarkdownChunkLimits,
) -> Vec<DocumentChunk> {
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

    let chunks = split_oversized_sections(text, chunks, limits);
    pack_small_sections(chunks, limits)
}

pub(crate) fn html_article(text: &str) -> Vec<DocumentChunk> {
    let mut plain = String::with_capacity(text.len());
    let normalized = text.to_ascii_lowercase();
    let mut cursor = 0usize;

    while let Some(relative_open) = normalized[cursor..].find('<') {
        let open = cursor + relative_open;
        plain.push_str(&text[cursor..open]);
        // The pre-`<` text is now consumed; keep the cursor on the `<` so a
        // trailing unclosed tag is emitted once by the tail push below
        // instead of duplicating the text pushed above.
        cursor = open;
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
                // Malformed HTML must not silently truncate the document.
                // Treat the unmatched container as ordinary content and keep
                // projecting the remaining text.
                cursor = search_from;
                plain.push('\n');
                continue;
            };
            let end_open = search_from + relative_end_open;
            let Some(relative_end_close) = normalized[end_open + closing_tag.len()..].find('>')
            else {
                cursor = search_from;
                plain.push('\n');
                continue;
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
    let open_len = if text.starts_with("---\n") {
        "---\n".len()
    } else if text.starts_with("---\r\n") {
        "---\r\n".len()
    } else {
        return (None, 0);
    };
    let rest = &text[open_len..];
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let line_end = offset + line.len();
        // The closing delimiter must be a whole line that is exactly `---`
        // (trailing `\r`/whitespace tolerated); `----` or `--- junk` are
        // content, not closers. `offset > 0` keeps the opener from also
        // acting as its own closer on `---\n---`-shaped documents.
        if offset > 0 && line.trim_end() == "---" {
            return (Some(()), open_len + line_end);
        }
        offset = line_end;
    }
    (None, 0)
}

/// Byte offsets/levels/titles of ATX headings (`#`..`######`) that are not
/// inside a fenced code block, starting the scan at `from`.
fn fence_aware_headings(text: &str, from: usize) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut open_fence: Option<(char, usize)> = None;
    let mut offset = from;
    for line in text[from..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let stripped = trimmed.trim_start();
        if let Some((marker, width)) = open_fence {
            if closes_fence(stripped, marker, width) {
                open_fence = None;
            }
        } else if let Some((marker, width, _)) = opens_fence(stripped) {
            open_fence = Some((marker, width));
        } else if let Some(level) = atx_heading_level(stripped) {
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

fn atx_heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')).then_some(hashes)
}

/// First fenced code block's language label within `content`, if any.
/// Fence-state aware: uses the shared `opens_fence`/`closes_fence` scanner so
/// wide fences (` ````rust ` → `rust`, not `` `rust ``) parse correctly and a
/// fence-looking line inside an already-open fence of the other marker is
/// treated as literal content, not a new opener.
fn first_fence_language(content: &str) -> Option<String> {
    let mut open_fence: Option<(char, usize)> = None;
    for line in content.lines() {
        let stripped = line.trim_end_matches('\r').trim_start();
        if let Some((marker, width)) = open_fence {
            if closes_fence(stripped, marker, width) {
                open_fence = None;
            }
        } else if let Some((marker, width, language)) = opens_fence(stripped) {
            if let Some(language) = language {
                return Some(language);
            }
            open_fence = Some((marker, width));
        }
    }
    None
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
