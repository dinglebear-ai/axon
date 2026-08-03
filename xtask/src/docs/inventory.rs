//! Required-living-documentation inventory check.
//!
//! Parses the ```text fenced tree under "## Required Living Documentation"
//! in `docs/README.md` and reports every named file that does not exist on
//! disk. This is a reporting-only check: it never creates stubs.

use std::path::Path;

use anyhow::{Result, bail};

const CONTRACT_PATH: &str = "docs/README.md";
const SECTION_HEADING: &str = "## Required Living Documentation";
const FORBIDDEN_STALE_ROOTS: &[&str] = &["docs/pipeline-unification"];

pub fn check(root: &Path) -> Result<()> {
    let stale_roots = FORBIDDEN_STALE_ROOTS
        .iter()
        .copied()
        .filter(|path| root.join(path).exists())
        .collect::<Vec<_>>();
    if !stale_roots.is_empty() {
        bail!(
            "docs inventory: obsolete living-doc root(s) must be removed:\n  {}",
            stale_roots.join("\n  ")
        );
    }

    let contract_path = root.join(CONTRACT_PATH);
    let content = std::fs::read_to_string(&contract_path)
        .map_err(|err| anyhow::anyhow!("docs inventory: failed to read {CONTRACT_PATH}: {err}"))?;
    let expected = parse_final_docs_tree(&content)?;

    let mut missing = Vec::new();
    for path in &expected {
        if !root.join(path).exists() {
            missing.push(path.clone());
        }
    }
    if !missing.is_empty() {
        let mut msg = format!(
            "docs inventory: {} required living file(s) from {CONTRACT_PATH} do not exist:\n",
            missing.len()
        );
        for path in &missing {
            msg.push_str(&format!("  {path}\n"));
        }
        bail!(msg);
    }
    println!(
        "docs inventory: all {} required living file(s) exist.",
        expected.len()
    );
    Ok(())
}

/// Parse the indentation-based `text` tree under the required-docs heading into a
/// flat list of repo-relative file paths. Directory lines (no `.` extension,
/// or ending in `/`) are used only to build ancestor prefixes; lines that are
/// purely descriptive placeholders (containing `...`) are skipped.
pub fn parse_final_docs_tree(contract: &str) -> Result<Vec<String>> {
    let Some(section_start) = contract.find(SECTION_HEADING) else {
        bail!("docs inventory: `{SECTION_HEADING}` section not found in {CONTRACT_PATH}");
    };
    let after_heading = &contract[section_start..];
    let Some(fence_start) = after_heading.find("```text") else {
        bail!("docs inventory: no ```text fence found under `{SECTION_HEADING}`");
    };
    let body_start = fence_start + "```text".len();
    let Some(fence_end_rel) = after_heading[body_start..].find("```") else {
        bail!("docs inventory: unterminated ```text fence under `{SECTION_HEADING}`");
    };
    let body = &after_heading[body_start..body_start + fence_end_rel];

    let mut files = Vec::new();
    // stack of (indent_width, name) for ancestor directories.
    let mut stack: Vec<(usize, String)> = Vec::new();
    for raw_line in body.lines() {
        if raw_line.trim().is_empty() {
            continue;
        }
        if raw_line.contains("...") {
            continue;
        }
        let indent = raw_line.len() - raw_line.trim_start().len();
        let name = raw_line.trim().trim_end_matches('/');
        if name.is_empty() {
            continue;
        }
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let is_dir = raw_line.trim_end().ends_with('/') || !name.contains('.');
        if is_dir {
            stack.push((indent, name.to_string()));
            // A directory itself is not a file to check for existence.
            continue;
        }
        let mut prefix: String = stack
            .iter()
            .map(|(_, n)| format!("{n}/"))
            .collect::<Vec<_>>()
            .join("");
        prefix.push_str(name);
        files.push(prefix);
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
