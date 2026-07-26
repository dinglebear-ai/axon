//! Fail-closed layering guardrail for the live pipeline.
//!
//! Transport crates may not depend on provider/domain crates or reach private
//! implementation modules. Transport and service code may not access raw
//! provider traits, handles, or reserved operations outside the one fixed
//! scheduler facade. Rust is parsed with `syn`, so grouped/multiline/renamed
//! imports and UFCS paths are inspected while comments and strings are ignored.
//!
//! Every temporary exception is an exact `(path, rule, owner, expected_count)`
//! record (plus dependency table for Cargo exceptions). Count drift, stale
//! exceptions, duplicate exceptions, and missing owners all fail the check.

mod exception_table;
mod exceptions;
mod syntax;

use std::path::Path;

use anyhow::{Result, bail};
use exception_table::{MANIFEST_EXCEPTIONS, REACH_EXCEPTIONS};
pub(crate) use exceptions::{ManifestException, ReachException};
use exceptions::{apply_manifest_exceptions, apply_reach_exceptions};
use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub(super) struct Finding {
    path: String,
    rule: String,
    detail: String,
}

#[derive(Clone, Debug)]
pub(super) struct ManifestFinding {
    path: String,
    dependency: String,
    table: String,
}

#[derive(Clone, Copy)]
pub(super) struct ReachRule {
    prefix: &'static [&'static str],
    kind: &'static str,
}

const TRANSPORT_REACH_RULES: &[ReachRule] = &[
    ReachRule {
        prefix: &["axon_adapters", "web_engine"],
        kind: "domain internal",
    },
    ReachRule {
        prefix: &["axon_llm"],
        kind: "provider crate",
    },
    ReachRule {
        prefix: &["axon_services", "source", "execution"],
        kind: "service source internal",
    },
    ReachRule {
        prefix: &["axon_services", "source", "events"],
        kind: "service source internal",
    },
    ReachRule {
        prefix: &["axon_services", "source", "progress"],
        kind: "service source internal",
    },
    ReachRule {
        prefix: &["axon_vectors", "qdrant"],
        kind: "domain internal",
    },
    ReachRule {
        prefix: &["axon_prune", "executor"],
        kind: "domain internal",
    },
    ReachRule {
        prefix: &["axon_extract", "verticals"],
        kind: "domain internal",
    },
];

const SERVICES_REACH_RULES: &[ReachRule] = &[ReachRule {
    prefix: &["axon_adapters", "web_engine"],
    kind: "domain internal",
}];

const TRANSPORT_SRC: &[&str] = &[
    "crates/axon-cli/src",
    "crates/axon-web/src",
    "crates/axon-mcp/src",
];
const SERVICES_SRC: &[&str] = &["crates/axon-services/src"];
const SURFACE_MANIFESTS: &[&str] = &[
    "crates/axon-cli/Cargo.toml",
    "crates/axon-web/Cargo.toml",
    "crates/axon-mcp/Cargo.toml",
];
const TRANSPORT_FORBIDDEN_DEPS: &[&str] = &[
    "axon-adapters",
    "axon-embedding",
    "axon-llm",
    "axon-retrieval",
    "axon-vectors",
];

const RESERVED_CALL_FACADE: &str = "crates/axon-services/src/reserved_call.rs";

fn is_test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.split('/').any(|component| component == "tests")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
}

fn collect_manifest_findings(root: &Path, violations: &mut Vec<String>) -> Vec<ManifestFinding> {
    let mut findings = Vec::new();
    for manifest in SURFACE_MANIFESTS {
        let path = root.join(manifest);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                violations.push(format!("{manifest}: failed to read manifest: {error}"));
                continue;
            }
        };
        let parsed = match toml::from_str::<toml::Table>(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                violations.push(format!("{manifest}: failed to parse manifest: {error}"));
                continue;
            }
        };
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(table) = parsed.get(table_name).and_then(toml::Value::as_table) else {
                continue;
            };
            for dependency in TRANSPORT_FORBIDDEN_DEPS {
                if table.contains_key(*dependency) {
                    findings.push(ManifestFinding {
                        path: (*manifest).to_owned(),
                        dependency: (*dependency).to_owned(),
                        table: table_name.to_owned(),
                    });
                }
            }
        }
    }
    findings
}

fn collect_rust_findings(
    root: &Path,
    src_roots: &[&str],
    reach_rules: &[ReachRule],
    violations: &mut Vec<String>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for src in src_roots {
        let dir = root.join(src);
        for entry in WalkDir::new(&dir) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    violations.push(format!("{src}: failed to walk source tree: {error}"));
                    continue;
                }
            };
            if !entry.file_type().is_file()
                || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
            {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if is_test_file(&rel) {
                continue;
            }
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    violations.push(format!("{rel}: failed to read Rust source: {error}"));
                    continue;
                }
            };
            let parsed = match syn::parse_file(&text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    violations.push(format!("{rel}: failed to parse Rust source: {error}"));
                    continue;
                }
            };
            let mut file_findings = syntax::scan(&parsed, &rel, reach_rules);
            if rel == RESERVED_CALL_FACADE {
                file_findings.retain(|finding| !finding.rule.starts_with("provider-"));
            }
            findings.extend(file_findings);
        }
    }
    findings
}

fn check_with_exceptions(
    root: &Path,
    reach_exceptions: &[ReachException],
    manifest_exceptions: &[ManifestException],
) -> Result<()> {
    let mut violations = Vec::new();
    let manifest_findings = collect_manifest_findings(root, &mut violations);
    apply_manifest_exceptions(manifest_findings, manifest_exceptions, &mut violations);

    let mut rust_findings =
        collect_rust_findings(root, TRANSPORT_SRC, TRANSPORT_REACH_RULES, &mut violations);
    rust_findings.extend(collect_rust_findings(
        root,
        SERVICES_SRC,
        SERVICES_REACH_RULES,
        &mut violations,
    ));
    apply_reach_exceptions(rust_findings, reach_exceptions, &mut violations);

    if violations.is_empty() {
        println!("OK: live transport/domain layering and reserved-call gate pass.");
        return Ok(());
    }
    bail!(
        "layering violation: {} issue(s)\n{}",
        violations.len(),
        violations.join("\n")
    )
}

pub fn check(root: &Path) -> Result<()> {
    check_with_exceptions(root, REACH_EXCEPTIONS, MANIFEST_EXCEPTIONS)
}

#[cfg(test)]
pub(super) fn check_fixture(root: &Path) -> Result<()> {
    check_with_exceptions(root, &[], &[])
}

#[cfg(test)]
pub(super) fn check_fixture_with_exceptions(
    root: &Path,
    reach_exceptions: &[ReachException],
    manifest_exceptions: &[ManifestException],
) -> Result<()> {
    check_with_exceptions(root, reach_exceptions, manifest_exceptions)
}
