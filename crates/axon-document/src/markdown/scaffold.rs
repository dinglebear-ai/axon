//! Empty headings are section context, not standalone content.
use super::{DocumentChunk, windowing::SourcePositions};

fn heading_only(chunk: &DocumentChunk) -> bool {
    if matches!(
        chunk
            .metadata
            .get("markdown_block_kind")
            .and_then(|v| v.as_str()),
        Some("code" | "frontmatter")
    ) {
        return false;
    }
    let mut lines = chunk.content.lines().filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    super::line_heading_level(first).is_some()
        && lines.all(|line| super::line_heading_level(line).is_some())
}

pub(crate) fn omit_empty_heading_spans(
    chunks: Vec<DocumentChunk>,
    source: &str,
) -> Vec<DocumentChunk> {
    let (_, body_start) = super::extract_frontmatter(source);
    let headings = super::fence_aware_headings(source, body_start);
    let mut empty: Vec<(usize, usize)> = Vec::new();
    for (index, heading) in headings.iter().enumerate() {
        let end = headings
            .get(index + 1)
            .map_or(source.len(), |next| next.byte);
        let mut lines = source[heading.byte..end].split_inclusive('\n');
        let heading_bytes = lines.next().map_or(0, str::len);
        let blank_bytes: usize = lines
            .take_while(|line| line.trim().is_empty())
            .map(str::len)
            .sum();
        let scaffold_end = heading.byte + heading_bytes + blank_bytes;
        if let Some(previous) = empty.last_mut()
            && previous.1 == heading.byte
        {
            previous.1 = scaffold_end;
        } else {
            empty.push((heading.byte, scaffold_end));
        }
    }
    chunks
        .into_iter()
        .filter(|chunk| {
            let (Some(start), Some(end)) = (chunk.range.byte_start, chunk.range.byte_end) else {
                return true;
            };
            let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
                return true;
            };
            // Classify the original fence-aware source, not a fallback window's
            // first character: long title continuations and fenced comments differ.
            if source
                .get(start..end)
                .is_none_or(|text| text.trim() != chunk.content.trim())
            {
                return true;
            }
            let index = empty.partition_point(|(_, boundary)| *boundary <= start);
            !empty
                .get(index)
                .is_some_and(|(a, b)| *a <= start && end <= *b)
        })
        .collect()
}

pub(super) fn attach_empty_headings(
    text: &str,
    positions: &SourcePositions,
    chunks: Vec<DocumentChunk>,
) -> Vec<DocumentChunk> {
    let mut pending: Option<DocumentChunk> = None;
    let mut output = Vec::with_capacity(chunks.len());
    for mut chunk in chunks {
        if let Some(heading) = pending.take()
            && heading.heading_path.len() < chunk.heading_path.len()
            && chunk.heading_path.starts_with(&heading.heading_path)
        {
            // Adjacent ancestor headings belong to the first content-bearing
            // descendant. Siblings keep their own scope; empty siblings vanish.
            let start = heading.range.byte_start.unwrap() as usize;
            let end = chunk.range.byte_end.unwrap() as usize;
            chunk.content = text[start..end].trim().to_string();
            chunk.range = positions.range(start, end);
        }
        if heading_only(&chunk) {
            pending = Some(chunk);
        } else {
            output.push(chunk);
        }
    }
    output
}
