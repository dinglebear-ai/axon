use std::collections::HashMap;

use super::chunk::{CodeChunk, SymbolKind};

const TINY_CHARS: usize = 200;
const MAX_HEADER_CHARS: usize = 200;
const MAX_CODE_CHUNK_CHARS: usize = 2000;

pub(super) fn attach_leading_comments(
    chunks: Vec<CodeChunk>,
    source: &str,
    ext: &str,
) -> Vec<CodeChunk> {
    chunks
        .into_iter()
        .map(|mut chunk| {
            let Some((prefix, start_line)) =
                leading_comment_prefix(source, chunk.declaration_start_line, ext)
            else {
                return chunk;
            };
            if chunk.text.trim_start().starts_with(prefix.trim_start()) {
                return chunk;
            }
            chunk.text = format!("{prefix}{}", chunk.text);
            chunk.start_line = start_line;
            chunk
        })
        .collect()
}

pub(super) fn dedupe_exact_ranges(chunks: Vec<CodeChunk>) -> Vec<CodeChunk> {
    let mut last_index: HashMap<(u32, u32, u32, u32, Option<SymbolKind>), usize> = HashMap::new();
    for (idx, chunk) in chunks.iter().enumerate() {
        last_index.insert(
            (
                chunk.declaration_start_line,
                chunk.declaration_end_line,
                chunk.start_line,
                chunk.end_line,
                chunk.symbol_kind(),
            ),
            idx,
        );
    }
    chunks
        .into_iter()
        .enumerate()
        .filter(|(idx, chunk)| {
            last_index.get(&(
                chunk.declaration_start_line,
                chunk.declaration_end_line,
                chunk.start_line,
                chunk.end_line,
                chunk.symbol_kind(),
            )) == Some(idx)
        })
        .map(|(_, chunk)| chunk)
        .collect()
}

pub(super) fn merge_tiny_declarations(chunks: Vec<CodeChunk>) -> Vec<CodeChunk> {
    let mut out = Vec::with_capacity(chunks.len());
    let mut idx = 0usize;
    while idx < chunks.len() {
        let Some(kind) = chunks[idx].symbol_kind() else {
            out.push(chunks[idx].clone());
            idx += 1;
            continue;
        };
        if !kind.is_tiny_merge_eligible() || chunks[idx].text.len() > TINY_CHARS {
            out.push(chunks[idx].clone());
            idx += 1;
            continue;
        }

        let start = idx;
        let mut end = idx + 1;
        let mut total_len = chunks[idx].text.len();
        while end < chunks.len()
            && chunks[end].symbol_kind() == Some(kind)
            && chunks[end].text.len() <= TINY_CHARS
            && !has_blank_line_between(&chunks[end - 1], &chunks[end])
            && total_len + chunks[end].text.len() + 2 <= MAX_CODE_CHUNK_CHARS
        {
            total_len += chunks[end].text.len() + 2;
            end += 1;
        }

        if end == start + 1 {
            out.push(chunks[idx].clone());
            idx += 1;
            continue;
        }

        let mut merged = chunks[start].clone();
        merged.text = chunks[start..end]
            .iter()
            .map(|chunk| chunk.text.trim_end())
            .collect::<Vec<_>>()
            .join("\n\n");
        merged.byte_end = chunks[end - 1].byte_end;
        merged.end_line = chunks[end - 1].end_line;
        merged.declaration_end_line = chunks[end - 1].declaration_end_line;
        // The merged group spans several declarations; drop the single name but
        // keep the kind (the merge-eligibility predicate guaranteed one).
        if let Some(symbol) = merged.symbol.as_mut() {
            symbol.name = None;
        }
        out.push(merged);
        idx = end;
    }
    out
}

pub(super) fn inject_declaration_headers(mut chunks: Vec<CodeChunk>) -> Vec<CodeChunk> {
    let mut group_counts: HashMap<(u32, u32), usize> = HashMap::new();
    for chunk in &chunks {
        *group_counts
            .entry((chunk.declaration_start_line, chunk.declaration_end_line))
            .or_default() += 1;
    }
    let mut group_seen: HashMap<(u32, u32), usize> = HashMap::new();
    let mut group_headers: HashMap<(u32, u32), String> = HashMap::new();

    for chunk in &mut chunks {
        let key = (chunk.declaration_start_line, chunk.declaration_end_line);
        let seen = group_seen.entry(key).or_default();
        *seen += 1;
        if group_counts.get(&key).copied().unwrap_or(0) < 2 {
            continue;
        }
        if *seen == 1 {
            group_headers.insert(key, header_from_text(&chunk.text));
            continue;
        }
        if group_headers
            .get(&key)
            .is_some_and(|header| !header.contains('{'))
        {
            let candidate = header_from_text(&chunk.text);
            if candidate.contains('{') {
                group_headers.insert(key, candidate);
            }
        }
        let Some(header) = group_headers.get(&key).filter(|h| !h.is_empty()) else {
            continue;
        };
        if chunk.text.starts_with(header) {
            continue;
        }
        let separator = if header.ends_with('\n') { "" } else { "\n" };
        let body_budget = MAX_CODE_CHUNK_CHARS.saturating_sub(header.len() + separator.len());
        let body = take_chars(&chunk.text, body_budget);
        chunk.text = format!("{header}{separator}{body}");
    }
    ensure_symbol_headers(chunks)
}

fn ensure_symbol_headers(mut chunks: Vec<CodeChunk>) -> Vec<CodeChunk> {
    let mut seen: HashMap<(u32, u32, Option<SymbolKind>, Option<String>), usize> = HashMap::new();
    for chunk in &mut chunks {
        let key = (
            chunk.declaration_start_line,
            chunk.declaration_end_line,
            chunk.symbol_kind(),
            chunk.symbol_name().map(str::to_string),
        );
        let count = seen.entry(key).or_default();
        *count += 1;
        if *count == 1 {
            continue;
        }
        let Some(header) = synthesized_header(chunk) else {
            continue;
        };
        if !chunk
            .text
            .trim_start()
            .starts_with(header.trim_end_matches('\n'))
        {
            chunk.text = format!("{header}{}", chunk.text);
        }
    }
    chunks
}

fn synthesized_header(chunk: &CodeChunk) -> Option<String> {
    let name = chunk.symbol_name()?;
    let short = name
        .rsplit_once("::")
        .map(|(_, rhs)| rhs)
        .or_else(|| name.rsplit_once('.').map(|(_, rhs)| rhs))
        .unwrap_or(name);
    match chunk.symbol_kind() {
        Some(SymbolKind::Function | SymbolKind::Method) => Some(format!("fn {short}()\n")),
        Some(SymbolKind::Struct) => Some(format!("struct {short}\n")),
        Some(SymbolKind::Enum) => Some(format!("enum {short}\n")),
        Some(SymbolKind::Trait) => Some(format!("trait {short}\n")),
        Some(SymbolKind::Const) => Some(format!("const {short}\n")),
        Some(SymbolKind::Static) => Some(format!("static {short}\n")),
        Some(SymbolKind::Type) => Some(format!("type {short}\n")),
        Some(SymbolKind::Mod) => Some(format!("mod {short}\n")),
        _ => None,
    }
}

fn leading_comment_prefix(
    source: &str,
    declaration_start_line: u32,
    ext: &str,
) -> Option<(String, u32)> {
    if declaration_start_line <= 1 {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut idx = declaration_start_line.saturating_sub(2) as usize;

    while idx < lines.len() && is_attribute_line(lines[idx], ext) {
        if idx == 0 {
            return None;
        }
        idx -= 1;
    }

    let mut collected = Vec::new();
    let mut comment_style: Option<CommentStyle> = None;
    loop {
        let line = lines[idx];
        if line.trim().is_empty() {
            break;
        }
        let Some(style) = comment_style_for_line(line, ext) else {
            break;
        };
        if matches!(style, CommentStyle::InnerDoc) {
            break;
        }
        if let Some(existing) = comment_style
            && existing != style
        {
            break;
        }
        comment_style = Some(style);
        collected.push(line);
        if idx == 0 {
            break;
        }
        idx -= 1;
    }

    if collected.is_empty() {
        return None;
    }
    collected.reverse();
    let start_line = declaration_start_line - collected.len() as u32;
    if start_line == 1 {
        return None;
    }
    if collected.len() > 10 {
        let keep_from = collected.len() - 10;
        collected = collected.split_off(keep_from);
    }
    let mut prefix = collected.join("\n");
    prefix.push('\n');
    Some((prefix, declaration_start_line - collected.len() as u32))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentStyle {
    RustDoc,
    RustPlain,
    GoPlain,
    Block,
    InnerDoc,
}

fn comment_style_for_line(line: &str, ext: &str) -> Option<CommentStyle> {
    let trimmed = line.trim_start();
    match ext {
        "rs" if trimmed.starts_with("//!") || trimmed.starts_with("/*!") => {
            Some(CommentStyle::InnerDoc)
        }
        "rs" if trimmed.starts_with("///") => Some(CommentStyle::RustDoc),
        "rs" if trimmed.starts_with("//") => Some(CommentStyle::RustPlain),
        "rs" if trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/") =>
        {
            Some(CommentStyle::Block)
        }
        "go" if trimmed.starts_with("//") => Some(CommentStyle::GoPlain),
        "go" if trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.ends_with("*/") =>
        {
            Some(CommentStyle::Block)
        }
        _ => None,
    }
}

fn is_attribute_line(line: &str, ext: &str) -> bool {
    ext == "rs" && line.trim_start().starts_with("#[")
}

fn has_blank_line_between(left: &CodeChunk, right: &CodeChunk) -> bool {
    right.start_line.saturating_sub(left.end_line) > 1
}

fn header_from_text(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines().take(10) {
        if out.len() + line.len() + 1 > MAX_HEADER_CHARS {
            break;
        }
        out.push_str(line);
        out.push('\n');
        if line.contains('{') {
            break;
        }
    }
    out
}

fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
#[path = "postprocess_tests.rs"]
mod tests;
