//! Verifies bench artifacts in docs/perf/results-*.json contain ONLY numerical
//! measurements — no chunk_text, queries, answers, source URLs.
//!
//! The bench harness (scripts/bench-ask.sh) enforces this on write, but this
//! test catches any artifact that slips through (e.g. manual hand-edits).
//!
//! Skipped silently if no results files exist.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

const FORBIDDEN_KEYS: &[&str] = &["query", "answer", "chunk_text", "url", "source"];
const MAX_STRING_LEN: usize = 200;

fn results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/perf")
}

fn collect_results() -> Vec<PathBuf> {
    let dir = results_dir();
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    read.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("json")
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|n| n.starts_with("results-"))
                    .unwrap_or(false)
        })
        .collect()
}

fn check_value(value: &Value, path: &str, violations: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if s.parse::<f64>().is_err() {
                violations.push(format!("non-numeric string value at `{path}`"));
            }
            if s.len() > MAX_STRING_LEN {
                violations.push(format!(
                    "string at `{path}` is {} chars (max {MAX_STRING_LEN})",
                    s.len()
                ));
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                check_value(v, &format!("{path}[{i}]"), violations);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                if FORBIDDEN_KEYS.contains(&k.as_str()) {
                    violations.push(format!("forbidden key `{k}` at `{path}`"));
                }
                let next = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                check_value(v, &next, violations);
            }
        }
        _ => {}
    }
}

#[test]
fn bench_artifacts_contain_only_numerical_data() {
    let files = collect_results();
    if files.is_empty() {
        eprintln!("(no docs/perf/results-*.json found — skipping)");
        return;
    }

    let mut all_violations: Vec<String> = Vec::new();
    for path in &files {
        let raw =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed: Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let mut violations = Vec::new();
        check_value(&parsed, "", &mut violations);
        if !violations.is_empty() {
            all_violations.push(format!(
                "{}:\n  - {}",
                path.display(),
                violations.join("\n  - ")
            ));
        }
    }

    assert!(
        all_violations.is_empty(),
        "bench artifacts contain non-numerical data:\n{}",
        all_violations.join("\n")
    );
}
