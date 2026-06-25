//! Normalize, noise-filter, and hash scraped markdown before diffing.

use regex::Regex;
use sha2::{Digest, Sha256};

/// Normalize line endings, strip trailing whitespace, collapse blank-line runs,
/// trim leading/trailing blanks. Conservative — whitespace only, no restructure.
pub fn normalize_markdown(md: &str) -> String {
    let unified = md.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in unified.lines() {
        let trimmed = line.trim_end().to_string();
        let is_blank = trimmed.is_empty();
        if is_blank && prev_blank {
            continue;
        }
        out.push(trimmed);
        prev_blank = is_blank;
    }
    out.join("\n").trim_matches('\n').to_string()
}

/// Compile user-supplied ignore patterns, surfacing a clear error on bad regex.
pub fn compile_patterns(patterns: &[String]) -> Result<Vec<Regex>, String> {
    patterns
        .iter()
        .map(|p| Regex::new(p).map_err(|e| format!("invalid ignore_pattern '{p}': {e}")))
        .collect()
}

/// Drop lines matching any ignore pattern (e.g. "Last updated: …").
pub fn apply_ignore(md: &str, patterns: &[Regex]) -> String {
    if patterns.is_empty() {
        return md.to_string();
    }
    md.lines()
        .filter(|line| !patterns.iter().any(|re| re.is_match(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// SHA-256 hex of the input (caller passes already-normalized+filtered text).
pub fn content_hash(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
