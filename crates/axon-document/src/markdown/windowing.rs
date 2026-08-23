//! Fence-aware Markdown windowing and metadata-safe packing.

use crate::chunk::DocumentChunk;
use crate::text::source_range_from_positions;

use super::MarkdownChunkLimits;
use super::semantics::{SemanticLayout, containing_block, latest_boundary, preferred_boundary};

pub(super) fn split_oversized_sections(
    source: &str,
    chunks: Vec<DocumentChunk>,
    limits: MarkdownChunkLimits,
) -> Vec<DocumentChunk> {
    let positions = SourcePositions::new(source);
    let mut split = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if chunk.content.chars().count() <= limits.max_chars {
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
        if chunk
            .metadata
            .get("markdown_block_kind")
            .and_then(serde_json::Value::as_str)
            == Some("frontmatter")
        {
            // Hard size backstop: genuine frontmatter within limits stays
            // atomic (handled above), but an oversized block — usually a
            // thematic-break false positive — must not bypass `max_chars`.
            for (window_start, window_end) in char_windows(&chunk.content, limits.max_chars) {
                if let Some(window) = ranged_clone(
                    source,
                    &positions,
                    &chunk,
                    content_start,
                    window_start,
                    window_end,
                ) {
                    split.push(window);
                }
            }
            continue;
        }
        for span in fenced_spans(&chunk.content) {
            match span.kind {
                MarkdownSpanKind::Fence { language } => {
                    // Hard size backstop: a fence larger than `max_chars`
                    // (including an unterminated fence that runs to the end
                    // of the section) is split into plain char-bounded
                    // windows — no overlap inside code — with the code
                    // metadata preserved on every window.
                    let fence = &chunk.content[span.start..span.end];
                    for (window_start, window_end) in char_windows(fence, limits.max_chars) {
                        let Some(mut window) = ranged_clone(
                            source,
                            &positions,
                            &chunk,
                            content_start,
                            span.start + window_start,
                            span.start + window_end,
                        ) else {
                            continue;
                        };
                        window
                            .metadata
                            .insert("markdown_block_kind".to_string(), "code".into());
                        if let Some(language) = &language {
                            window
                                .metadata
                                .insert("code_fence_language".to_string(), language.clone().into());
                        } else {
                            window.metadata.remove("code_fence_language");
                        }
                        split.push(window);
                    }
                }
                MarkdownSpanKind::Prose => {
                    let prose = &chunk.content[span.start..span.end];
                    for (window_start, window_end) in
                        bounded_content_windows(prose, limits.max_chars, limits.overlap_chars)
                    {
                        let Some(mut window) = ranged_clone(
                            source,
                            &positions,
                            &chunk,
                            content_start,
                            span.start + window_start,
                            span.start + window_end,
                        ) else {
                            continue;
                        };
                        window
                            .metadata
                            .insert("markdown_block_kind".to_string(), "prose".into());
                        window.metadata.remove("code_fence_language");
                        split.push(window);
                    }
                }
            }
        }
    }
    split
}

/// Plain char-bounded, UTF-8-safe byte windows over `content`, each at most
/// `max_chars` characters, with no overlap. Size backstop for spans that are
/// otherwise emitted whole (fences, frontmatter).
fn char_windows(content: &str, max_chars: usize) -> Vec<(usize, usize)> {
    let max_chars = max_chars.max(1);
    let mut char_offsets = content
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    char_offsets.push(content.len());
    let char_count = char_offsets.len().saturating_sub(1);
    let mut windows = Vec::new();
    let mut start_char = 0usize;
    while start_char < char_count {
        let end_char = start_char.saturating_add(max_chars).min(char_count);
        windows.push((char_offsets[start_char], char_offsets[end_char]));
        start_char = end_char;
    }
    windows
}

fn bounded_content_windows(
    content: &str,
    max_chars: usize,
    overlap_chars: usize,
) -> Vec<(usize, usize)> {
    let max_chars = max_chars.max(1);
    let overlap_chars = overlap_chars.min(max_chars.saturating_sub(1));
    let layout = SemanticLayout::new(content, max_chars);
    let mut char_offsets = content
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    char_offsets.push(content.len());
    let mut windows = Vec::new();
    let char_count = char_offsets.len().saturating_sub(1);
    let mut start_char = 0usize;
    let mut paragraph_cursor = 0usize;
    let mut structural_cursor = 0usize;
    let mut line_cursor = 0usize;
    let mut overlap_cursor = 0usize;
    let mut target_block_cursor = 0usize;
    let mut overlap_block_cursor = 0usize;
    while start_char < char_count {
        let target_char = start_char.saturating_add(max_chars).min(char_count);
        let mut skip_overlap = false;
        let end_char = if target_char == char_count {
            target_char
        } else if let Some(block) = containing_block(
            &layout.structural_blocks,
            &mut target_block_cursor,
            target_char,
        ) {
            if block.start > start_char {
                skip_overlap = true;
                block.start
            } else {
                block.end
            }
        } else {
            let preferred_floor = start_char
                .saturating_add(max_chars.saturating_mul(2) / 3)
                .max(start_char.saturating_add(overlap_chars).saturating_add(1));
            preferred_boundary(
                &layout,
                target_char,
                preferred_floor,
                &mut paragraph_cursor,
                &mut structural_cursor,
                &mut line_cursor,
            )
            .unwrap_or(target_char)
        };
        windows.push((char_offsets[start_char], char_offsets[end_char]));
        if end_char == char_count {
            break;
        }
        if skip_overlap {
            start_char = end_char;
            continue;
        }

        let desired_start = end_char - overlap_chars;
        let semantic_start = (overlap_chars > 0)
            .then(|| {
                latest_boundary(
                    &layout.line_boundaries,
                    &mut overlap_cursor,
                    desired_start,
                    start_char,
                )
            })
            .flatten()
            .unwrap_or(desired_start);
        start_char = if let Some(block) = containing_block(
            &layout.structural_blocks,
            &mut overlap_block_cursor,
            semantic_start,
        ) {
            block.end.min(end_char)
        } else {
            semantic_start
        };
    }
    windows
}

pub(super) fn pack_small_sections(
    chunks: Vec<DocumentChunk>,
    limits: MarkdownChunkLimits,
) -> Vec<DocumentChunk> {
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
        let should_pack = compatible_for_packing(previous, &chunk)
            && combined_chars <= max_chars
            && (previous_chars < min_chars || chunk_chars < min_chars);
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

fn compatible_for_packing(left: &DocumentChunk, right: &DocumentChunk) -> bool {
    left.title == right.title
        && left.heading_path == right.heading_path
        && left.symbol == right.symbol
        && left.metadata == right.metadata
        && left.range.byte_end <= right.range.byte_start
}

#[derive(Debug)]
struct MarkdownSpan {
    start: usize,
    end: usize,
    kind: MarkdownSpanKind,
}

#[derive(Debug)]
enum MarkdownSpanKind {
    Prose,
    Fence { language: Option<String> },
}

fn fenced_spans(content: &str) -> Vec<MarkdownSpan> {
    let mut spans = Vec::new();
    let mut prose_start = 0usize;
    let mut fence: Option<(usize, char, usize, Option<String>)> = None;
    let mut offset = 0usize;

    for line in content.split_inclusive('\n') {
        let line_end = offset + line.len();
        let stripped = line.trim_end_matches(['\r', '\n']).trim_start();
        if let Some((_, marker, width, _)) = fence.as_ref() {
            if closes_fence(stripped, *marker, *width) {
                let (fence_start, _, _, language) = fence.take().expect("open fence exists");
                spans.push(MarkdownSpan {
                    start: fence_start,
                    end: line_end,
                    kind: MarkdownSpanKind::Fence { language },
                });
                prose_start = line_end;
            }
        } else if let Some((marker, width, language)) = opens_fence(stripped) {
            if prose_start < offset {
                spans.push(MarkdownSpan {
                    start: prose_start,
                    end: offset,
                    kind: MarkdownSpanKind::Prose,
                });
            }
            fence = Some((offset, marker, width, language));
        }
        offset = line_end;
    }

    if let Some((fence_start, _, _, language)) = fence {
        spans.push(MarkdownSpan {
            start: fence_start,
            end: content.len(),
            kind: MarkdownSpanKind::Fence { language },
        });
    } else if prose_start < content.len() {
        spans.push(MarkdownSpan {
            start: prose_start,
            end: content.len(),
            kind: MarkdownSpanKind::Prose,
        });
    }
    spans
}

pub(super) fn opens_fence(line: &str) -> Option<(char, usize, Option<String>)> {
    let marker = line.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let width = line
        .chars()
        .take_while(|candidate| *candidate == marker)
        .count();
    if width < 3 {
        return None;
    }
    let language = line[marker.len_utf8() * width..]
        .trim()
        .split_ascii_whitespace()
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some((marker, width, language))
}

pub(super) fn closes_fence(line: &str, marker: char, width: usize) -> bool {
    let run = line
        .chars()
        .take_while(|candidate| *candidate == marker)
        .count();
    run >= width && line[marker.len_utf8() * run..].trim().is_empty()
}

fn ranged_clone(
    source: &str,
    positions: &SourcePositions,
    chunk: &DocumentChunk,
    content_start: usize,
    relative_start: usize,
    relative_end: usize,
) -> Option<DocumentChunk> {
    let raw = &chunk.content[relative_start..relative_end];
    if raw.trim().is_empty() {
        return None;
    }
    let trimmed_start = raw.len() - raw.trim_start().len();
    let trimmed_end = raw.trim_end().len();
    let absolute_start = content_start + relative_start + trimmed_start;
    let absolute_end = content_start + relative_start + trimmed_end;
    let mut window = chunk.clone();
    window.content = source[absolute_start..absolute_end].to_string();
    window.range = positions.range(absolute_start, absolute_end);
    Some(window)
}

struct SourcePositions {
    char_offsets: Vec<usize>,
    line_offsets: Vec<usize>,
}

impl SourcePositions {
    fn new(source: &str) -> Self {
        let mut char_offsets = source
            .char_indices()
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        char_offsets.push(source.len());
        let mut line_offsets = vec![0];
        line_offsets.extend(
            source
                .match_indices('\n')
                .map(|(offset, _)| offset.saturating_add(1)),
        );
        Self {
            char_offsets,
            line_offsets,
        }
    }

    fn range(&self, start: usize, end: usize) -> axon_api::source::SourceRange {
        let start_char = self.char_offsets.partition_point(|offset| *offset < start);
        let end_char = self.char_offsets.partition_point(|offset| *offset < end);
        let line_start = self.line_offsets.partition_point(|offset| *offset <= start) as u32;
        let line_end_offset = end.saturating_sub(1);
        let line_end = self
            .line_offsets
            .partition_point(|offset| *offset <= line_end_offset) as u32;
        source_range_from_positions(
            start,
            end,
            line_start.max(1),
            line_end.max(1),
            start_char as u64,
            end_char as u64,
        )
    }
}
