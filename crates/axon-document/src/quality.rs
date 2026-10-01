//! Final shared quality gate, run after structural fallback and size bounding.
use crate::{chunk::DocumentChunk, text};

pub(crate) const MIN_CHUNK_CHARS: usize = 50;

fn meaningful(chunk: &DocumentChunk) -> bool {
    chunk.content.chars().any(char::is_alphanumeric)
}
fn short(chunk: &DocumentChunk) -> bool {
    chunk.content.chars().count() < MIN_CHUNK_CHARS
}
fn literal(chunk: &DocumentChunk, source: &str) -> Option<(usize, usize)> {
    let start = usize::try_from(chunk.range.byte_start?).ok()?;
    let end = usize::try_from(chunk.range.byte_end?).ok()?;
    (source.get(start..end) == Some(chunk.content.as_str())).then_some((start, end))
}
fn compatible(a: &DocumentChunk, b: &DocumentChunk, source: &str) -> bool {
    if a.heading_path != b.heading_path
        || a.title != b.title
        || a.range.session_turn_id != b.range.session_turn_id
        || a.range.json_pointer != b.range.json_pointer
        || a.range.yaml_path != b.range.yaml_path
        || a.range.xml_xpath != b.range.xml_xpath
        || a.range.csv_row != b.range.csv_row
        || a.range.time_start_ms != b.range.time_start_ms
        || a.range.time_end_ms != b.range.time_end_ms
    {
        return false;
    }
    match (literal(a, source), literal(b, source)) {
        (Some((_, end)), Some((start, _))) => {
            end <= start && !source[end..start].chars().any(char::is_alphanumeric)
        }
        _ => false,
    }
}
fn fits(content: &str, max_chars: usize) -> bool {
    content.len() <= text::MAX_PLAIN_TEXT_CHUNK_BYTES
        && content.chars().count() <= max_chars.min(text::MAX_PLAIN_TEXT_CHUNK_CHARS)
}
fn remember_symbols(target: &mut DocumentChunk, other: &DocumentChunk) {
    let mut symbols = Vec::new();
    for chunk in [&*target, other] {
        if let Some(symbol) = &chunk.symbol {
            symbols.push(symbol.clone());
        }
        if let Some(values) = chunk
            .metadata
            .get("code_symbol_aliases")
            .and_then(|v| v.as_array())
        {
            symbols.extend(values.iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
    }
    symbols.sort();
    symbols.dedup();
    if !symbols.is_empty() {
        target
            .metadata
            .insert("code_symbol_aliases".into(), serde_json::json!(symbols));
    }
}
fn combine(
    a: &mut DocumentChunk,
    b: &DocumentChunk,
    source: &str,
    positions: &text::SourcePositions,
    max_chars: usize,
) -> bool {
    if !compatible(a, b, source) {
        return false;
    }
    let (start, _) = literal(a, source).unwrap();
    let (_, end) = literal(b, source).unwrap();
    if !fits(&source[start..end], max_chars) {
        return false;
    }
    remember_symbols(a, b);
    if a.symbol.is_none() {
        a.symbol = b.symbol.clone();
        if let Some(status) = b.metadata.get("symbol_extraction_status") {
            a.metadata
                .insert("symbol_extraction_status".into(), status.clone());
        }
        for key in [
            "code_symbol_kind",
            "code_symbol_source_range",
            "parser_method",
        ] {
            if let Some(value) = b.metadata.get(key) {
                a.metadata.entry(key.into()).or_insert(value.clone());
            }
        }
    }
    let left_method = a.metadata.get("actual_chunking_method").cloned();
    let right_method = b.metadata.get("actual_chunking_method").cloned();
    match (left_method, right_method) {
        (None, Some(method)) => {
            a.metadata.insert("actual_chunking_method".into(), method);
        }
        (Some(left), Some(right)) if left != right => {
            a.metadata.insert(
                "actual_chunking_method".into(),
                "source_adjacent_packing".into(),
            );
        }
        _ => {}
    }
    if b.metadata
        .get("code_syntax_recovered")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        a.metadata
            .insert("code_syntax_recovered".into(), true.into());
    }
    if b.metadata.get("code_parse_status").and_then(|v| v.as_str()) == Some("partial") {
        a.metadata
            .insert("code_parse_status".into(), "partial".into());
    }
    a.content = source[start..end].into();
    update_literal_range(a, positions, start, end);
    a.metadata
        .insert("chunk_quality_action".into(), "packed_adjacent".into());
    true
}

fn update_literal_range(
    chunk: &mut DocumentChunk,
    positions: &text::SourcePositions,
    start: usize,
    end: usize,
) {
    let exact = positions.source_range(start, end);
    chunk.range.byte_start = exact.byte_start;
    chunk.range.byte_end = exact.byte_end;
    chunk.range.char_start = exact.char_start;
    chunk.range.char_end = exact.char_end;
    chunk.range.line_start = exact.line_start;
    chunk.range.line_end = exact.line_end;
}

#[cfg(test)]
pub(crate) fn useful_chunks(chunks: Vec<DocumentChunk>, source: &str) -> Vec<DocumentChunk> {
    useful_chunks_with_limit(chunks, source, text::MAX_PLAIN_TEXT_CHUNK_CHARS)
}

pub(crate) fn useful_chunks_with_limit(
    chunks: Vec<DocumentChunk>,
    source: &str,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    let positions = std::cell::OnceCell::new();
    let mut packed: Vec<DocumentChunk> = Vec::new();
    for chunk in chunks.into_iter().filter(meaningful) {
        if let Some(previous) = packed.last_mut() {
            if (short(previous) || short(&chunk))
                && compatible(previous, &chunk, source)
                && combine(
                    previous,
                    &chunk,
                    source,
                    positions.get_or_init(|| text::SourcePositions::new(source)),
                    max_chars,
                )
            {
                continue;
            }
        }
        packed.push(chunk);
    }
    if packed.iter().any(short) {
        let positions = positions.get_or_init(|| text::SourcePositions::new(source));
        borrow_following_context(&mut packed, source, positions, max_chars);
        borrow_previous_context(&mut packed, source, positions, max_chars);
    }
    packed
        .into_iter()
        .filter(|chunk| !short(chunk) && meaningful(chunk))
        .collect()
}

fn borrow_following_context(
    packed: &mut [DocumentChunk],
    source: &str,
    positions: &text::SourcePositions,
    max_chars: usize,
) {
    // A leading short symbol can borrow the beginning of its compatible
    // successor when that successor is too large to absorb it in full.
    for index in 0..packed.len().saturating_sub(1) {
        if !short(&packed[index]) || !compatible(&packed[index], &packed[index + 1], source) {
            continue;
        }
        let (start, _) = literal(&packed[index], source).unwrap();
        let (_, upper) = literal(&packed[index + 1], source).unwrap();
        let end = source[start..upper]
            .char_indices()
            .nth(MIN_CHUNK_CHARS)
            .map(|(offset, _)| start + offset)
            .unwrap_or(upper);
        if fits(&source[start..end], max_chars) {
            packed[index].content = source[start..end].into();
            update_literal_range(&mut packed[index], positions, start, end);
            packed[index]
                .metadata
                .insert("chunk_quality_action".into(), "source_context".into());
            packed[index]
                .metadata
                .insert("actual_chunking_method".into(), "source_context".into());
        }
    }
}

fn borrow_previous_context(
    packed: &mut [DocumentChunk],
    source: &str,
    positions: &text::SourcePositions,
    max_chars: usize,
) {
    // Include same-record context when a full predecessor cannot absorb a tiny tail.
    for index in 1..packed.len() {
        if !short(&packed[index]) {
            continue;
        }
        let missing = MIN_CHUNK_CHARS - packed[index].content.chars().count();
        if compatible(&packed[index - 1], &packed[index], source) {
            let (lower, _) = literal(&packed[index - 1], source).unwrap();
            let (start, end) = literal(&packed[index], source).unwrap();
            let context_start = source[lower..start]
                .char_indices()
                .rev()
                .nth(missing.saturating_sub(1))
                .map(|(offset, _)| lower + offset)
                .unwrap_or(lower);
            if fits(&source[context_start..end], max_chars) {
                packed[index].content = source[context_start..end].into();
                update_literal_range(&mut packed[index], positions, context_start, end);
            }
        } else if packed[index].range == packed[index - 1].range
            && packed[index].heading_path == packed[index - 1].heading_path
            && packed[index]
                .metadata
                .get("chunking_fallback")
                .and_then(|v| v.as_str())
                == Some("embedding_size_backstop")
            && literal(&packed[index], source).is_none()
        {
            let previous = &packed[index - 1].content;
            let context_start = previous
                .char_indices()
                .rev()
                .nth(missing.saturating_sub(1))
                .map(|(offset, _)| offset)
                .unwrap_or(0);
            let content = format!("{}{}", &previous[context_start..], packed[index].content);
            if fits(&content, max_chars) {
                packed[index].content = content;
            }
        }
        if !short(&packed[index]) {
            packed[index]
                .metadata
                .insert("chunk_quality_action".into(), "source_context".into());
            packed[index]
                .metadata
                .insert("actual_chunking_method".into(), "source_context".into());
        }
    }
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
