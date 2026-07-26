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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
    let workspace_dependencies = read_workspace_dependencies(root);
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
        for (table_name, table) in dependency_tables(&parsed) {
            for (declared_name, declaration) in table {
                let dependency = match canonical_dependency_name(
                    declared_name,
                    declaration,
                    &workspace_dependencies,
                ) {
                    Ok(dependency) => dependency,
                    Err(error) => {
                        violations.push(format!(
                            "{manifest}: failed to resolve [{table_name}] dependency \
                             `{declared_name}`: {error}"
                        ));
                        continue;
                    }
                };
                if TRANSPORT_FORBIDDEN_DEPS.contains(&dependency.as_str()) {
                    findings.push(ManifestFinding {
                        path: (*manifest).to_owned(),
                        dependency,
                        table: table_name.clone(),
                    });
                }
            }
        }
    }
    findings
}

fn read_workspace_dependencies(root: &Path) -> Result<toml::Table, String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let parsed = toml::from_str::<toml::Table>(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(parsed
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default())
}

fn canonical_dependency_name(
    declared_name: &str,
    value: &toml::Value,
    workspace_dependencies: &Result<toml::Table, String>,
) -> Result<String, String> {
    let Some(table) = value.as_table() else {
        return Ok(declared_name.to_owned());
    };
    let inherited = match table.get("workspace") {
        Some(value) => value.as_bool().ok_or_else(|| {
            format!(
                "`workspace` must be a boolean, found {}",
                toml_value_kind(value)
            )
        })?,
        None => false,
    };
    if !inherited {
        return optional_package_name(table, declared_name);
    }

    let workspace_dependencies = workspace_dependencies
        .as_ref()
        .map_err(|error| format!("cannot load inherited workspace dependency: {error}"))?;
    let inherited = workspace_dependencies.get(declared_name).ok_or_else(|| {
        format!("workspace dependency `{declared_name}` is missing from [workspace.dependencies]")
    })?;
    if inherited.is_str() {
        return Ok(declared_name.to_owned());
    }
    let inherited_table = inherited.as_table().ok_or_else(|| {
        format!(
            "workspace dependency `{declared_name}` must be a string or table, found {}",
            toml_value_kind(inherited)
        )
    })?;
    if inherited_table.get("workspace").is_some() {
        return Err(format!(
            "workspace dependency `{declared_name}` cannot itself use `workspace` inheritance"
        ));
    }
    optional_package_name(inherited_table, declared_name)
}

fn optional_package_name(table: &toml::Table, declared_name: &str) -> Result<String, String> {
    match table.get("package") {
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(|| {
            format!(
                "`package` must be a string, found {}",
                toml_value_kind(value)
            )
        }),
        None => Ok(declared_name.to_owned()),
    }
}

fn toml_value_kind(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Datetime(_) => "datetime",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
    }
}

fn dependency_tables(parsed: &toml::Table) -> Vec<(String, &toml::Table)> {
    let mut tables = Vec::new();
    for dependency_kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = parsed.get(dependency_kind).and_then(toml::Value::as_table) {
            tables.push((dependency_kind.to_owned(), table));
        }
    }
    if let Some(targets) = parsed.get("target").and_then(toml::Value::as_table) {
        for (target_name, target) in targets {
            let Some(target) = target.as_table() else {
                continue;
            };
            for dependency_kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(table) = target.get(dependency_kind).and_then(toml::Value::as_table) {
                    tables.push((format!("target.'{target_name}'.{dependency_kind}"), table));
                }
            }
        }
    }
    tables
}

struct ParsedRustFile {
    path: PathBuf,
    rel: String,
    syntax: syn::File,
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
        let mut parsed_files = Vec::new();
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
            parsed_files.push(ParsedRustFile {
                path: path.to_owned(),
                rel,
                syntax: parsed,
            });
        }

        let excluded_modules = test_only_external_module_paths(&parsed_files);
        for file in parsed_files {
            if excluded_modules.contains(&file.path) {
                continue;
            }
            let mut file_findings = syntax::scan(&file.syntax, &file.rel, reach_rules);
            if file.rel == RESERVED_CALL_FACADE {
                file_findings.retain(|finding| !finding.rule.starts_with("provider-"));
            }
            findings.extend(file_findings);
        }
    }
    findings
}

fn test_only_external_module_paths(files: &[ParsedRustFile]) -> BTreeSet<PathBuf> {
    let mut production = BTreeSet::new();
    let mut test = BTreeSet::new();
    for file in files {
        for module in syntax::external_modules(&file.syntax, &file.path) {
            let path = module.path;
            if module.test_only {
                test.insert(path);
            } else {
                production.insert(path);
            }
        }
    }
    test.difference(&production).cloned().collect()
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
