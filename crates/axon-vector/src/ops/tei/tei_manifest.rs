use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Entry in the manifest URL map: (url, changed, structured_blob).
/// `structured_blob` is `None` when no structured data was extracted from the
/// page at crawl time (bead axon_rust-jej7.2).
pub(super) type ManifestEntry = (String, bool, Option<serde_json::Value>);

pub(super) fn read_manifest_url_map(markdown_dir: &Path) -> HashMap<PathBuf, ManifestEntry> {
    let Some(parent) = markdown_dir.parent() else {
        return HashMap::new();
    };
    let manifest = parent.join("manifest.jsonl");
    let file = match std::fs::File::open(&manifest) {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let url = match parsed.get("url").and_then(|v| v.as_str()) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => continue,
        };
        let changed = parsed
            .get("changed")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        // Structured-data blob written by process_page() (bead axon_rust-jej7.2).
        // Absent on manifest entries from before this bead or when no structured
        // data was found on the page. Passed through to PreparedDoc for Qdrant.
        let structured = parsed.get("structured").cloned();

        let normalized = if let Some(rel) = parsed.get("relative_path").and_then(|v| v.as_str()) {
            parent.join(rel)
        } else if let Some(abs) = parsed.get("file_path").and_then(|v| v.as_str()) {
            std::fs::canonicalize(abs).unwrap_or_else(|_| PathBuf::from(abs))
        } else {
            continue;
        };
        out.insert(normalized, (url, changed, structured));
    }
    out
}

#[cfg(test)]
#[path = "tei_manifest_tests.rs"]
mod tests;
