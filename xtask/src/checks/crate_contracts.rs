//! `cargo xtask check-crate-contracts` — audits real crate structure against
//! the per-crate implementation contracts in
//! `docs/pipeline-unification/crates/<name>/README.md`.
//!
//! Four assertions, derived from the README text and the completed vertical
//! ownership cutover (see
//! `crate_contracts_spec.rs` for how each crate's data was extracted):
//!
//! 1. Every documented "Public Modules" file exists under `src/` and is
//!    declared `pub mod <name>;` in `lib.rs`. This is one-directional
//!    (documented ⊆ actual) — real crates legitimately grow additional
//!    modules, sidecar `_tests.rs` files, and nested submodule directories
//!    beyond the flat contract list, and that is not drift.
//! 2. No crate's `[dependencies]` table (dev/build dependencies are exempt —
//!    fixtures legitimately cross boundaries production code must not)
//!    declares a dependency its README's "Dependencies Forbidden" section
//!    rules out.
//! 3. Adapter-owned vertical routing keeps its intentional one-way production
//!    edges: `axon-adapters` depends on both `axon-extract` implementations and
//!    `axon-parse` artifacts, while neither lower crate may depend back on
//!    `axon-adapters`.
//! 4. The static 23-crate contract inventory exactly matches both root
//!    `[workspace].members` and Cargo manifests under `crates/`; duplicate
//!    contract rows and count drift are rejected.
//!
//! The check is also part of the `cargo xtask check` aggregate. Run it
//! explicitly with `cargo xtask check-crate-contracts`.

use std::path::Path;

use anyhow::{Result, bail};

pub use super::crate_contracts_spec::{CrateContract, LIVE_CRATE_NAMES, all_crate_contracts};

const ADAPTER_VERTICAL_DEPS: &[&str] = &["axon-extract", "axon-parse"];

pub fn check(root: &Path) -> Result<()> {
    let mut violations = Vec::new();
    let mut checked = 0usize;
    for contract in all_crate_contracts() {
        checked += 1;
        check_modules(root, contract, &mut violations);
        check_forbidden_deps(root, contract, &mut violations);
    }
    check_live_crate_inventory(root, &mut violations);
    check_adapter_vertical_boundary(root, &mut violations);

    if violations.is_empty() {
        println!(
            "check-crate-contracts: OK — {checked} crate(s) match their pipeline-unification contract."
        );
        return Ok(());
    }

    bail!(
        "check-crate-contracts: {} violation(s) against docs/pipeline-unification/crates/<name>/README.md:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

fn check_live_crate_inventory(root: &Path, violations: &mut Vec<String>) {
    let contract_rows = all_crate_contracts()
        .map(|contract| contract.name.to_owned())
        .collect::<Vec<_>>();
    let workspace_members = workspace_crate_members(root, violations);
    let on_disk = on_disk_crates(root, violations);
    compare_live_inventory(&contract_rows, &workspace_members, &on_disk, violations);
}

fn workspace_crate_members(root: &Path, violations: &mut Vec<String>) -> Vec<String> {
    let manifest_path = root.join("Cargo.toml");
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => {
            violations.push(format!("failed to read root Cargo.toml: {error}"));
            return Vec::new();
        }
    };
    let parsed = match toml::from_str::<toml::Table>(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            violations.push(format!("failed to parse root Cargo.toml: {error}"));
            return Vec::new();
        }
    };
    let Some(members) = parsed
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
    else {
        violations.push("root Cargo.toml has no [workspace].members array".to_owned());
        return Vec::new();
    };
    let mut crates = Vec::new();
    for member in members {
        let Some(member) = member.as_str() else {
            violations.push("root [workspace].members contains a non-string entry".to_owned());
            continue;
        };
        if let Some(name) = member.strip_prefix("crates/")
            && !name.is_empty()
            && !name.contains('/')
        {
            crates.push(name.to_owned());
        }
    }
    crates
}

fn on_disk_crates(root: &Path, violations: &mut Vec<String>) -> Vec<String> {
    let crates_dir = root.join("crates");
    let entries = match std::fs::read_dir(&crates_dir) {
        Ok(entries) => entries,
        Err(error) => {
            violations.push(format!(
                "failed to read live crate directory {crates_dir:?}: {error}"
            ));
            return Vec::new();
        }
    };
    let mut crates = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                violations.push(format!("failed to read entry under crates/: {error}"));
                continue;
            }
        };
        if entry.path().join("Cargo.toml").is_file() {
            match entry.file_name().into_string() {
                Ok(name) => crates.push(name),
                Err(_) => violations.push("crates/ contains a non-UTF-8 crate name".to_owned()),
            }
        }
    }
    crates
}

fn compare_live_inventory(
    contract_rows: &[String],
    workspace_members: &[String],
    on_disk: &[String],
    violations: &mut Vec<String>,
) {
    use std::collections::{BTreeMap, BTreeSet};

    let expected = LIVE_CRATE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    for (label, rows) in [
        ("crate-contract", contract_rows),
        ("workspace member", workspace_members),
        ("on-disk crate", on_disk),
    ] {
        let mut counts = BTreeMap::new();
        for row in rows {
            *counts.entry(row).or_insert(0usize) += 1;
        }
        for (name, count) in counts.iter().filter(|(_, count)| **count > 1) {
            violations.push(format!(
                "{label} inventory has duplicate `{name}` ({count} rows)"
            ));
        }
        let found = counts
            .keys()
            .map(|name| (*name).clone())
            .collect::<BTreeSet<_>>();
        if found != expected {
            violations.push(format!(
                "{label} inventory differs from the 23 live crates: expected {expected:?}, found {found:?}"
            ));
        }
    }
    if contract_rows.len() != LIVE_CRATE_NAMES.len() {
        violations.push(format!(
            "crate-contract row count differs from the exact live inventory: expected {}, found {}",
            LIVE_CRATE_NAMES.len(),
            contract_rows.len()
        ));
    }
}

fn check_adapter_vertical_boundary(root: &Path, violations: &mut Vec<String>) {
    let Some(adapter_deps) = normal_dependencies(root, "axon-adapters", violations) else {
        return;
    };
    for dependency in ADAPTER_VERTICAL_DEPS {
        if !adapter_deps.contains_key(*dependency) {
            violations.push(format!(
                "axon-adapters: missing required one-way vertical dependency `{dependency}`"
            ));
        }
        let Some(reverse_deps) = normal_dependencies(root, dependency, violations) else {
            continue;
        };
        if reverse_deps.contains_key("axon-adapters") {
            violations.push(format!(
                "{dependency}: must not depend on `axon-adapters`; vertical ownership flows axon-adapters -> {dependency}"
            ));
        }
    }
}

fn normal_dependencies(
    root: &Path,
    crate_name: &str,
    violations: &mut Vec<String>,
) -> Option<toml::Table> {
    let manifest_path = root.join("crates").join(crate_name).join("Cargo.toml");
    let manifest_text = match std::fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(err) => {
            violations.push(format!("{crate_name}: failed to read Cargo.toml: {err}"));
            return None;
        }
    };
    let parsed = match toml::from_str::<toml::Table>(&manifest_text) {
        Ok(parsed) => parsed,
        Err(err) => {
            violations.push(format!("{crate_name}: failed to parse Cargo.toml: {err}"));
            return None;
        }
    };
    Some(
        parsed
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default(),
    )
}

fn check_modules(root: &Path, contract: &CrateContract, violations: &mut Vec<String>) {
    if contract.modules.is_empty() {
        return;
    }
    let src_dir = root.join("crates").join(contract.name).join("src");
    let lib_rs = match std::fs::read_to_string(src_dir.join("lib.rs")) {
        Ok(content) => content,
        Err(err) => {
            violations.push(format!(
                "{}: failed to read src/lib.rs: {err}",
                contract.name
            ));
            return;
        }
    };
    let declared = lib_rs
        .lines()
        .map(str::trim)
        .collect::<std::collections::BTreeSet<_>>();

    for module in contract.modules {
        if !src_dir.join(format!("{module}.rs")).is_file() {
            violations.push(format!(
                "{}: documented module `{module}.rs` does not exist under src/",
                contract.name
            ));
        }
        let expected = format!("pub mod {module};");
        if !declared.contains(expected.as_str()) {
            violations.push(format!(
                "{}: lib.rs does not declare `{expected}` (documented as a Public Module)",
                contract.name
            ));
        }
    }
}

fn check_forbidden_deps(root: &Path, contract: &CrateContract, violations: &mut Vec<String>) {
    if contract.forbidden_axon_deps.is_empty() {
        return;
    }
    let manifest_path = root.join("crates").join(contract.name).join("Cargo.toml");
    let manifest_text = match std::fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(err) => {
            violations.push(format!(
                "{}: failed to read Cargo.toml: {err}",
                contract.name
            ));
            return;
        }
    };
    let parsed = match toml::from_str::<toml::Table>(&manifest_text) {
        Ok(parsed) => parsed,
        Err(err) => {
            violations.push(format!(
                "{}: failed to parse Cargo.toml: {err}",
                contract.name
            ));
            return;
        }
    };
    let dep_tables = dependency_tables(&parsed);
    for forbidden in contract.forbidden_axon_deps {
        if dep_tables
            .iter()
            .any(|table| table.contains_key(*forbidden))
        {
            violations.push(format!(
                "{}: declares forbidden `{forbidden}` (see Dependencies Forbidden in its README contract)",
                contract.name
            ));
        }
    }
}

/// The top-level `[dependencies]` table plus any platform-specific
/// `[target.'cfg(...)'.dependencies]` tables — both are real (non-dev/build)
/// dependencies a crate can use to route around the forbidden-deps check.
fn dependency_tables(parsed: &toml::Table) -> Vec<&toml::Table> {
    let mut tables = Vec::new();
    if let Some(deps) = parsed.get("dependencies").and_then(toml::Value::as_table) {
        tables.push(deps);
    }
    if let Some(targets) = parsed.get("target").and_then(toml::Value::as_table) {
        for target_cfg in targets.values() {
            if let Some(deps) = target_cfg
                .as_table()
                .and_then(|cfg| cfg.get("dependencies"))
                .and_then(toml::Value::as_table)
            {
                tables.push(deps);
            }
        }
    }
    tables
}

#[cfg(test)]
#[path = "crate_contracts_tests.rs"]
mod tests;
