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

#[derive(Debug, Clone, Copy)]
struct MarkdownChunkLimits {
    max_chars: usize,
    min_chars: usize,
}

const CURRENT_STRUCTURAL_DEFAULTS: MarkdownChunkLimits = MarkdownChunkLimits {
    max_chars: 2_000,
    min_chars: 500,
};

impl MarkdownChunkLimits {
    fn configured() -> Self {
        let max_chars = axon_core::config::parse::tuning::chunking_markdown_max_chars();
        Self {
            max_chars,
            min_chars: axon_core::config::parse::tuning::chunking_markdown_min_chars(max_chars),
        }
    }
}

/// One ATX heading line: byte offset of its `#` run, its level (1-6), and
/// its title text.
struct Heading {
    byte: usize,
    level: usize,
    title: String,
}

pub(crate) fn markdown_sections(text: &str) -> Vec<DocumentChunk> {
    markdown_sections_with_limits(text, MarkdownChunkLimits::configured())
}

fn markdown_sections_with_limits(text: &str, limits: MarkdownChunkLimits) -> Vec<DocumentChunk> {
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

    let chunks = split_oversized_sections(text, chunks, limits.max_chars);
    pack_small_sections(chunks, limits)
}

fn split_oversized_sections(
    source: &str,
    chunks: Vec<DocumentChunk>,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    let max_chars = max_chars.max(1);
    let mut split = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if chunk.content.chars().count() <= max_chars
            || chunk.metadata.contains_key("code_fence_language")
        {
            split.push(chunk);
            continue;
        }
        let Some(range_start) = chunk.range.byte_start.map(|value| value as usize) else {
            split.push(chunk);
            continue;
        };
        let range_end = chunk
            .range
            .byte_end
            .map(|value| value as usize)
            .unwrap_or(source.len())
            .min(source.len());
        let Some(relative_content_start) = source[range_start..range_end].find(&chunk.content)
        else {
            split.push(chunk);
            continue;
        };
        let content_start = range_start + relative_content_start;
        for (relative_start, relative_end) in bounded_content_windows(&chunk.content, max_chars) {
            let mut window = chunk.clone();
            window.content = chunk.content[relative_start..relative_end]
                .trim()
                .to_string();
            if window.content.is_empty() {
                continue;
            }
            let leading_trim = chunk.content[relative_start..relative_end]
                .find(&window.content)
                .unwrap_or(0);
            let absolute_start = content_start + relative_start + leading_trim;
            let absolute_end = absolute_start + window.content.len();
            window.range = source_range(source, absolute_start, absolute_end);
            split.push(window);
        }
    }
    split
}

fn bounded_content_windows(content: &str, max_chars: usize) -> Vec<(usize, usize)> {
    let mut windows = Vec::new();
    let mut start = 0usize;
    while content[start..].chars().count() > max_chars {
        let tentative_end = content[start..]
            .char_indices()
            .nth(max_chars)
            .map(|(relative, _)| start + relative)
            .unwrap_or(content.len());
        let end = content[start..tentative_end]
            .rfind('\n')
            .map(|relative| start + relative + 1)
            .filter(|end| *end > start)
            .unwrap_or(tentative_end);
        windows.push((start, end));
        start = end;
    }
    if start < content.len() {
        windows.push((start, content.len()));
    }
    windows
}

fn pack_small_sections(
    chunks: Vec<DocumentChunk>,
    limits: MarkdownChunkLimits,
) -> Vec<DocumentChunk> {
    // Preserve the established heading-per-chunk behavior unless an operator
    // explicitly tunes the advertised Markdown limits. This avoids silently
    // changing retrieval granularity for existing installations while making
    // the tuning surface effective again after the unified-pipeline cutover.
    if limits.max_chars == CURRENT_STRUCTURAL_DEFAULTS.max_chars
        && limits.min_chars == CURRENT_STRUCTURAL_DEFAULTS.min_chars
    {
        return chunks;
    }
    let max_chars = limits.max_chars.max(1);
    let min_chars = limits.min_chars.clamp(1, max_chars);
    let mut packed: Vec<DocumentChunk> = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let chunk_chars = chunk.content.chars().count();
        let Some(previous) = packed.last_mut() else {
            packed.push(chunk);
            continue;
        };
        let previous_chars = previous.content.chars().count();
        let separator_chars = usize::from(!previous.content.is_empty()) * 2;
        let combined_chars = previous_chars
            .saturating_add(separator_chars)
            .saturating_add(chunk_chars);
        let should_pack =
            combined_chars <= max_chars && (previous_chars < min_chars || chunk_chars < min_chars);
        if !should_pack {
            packed.push(chunk);
            continue;
        }

        if !previous.content.is_empty() {
            previous.content.push_str("\n\n");
        }
        previous.content.push_str(&chunk.content);
        previous.range.line_end = chunk.range.line_end;
        previous.range.byte_end = chunk.range.byte_end;
        previous.range.char_end = chunk.range.char_end;
        previous.range.time_end_ms = chunk.range.time_end_ms;
    }

    packed
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
