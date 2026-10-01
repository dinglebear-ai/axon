//! Code-oriented chunk builders.
//!
//! Parser facts partition source into disjoint symbol and module intervals.
//! Without parser facts, keyword heuristics provide an explicit fallback.
//! Metadata records the parser and recovery status; oversized intervals become
//! line windows with their symbol provenance retained.

use axon_api::source::SourceParseFacts;

use crate::chunk::DocumentChunk;
use crate::text::{SourcePositions, atomic_text};

mod parser_facts;

/// Symbol chunks larger than this are split into line-window sub-chunks.
/// ~6000 bytes is comfortably above the code profile's 1400-token hard max
/// (`chunk_router::decision_for_profile`) at a rough 4 bytes/token estimate.
const MAX_SYMBOL_CHUNK_BYTES: usize = 6000;
/// Target sub-window size when a symbol chunk is split.
const SYMBOL_SPLIT_WINDOW_BYTES: usize = 3000;

struct LineSpan<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

pub(crate) fn code_manifest(text: &str, path: Option<&str>) -> Vec<DocumentChunk> {
    let language = detect_code_language(path, None);
    atomic_text(text)
        .into_iter()
        .map(|chunk| {
            chunk
                .with_metadata("manifest", true.into())
                .with_metadata("code_language", language.into())
                .with_metadata("code_file_type", "config".into())
                .with_metadata("code_chunk_source", "atomic_manifest".into())
                .with_metadata("actual_chunking_method", "atomic_manifest".into())
                .with_metadata("code_parse_status", "unsupported".into())
                .with_metadata("symbol_extraction_status", "none".into())
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn code_symbols(
    text: &str,
    path: Option<&str>,
    language_hint: Option<&str>,
) -> Vec<DocumentChunk> {
    code_symbols_with_facts(text, path, language_hint, &[])
}

pub(crate) fn code_symbols_with_facts(
    text: &str,
    path: Option<&str>,
    language_hint: Option<&str>,
    parse_facts: &[SourceParseFacts],
) -> Vec<DocumentChunk> {
    let positions = SourcePositions::new(text);
    let raw = if let Some(chunks) = parser_facts::parser_code_symbol_chunks(text, parse_facts) {
        chunks
    } else if let Some(chunks) = repomix_packed_code_symbols(text, &positions) {
        chunks
    } else {
        code_symbols_with_base(text, 0, &positions)
    };

    let language = detect_code_language(path, language_hint);
    let is_test = axon_parse::code_path::is_test_path(path);
    raw.into_iter()
        .flat_map(|chunk| split_if_huge(chunk, &positions))
        .map(|chunk| stamp_code_metadata(chunk, language, is_test))
        .collect()
}

fn stamp_code_metadata(
    mut chunk: DocumentChunk,
    language: &'static str,
    is_test: bool,
) -> DocumentChunk {
    let already_stamped = chunk.metadata.get("code_chunk_source").is_some();
    chunk = chunk.with_metadata("code_language", language.into());
    chunk = chunk.with_metadata("code_is_test", is_test.into());
    if !already_stamped {
        let (source, parse_status, extraction_status) = if chunk.symbol.is_some() {
            ("heuristic_symbol", "fallback", "fallback")
        } else if language == "unknown" {
            ("atomic_code", "unsupported", "unsupported")
        } else {
            ("atomic_code", "fallback", "none")
        };
        chunk = chunk
            .with_metadata("code_chunk_source", source.into())
            .with_metadata("actual_chunking_method", source.into())
            .with_metadata("code_parse_status", parse_status.into())
            .with_metadata("symbol_extraction_status", extraction_status.into());
    }
    chunk
}

/// Splits a chunk whose content exceeds [`MAX_SYMBOL_CHUNK_BYTES`] into
/// line-window sub-chunks, stamping the contract's fallback metadata
/// (`chunking_fallback`, `preferred_chunking_method`,
/// `actual_chunking_method`). Leaves small chunks untouched.
fn split_if_huge(chunk: DocumentChunk, positions: &SourcePositions<'_>) -> Vec<DocumentChunk> {
    if chunk.content.len() <= MAX_SYMBOL_CHUNK_BYTES {
        return vec![chunk];
    }
    let base = chunk.range.byte_start.unwrap_or(0) as usize;
    let symbol = chunk.symbol.clone();
    let windows = line_windows(&chunk.content, SYMBOL_SPLIT_WINDOW_BYTES);
    windows
        .into_iter()
        .enumerate()
        .filter_map(|(idx, (start, end))| {
            let content = &chunk.content[start..end];
            if content.is_empty() {
                return None;
            }
            let mut sub = DocumentChunk::new(
                content.to_string(),
                positions.source_range(base + start, base + end),
            )
            .with_metadata("code_chunk_source", "line_window".into())
            .with_metadata("code_parse_status", "partial".into())
            .with_metadata("symbol_extraction_status", "fallback".into())
            .with_metadata("chunking_fallback", "line_window".into())
            .with_metadata("preferred_chunking_method", "tree_sitter".into())
            .with_metadata("actual_chunking_method", "line_window".into());
            for (key, value) in &chunk.metadata.0 {
                sub.metadata
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            if let Some(symbol) = &symbol {
                sub = sub.with_symbol(format!("{symbol}#part{idx}"));
            }
            Some(sub)
        })
        .collect()
}

fn line_windows(text: &str, max_bytes: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut window_start = 0usize;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let next = offset + line.len();
        if next - window_start > max_bytes && offset > window_start {
            spans.push((window_start, offset));
            window_start = offset;
        }
        offset = next;
    }
    if window_start < text.len() {
        spans.push((window_start, text.len()));
    }
    spans
}

/// Best-effort language detection from a repo/local path and/or an adapter
/// language hint. Falls back to `"unknown"` -- a known, searchable value
/// distinct from a real detected language, per the contract's "unsupported
/// languages still produce line-aware chunks" rule.
fn detect_code_language(path: Option<&str>, language_hint: Option<&str>) -> &'static str {
    if let Some(hint) = language_hint.map(str::to_ascii_lowercase)
        && let Some(lang) = language_from_extension(&hint)
    {
        return lang;
    }
    let Some(path) = path else { return "unknown" };
    let filename = path.rsplit('/').next().unwrap_or(path);
    let ext = filename.rsplit('.').next().filter(|ext| *ext != filename);
    ext.and_then(|ext| language_from_extension(&ext.to_ascii_lowercase()))
        .unwrap_or("unknown")
}

fn language_from_extension(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "sh" | "bash" | "zsh" => "shell",
        "ps1" => "powershell",
        "dart" => "dart",
        "ex" | "exs" => "elixir",
        "swift" => "swift",
        "scala" => "scala",
        "lua" => "lua",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        _ => return None,
    })
}

fn code_symbols_with_base(
    section: &str,
    base: usize,
    positions: &SourcePositions<'_>,
) -> Vec<DocumentChunk> {
    let starts: Vec<usize> = section
        .lines()
        .scan(0usize, |offset, line| {
            let current = *offset;
            *offset += line.len() + 1;
            Some((current, line))
        })
        .filter(|(_, line)| looks_like_symbol(line))
        .map(|(offset, _)| offset)
        .collect();

    if starts.is_empty() {
        let content = section;
        if content.is_empty() {
            return Vec::new();
        }
        return vec![DocumentChunk::new(
            content.to_string(),
            positions.source_range(base, base + section.len()),
        )];
    }

    let mut boundaries = starts;
    if boundaries.first().copied() != Some(0) {
        boundaries.insert(0, 0);
    }
    boundaries.push(section.len());
    boundaries
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let content = &section[start..end];
            if content.is_empty() {
                return None;
            }
            let chunk = DocumentChunk::new(
                content.to_string(),
                positions.source_range(base + start, base + end),
            );
            let symbol = content.lines().next().and_then(symbol_name);
            Some(if let Some(symbol) = symbol {
                chunk.with_symbol(symbol)
            } else {
                chunk
            })
        })
        .collect()
}

fn repomix_packed_code_symbols(
    text: &str,
    positions: &SourcePositions<'_>,
) -> Option<Vec<DocumentChunk>> {
    let lines = line_spans(text);
    let markers: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let path = line.text.trim().strip_prefix("File: ")?;
            let has_divider = idx
                .checked_sub(1)
                .and_then(|prev| lines.get(prev))
                .is_some_and(|line| is_divider(line.text))
                || lines.get(idx + 1).is_some_and(|line| is_divider(line.text));
            (has_divider && !path.trim().is_empty()).then(|| (idx, path.trim().to_string()))
        })
        .collect();

    if markers.is_empty() {
        return None;
    }

    let mut chunks = Vec::new();
    for (marker_idx, (line_idx, path)) in markers.iter().enumerate() {
        let next_marker_line = markers
            .get(marker_idx + 1)
            .map(|(idx, _)| *idx)
            .unwrap_or(lines.len());
        let mut first = line_idx + 1;
        if lines.get(first).is_some_and(|line| is_divider(line.text)) {
            first += 1;
        }
        let mut last = next_marker_line;
        while last > first
            && lines
                .get(last - 1)
                .is_some_and(|line| line.text.trim().is_empty() || is_divider(line.text))
        {
            last -= 1;
        }
        if first >= last {
            continue;
        }

        let start = lines[first].start;
        let end = lines[last - 1].end;
        chunks.extend(
            code_symbols_with_base(&text[start..end], start, positions)
                .into_iter()
                .map(|chunk| chunk.with_metadata("original_path", path.clone().into())),
        );
    }

    (!chunks.is_empty()).then_some(chunks)
}

fn line_spans(text: &str) -> Vec<LineSpan<'_>> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        spans.push(LineSpan {
            start,
            end,
            text: line,
        });
        start = end;
    }
    spans
}

fn is_divider(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 8 && trimmed.chars().all(|ch| ch == '=' || ch == '-')
}

fn looks_like_symbol(line: &str) -> bool {
    let trimmed = line.trim_start();
    normalized_symbol_prefix(trimmed).is_some()
}

fn symbol_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_keyword = normalized_symbol_prefix(trimmed)?;
    Some(
        after_keyword
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .next()
            .unwrap_or("symbol")
            .to_string(),
    )
}

pub(crate) fn code_symbol_kind_for_content(content: &str) -> &'static str {
    let first = strip_rust_visibility(strip_js_ts_modifiers(
        content.lines().next().unwrap_or_default().trim_start(),
    ));
    if first.starts_with("pub fn ")
        || first.starts_with("fn ")
        || first.starts_with("def ")
        || first.starts_with("async function ")
        || first.starts_with("function ")
        || js_ts_assignment_function(first)
    {
        "function"
    } else if first.starts_with("class ")
        || first.starts_with("interface ")
        || first.starts_with("type ")
        || first.starts_with("struct ")
    {
        "type"
    } else if first.starts_with("enum ") {
        "enum"
    } else if first.starts_with("impl ") {
        "impl"
    } else {
        "symbol"
    }
}

fn normalized_symbol_prefix(line: &str) -> Option<&str> {
    let line = strip_rust_visibility(strip_js_ts_modifiers(line));
    [
        "pub fn ",
        "fn ",
        "def ",
        "class ",
        "struct ",
        "enum ",
        "impl ",
        "async function ",
        "function ",
        "interface ",
        "type ",
    ]
    .iter()
    .find_map(|prefix| line.strip_prefix(prefix))
    .or_else(|| js_ts_assignment_rest(line))
}

fn strip_rust_visibility(line: &str) -> &str {
    line.strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .or_else(|| line.strip_prefix("pub(super) "))
        .unwrap_or(line)
}

fn strip_js_ts_modifiers(mut line: &str) -> &str {
    loop {
        if let Some(stripped) = line.strip_prefix("export default ") {
            line = stripped;
        } else if let Some(stripped) = line.strip_prefix("export ") {
            line = stripped;
        } else if let Some(stripped) = line.strip_prefix("declare ") {
            line = stripped;
        } else if let Some(stripped) = line.strip_prefix("abstract ") {
            line = stripped;
        } else {
            return line;
        }
    }
}

fn js_ts_assignment_function(line: &str) -> bool {
    js_ts_assignment_rest(line).is_some()
}

fn js_ts_assignment_rest(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    let name_end = rest
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
        .last()
        .map(|(idx, ch)| idx + ch.len_utf8())?;
    let rhs = rest[name_end..].trim_start();
    (rhs.contains("=>") || rhs.contains("function") || rhs.contains("React.FC")).then_some(rest)
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
