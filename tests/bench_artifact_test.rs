//! Verifies bench artifacts in docs/perf/results-*.json contain only safe
//! metadata plus numerical/boolean timing measurements — no chunk_text, queries,
//! answers, source URLs, or long strings.
//!
//! The bench harness (scripts/bench-ask.sh) enforces this on write, but this
//! test catches any artifact that slips through (e.g. manual hand-edits).
//!
//! Skipped silently if no results files exist.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

const FORBIDDEN_KEYS: &[&str] = &[
    "query",
    "answer",
    "chunk_text",
    "url",
    "source",
    "prompt",
    "prompt_text",
];
const MAX_STRING_LEN: usize = 100;

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

fn is_allowed_metadata_string(path: &str) -> bool {
    matches!(
        path,
        "schema" | "backend" | "timestamp_utc" | "git_sha" | "git_branch"
    ) || path.ends_with(".backend")
        || path.ends_with(".prompt_id")
        || path.ends_with(".mode")
        || path.ends_with(".status")
}

fn check_value(value: &Value, path: &str, in_timing: bool, violations: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if in_timing && s.parse::<f64>().is_err() {
                violations.push(format!("non-numeric string value at `{path}`"));
            }
            if !in_timing && !is_allowed_metadata_string(path) {
                violations.push(format!("unexpected string metadata at `{path}`"));
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
                check_value(v, &format!("{path}[{i}]"), in_timing, violations);
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
                let child_in_timing = in_timing || next.contains(".timings[");
                check_value(v, &next, child_in_timing, violations);
            }
        }
        _ => {}
    }
}

fn artifact_violations(parsed: &Value) -> Vec<String> {
    let mut violations = Vec::new();
    check_value(parsed, "", false, &mut violations);
    violations
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
        let violations = artifact_violations(&parsed);
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

#[test]
fn bench_artifact_schema_allows_safe_metadata_and_timing_bools() {
    let artifact = serde_json::json!({
        "schema": "axon-bench-ask/v2",
        "backend": "gemini-headless",
        "timestamp_utc": "2026-05-16T00:00:00Z",
        "git_sha": "0123456789abcdef",
        "git_branch": "feat/ask-perf",
        "runs_per_prompt": 30,
        "results": [{
            "backend": "gemini-headless",
            "prompt_id": "nl-canonical",
            "mode": "cold",
            "status": "measured",
            "runs_requested": 30,
            "samples": 30,
            "timings": [{
                "retrieval": 10,
                "context_build": 20,
                "tei_embed_ms": 3,
                "full_doc_fetch_ms": 4,
                "streamed": true,
                "total": 42
            }]
        }]
    });

    assert!(artifact_violations(&artifact).is_empty());
}

#[test]
fn bench_artifact_schema_rejects_prompt_text_and_timing_strings() {
    let artifact = serde_json::json!({
        "schema": "axon-bench-ask/v2",
        "backend": "gemini-headless",
        "results": [{
            "prompt": "How does Qdrant reciprocal rank fusion combine dense and sparse vectors?",
            "timings": [{ "total": "slow" }]
        }]
    });

    let violations = artifact_violations(&artifact);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("forbidden key `prompt`")),
        "expected forbidden prompt key violation, got {violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("non-numeric string value")),
        "expected timing string violation, got {violations:?}"
    );
}
