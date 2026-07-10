//! Record-level chunkers for structured formats beyond JSON: YAML, TOML,
//! CSV, and a lightweight XML top-level-element splitter. Every chunk here
//! carries a real byte-accurate `source_range` (unlike the JSON branch in
//! `metadata.rs`, which chunks a parsed `serde_json::Value` and can only
//! stamp a synthetic range). Contract:
//! `docs/pipeline-unification/sources/chunking-contract.md`
//! "Structured Record Chunking".

use crate::chunk::DocumentChunk;
use crate::text::source_range;

/// Splits YAML at top-level (column-0) keys/list items. Nested content
/// (anything indented) stays attached to its owning top-level record.
pub(crate) fn yaml_records(text: &str) -> Vec<DocumentChunk> {
    let starts = top_level_line_starts(text, |line| {
        !line.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !line.starts_with('#')
    });
    sectioned_chunks(text, starts, |content| {
        let key = content
            .lines()
            .next()
            .map(|line| {
                line.trim_start_matches('-')
                    .trim()
                    .split(':')
                    .next()
                    .unwrap_or(line)
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        ("yaml_path", key)
    })
}

/// Splits TOML at top-level `[table]` / `[[array_of_tables]]` headers. Any
/// content before the first header becomes an implicit root record.
pub(crate) fn toml_records(text: &str) -> Vec<DocumentChunk> {
    let starts = top_level_line_starts(text, |line| line.starts_with('['));
    sectioned_chunks(text, starts, |content| {
        let table = content
            .lines()
            .next()
            .filter(|line| line.starts_with('['))
            .map(|line| line.trim_matches(['[', ']']).to_string())
            .unwrap_or_else(|| "root".to_string());
        ("toml_table", table)
    })
}

/// Splits CSV into one chunk per data row (header excluded from chunk
/// content but used to build a searchable `col: value` record). `csv_row`
/// on the range is the 0-based data-row index (header is not counted).
pub(crate) fn csv_records(text: &str) -> Vec<DocumentChunk> {
    let mut lines = line_offsets(text);
    if lines.is_empty() {
        return Vec::new();
    }
    let header_line = lines.remove(0);
    let headers: Vec<String> = split_csv_line(header_line.text)
        .into_iter()
        .map(|field| field.trim().to_string())
        .collect();

    lines
        .into_iter()
        .enumerate()
        .filter_map(|(row_idx, line)| {
            if line.text.trim().is_empty() {
                return None;
            }
            let fields = split_csv_line(line.text);
            let record = headers
                .iter()
                .zip(fields.iter())
                .map(|(header, value)| format!("{header}: {}", value.trim()))
                .collect::<Vec<_>>()
                .join("\n");
            if record.is_empty() {
                return None;
            }
            let mut range = source_range(text, line.start, line.end);
            range.csv_row = Some(row_idx as u32);
            Some(
                DocumentChunk::new(record, range)
                    .with_metadata("structured_record_kind", "csv_row".into()),
            )
        })
        .collect()
}

/// Splits XML into one chunk per top-level child element of the document's
/// root element. Depth tracking is tag-name-agnostic (counts opens/closes),
/// not a validating parser -- malformed XML degrades to an empty result,
/// which callers treat as a parse failure and fall back to atomic chunking.
pub(crate) fn xml_records(text: &str) -> Vec<DocumentChunk> {
    let Some(root_open_end) = text.find('>').map(|idx| idx + 1) else {
        return Vec::new();
    };
    let root_start = if text[..root_open_end].starts_with("<?xml") {
        match text[root_open_end..].find('>') {
            Some(idx) => root_open_end + idx + 1,
            None => return Vec::new(),
        }
    } else {
        root_open_end
    };

    let mut chunks = Vec::new();
    let mut depth = 0i32;
    let mut record_start: Option<usize> = None;
    let mut idx = root_start;
    let bytes = text.as_bytes();
    while idx < text.len() {
        if bytes[idx] != b'<' {
            idx += 1;
            continue;
        }
        let Some(tag_end) = text[idx..].find('>').map(|offset| idx + offset + 1) else {
            break;
        };
        let tag = &text[idx..tag_end];
        let is_closing = tag.starts_with("</");
        let is_self_closing = tag.ends_with("/>") || tag.starts_with("<!") || tag.starts_with("<?");

        if !is_closing && !is_self_closing {
            if depth == 0 {
                record_start = Some(idx);
            }
            depth += 1;
        } else if is_closing {
            depth -= 1;
            if depth == 0
                && let Some(start) = record_start.take()
            {
                let content = text[start..tag_end].to_string();
                let name = element_name(&text[start..]);
                let mut range = source_range(text, start, tag_end);
                range.xml_xpath = Some(format!("/*/{name}"));
                chunks.push(
                    DocumentChunk::new(content, range)
                        .with_metadata("structured_record_kind", "xml_element".into()),
                );
            }
            if depth < 0 {
                break;
            }
        } else if depth == 0 {
            let content = text[idx..tag_end].to_string();
            let name = element_name(&text[idx..]);
            let mut range = source_range(text, idx, tag_end);
            range.xml_xpath = Some(format!("/*/{name}"));
            chunks.push(
                DocumentChunk::new(content, range)
                    .with_metadata("structured_record_kind", "xml_element".into()),
            );
        }
        idx = tag_end;
    }
    chunks
}

fn element_name(tag_and_rest: &str) -> String {
    tag_and_rest
        .trim_start_matches('<')
        .trim_start_matches('/')
        .split(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
        .next()
        .unwrap_or("element")
        .to_string()
}

struct LineOffset<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

fn line_offsets(text: &str) -> Vec<LineOffset<'_>> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        lines.push(LineOffset {
            start,
            end,
            text: line.trim_end_matches('\n').trim_end_matches('\r'),
        });
        start = end;
    }
    lines
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

fn top_level_line_starts(text: &str, is_start: impl Fn(&str) -> bool) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        if is_start(line.trim_end_matches('\n').trim_end_matches('\r')) {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts
}

fn sectioned_chunks(
    text: &str,
    mut starts: Vec<usize>,
    path_for: impl Fn(&str) -> (&'static str, String),
) -> Vec<DocumentChunk> {
    if starts.first().copied() != Some(0) {
        starts.insert(0, 0);
    }
    starts.push(text.len());
    starts
        .windows(2)
        .filter_map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let content = text[start..end].trim();
            if content.is_empty() {
                return None;
            }
            let (field, value) = path_for(content);
            let mut range = source_range(text, start, end);
            if field == "yaml_path" {
                range.yaml_path = Some(value.clone());
            }
            let mut chunk = DocumentChunk::new(content.to_string(), range)
                .with_metadata("structured_record_kind", "top_level_record".into());
            if field == "toml_table" {
                chunk = chunk.with_metadata("toml_table", value.into());
            }
            Some(chunk)
        })
        .collect()
}

#[cfg(test)]
#[path = "structured_formats_tests.rs"]
mod tests;
