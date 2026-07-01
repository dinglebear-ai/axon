//! Code-oriented chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::{atomic_text, source_range};

pub fn code_manifest(text: &str) -> Vec<DocumentChunk> {
    atomic_text(text)
        .into_iter()
        .map(|chunk| chunk.with_metadata("manifest", true.into()))
        .collect()
}

pub fn code_symbols(text: &str) -> Vec<DocumentChunk> {
    let starts: Vec<usize> = text
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
        return atomic_text(text);
    }

    let mut boundaries = starts;
    boundaries.push(text.len());
    boundaries
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let content = text[start..end].trim();
            if content.is_empty() {
                return None;
            }
            let symbol = content
                .lines()
                .next()
                .and_then(symbol_name)
                .unwrap_or_else(|| format!("symbol_at_{start}"));
            Some(
                DocumentChunk::new(content.to_string(), source_range(text, start, end))
                    .with_symbol(symbol),
            )
        })
        .collect()
}

fn looks_like_symbol(line: &str) -> bool {
    let trimmed = line.trim_start();
    [
        "fn ", "pub fn ", "def ", "class ", "struct ", "enum ", "impl ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn symbol_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_keyword = [
        "pub fn ", "fn ", "def ", "class ", "struct ", "enum ", "impl ",
    ]
    .iter()
    .find_map(|prefix| trimmed.strip_prefix(prefix))?;
    Some(
        after_keyword
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .next()
            .unwrap_or("symbol")
            .to_string(),
    )
}
