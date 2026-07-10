//! Docs-inventory-vs-Final-Docs-Tree diff.
//!
//! Parses the ```text fenced tree under "## Final Docs Tree" in
//! `documentation-contract.md` and reports every named file that does not
//! yet exist on disk. This is a reporting-only check: it never creates stub
//! docs, it only fails and lists what's missing.

use std::path::Path;

use anyhow::{Result, bail};

const CONTRACT_PATH: &str = "docs/pipeline-unification/delivery/documentation-contract.md";
const SECTION_HEADING: &str = "## Final Docs Tree";

pub fn check(root: &Path) -> Result<()> {
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
            "docs inventory: {} file(s) from the Final Docs Tree in {CONTRACT_PATH} do not exist yet:\n",
            missing.len()
        );
        for path in &missing {
            msg.push_str(&format!("  {path}\n"));
        }
        bail!(msg);
    }
    println!(
        "docs inventory: all {} file(s) from the Final Docs Tree exist.",
        expected.len()
    );
    Ok(())
}

/// Parse the indentation-based `text` tree under `## Final Docs Tree` into a
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
