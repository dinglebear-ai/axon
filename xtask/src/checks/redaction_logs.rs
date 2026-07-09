use anyhow::{Result, bail};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::{DirEntry, WalkDir};

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".claude",
    "target",
    "node_modules",
    ".cache",
    ".next",
    "vendor",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    path: PathBuf,
    line: usize,
    identifier: String,
    call: String,
}

fn is_skipped_dir(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map(|name| SKIP_DIRS.contains(&name))
            .unwrap_or(false)
}

fn logging_call_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(?:tracing::(?:trace|debug|info|warn|error)!|log::(?:trace|debug|info|warn|error)!|log_(?:trace|debug|info|warn|error)\s*\()",
        )
        .expect("valid logging call regex")
    })
}

fn sensitive_identifier_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:api[_-]?key|authorization|auth[_-]?header|bearer[_-]?token|client[_-]?secret|cookie|credential|jwt|oauth[_-]?token|password|passwd|private[_-]?key|refresh[_-]?token|secret|session[_-]?(?:key|secret|token)|token|x[_-]?api[_-]?key)\b",
        )
        .expect("valid sensitive identifier regex")
    })
}

fn has_redaction_marker(call: &str) -> bool {
    call.contains("redact")
        || call.contains("Redactor")
        || call.contains("RedactionContext")
        || call.contains("<redacted")
        || call.contains("[redacted")
}

fn is_test_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|name| name == "test" || name == "tests")
            .unwrap_or(false)
    }) || path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name == "test.rs"
                || name == "tests.rs"
                || name.ends_with("_test.rs")
                || name.ends_with("_tests.rs")
        })
        .unwrap_or(false)
}

fn call_surface_for_identifier_scan(call: &str) -> String {
    let mut out = String::with_capacity(call.len());
    let mut chars = call.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut string_text = String::new();

    while let Some(ch) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
                string_text.push(ch);
                continue;
            }
            match ch {
                '\\' => {
                    escaped = true;
                    string_text.push(ch);
                }
                '"' => {
                    for capture in format_identifier_regex().captures_iter(&string_text) {
                        out.push_str(capture.get(1).map(|m| m.as_str()).unwrap_or_default());
                        out.push(' ');
                    }
                    string_text.clear();
                    in_string = false;
                }
                _ => string_text.push(ch),
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }
        out.push(ch);
    }

    out
}

fn format_identifier_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("valid format regex"))
}

fn find_unredacted_logging_calls(path: &Path, source: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut current = String::new();
    let mut start_line = 0usize;
    let mut collecting = false;

    for (idx, line) in source.lines().enumerate() {
        let line_no = idx + 1;
        if !collecting && logging_call_regex().is_match(line) {
            collecting = true;
            start_line = line_no;
            current.clear();
        }

        if collecting {
            current.push_str(line);
            current.push('\n');
            if line.contains(';') {
                let scan_surface = call_surface_for_identifier_scan(&current);
                if let Some(found) = sensitive_identifier_regex().find(&scan_surface) {
                    if !has_redaction_marker(&current) {
                        findings.push(Finding {
                            path: path.to_path_buf(),
                            line: start_line,
                            identifier: found.as_str().to_string(),
                            call: current.trim().to_string(),
                        });
                    }
                }
                collecting = false;
            }
        }
    }

    findings
}

pub fn check(root: &Path) -> Result<()> {
    let mut findings = Vec::new();
    let walker = WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_skipped_dir(entry));

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
        {
            continue;
        }

        let source = fs::read_to_string(entry.path())?;
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();
        if is_test_path(&rel) {
            continue;
        }
        findings.extend(find_unredacted_logging_calls(&rel, &source));
    }

    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));

    if !findings.is_empty() {
        eprintln!("ERROR: sensitive-looking log call sites must route through the redaction gate:");
        for finding in &findings {
            eprintln!(
                "  {}:{} contains `{}`",
                finding.path.display(),
                finding.line,
                finding.identifier
            );
        }
        eprintln!();
        eprintln!(
            "Use axon_core::redact helpers or another explicit redaction wrapper before logging."
        );
        bail!(
            "found {} unredacted sensitive log call site(s)",
            findings.len()
        );
    }

    println!("OK: sensitive log call sites route through redaction.");
    Ok(())
}

#[cfg(test)]
#[path = "redaction_logs_tests.rs"]
mod tests;
